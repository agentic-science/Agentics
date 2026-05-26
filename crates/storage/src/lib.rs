#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::enum_glob_use,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::wildcard_imports,
        reason = "unit tests use direct assertions and fixture indexing for concise failure diagnostics"
    )
)]

//! Durable object storage for submissions, private assets, logs, and challenge bundles.
//!
//! A storage key is an opaque object locator inside the configured storage
//! backend. Local development maps it onto a filesystem path, while hosted
//! deployments may map it to an S3 object key. Runner writable storage is a
//! separate local filesystem concern and is not represented by this crate.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use agentics_config::{Config, StorageBackend};
use agentics_domain::error::ServiceError;
pub use agentics_domain::storage::{StorageKey, StorageKeyError};
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub type Result<T> = std::result::Result<T, StorageError>;

mod tar_archive;
pub use tar_archive::{pack_directory_to_tar, unpack_tar_to_directory};

/// Local storage-layer failures before conversion to service/API errors.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("{0}")]
    InvalidKey(String),
    #[error("{0}")]
    SymlinkRejected(String),
    #[error("storage object already exists: {0}")]
    ObjectConflict(String),
    #[error("storage object not found: {0}")]
    ObjectNotFound(String),
    #[error("{label} exceeds storage byte limit: {actual} > {limit} bytes")]
    ObjectTooLarge {
        label: &'static str,
        actual: u64,
        limit: u64,
    },
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("storage invariant violated: {0}")]
    Internal(String),
    #[error("storage IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::InvalidKey(message) | StorageError::SymlinkRejected(message) => {
                ServiceError::BadRequest(message)
            }
            StorageError::ObjectTooLarge { .. } => ServiceError::BadRequest(error.to_string()),
            StorageError::ObjectConflict(_) => ServiceError::Conflict,
            StorageError::ObjectNotFound(_) => ServiceError::NotFound,
            StorageError::Internal(message) | StorageError::Backend(message) => {
                ServiceError::Internal(message)
            }
            StorageError::Io(error) => ServiceError::Io(error),
        }
    }
}

/// Storage write/read purpose with an explicit byte cap.
#[derive(Debug, Clone, Copy)]
pub struct StorageWriteIntent {
    label: &'static str,
    max_bytes: u64,
}

impl StorageWriteIntent {
    /// Create a write intent with a caller-owned byte limit.
    pub const fn new(label: &'static str, max_bytes: u64) -> Self {
        Self { label, max_bytes }
    }

    /// User-facing purpose label.
    pub const fn label(self) -> &'static str {
        self.label
    }

    /// Maximum bytes allowed for this object.
    pub const fn max_bytes(self) -> u64 {
        self.max_bytes
    }

    /// Verify a byte length against this intent.
    pub fn ensure_len(self, actual: u64) -> Result<()> {
        if actual > self.max_bytes {
            return Err(StorageError::ObjectTooLarge {
                label: self.label,
                actual,
                limit: self.max_bytes,
            });
        }
        Ok(())
    }
}

/// Minimal object-storage interface used by services and workers.
#[async_trait]
pub trait Storage: std::fmt::Debug + Send + Sync {
    /// Store content at a storage-relative key. Implementations must not overwrite.
    async fn put(
        &self,
        key: &StorageKey,
        content: &[u8],
        intent: StorageWriteIntent,
    ) -> Result<StorageKey>;

    /// Store a local file at a storage-relative key. Implementations must not overwrite.
    async fn put_file(
        &self,
        key: &StorageKey,
        source: &Path,
        intent: StorageWriteIntent,
    ) -> Result<StorageKey>;

    /// Promote a temporary object to a durable storage-relative key without overwriting.
    async fn promote(
        &self,
        temporary_key: &StorageKey,
        durable_key: &StorageKey,
    ) -> Result<StorageKey>;

    /// Read content from a storage-relative key with an object-size cap.
    async fn get(&self, key: &StorageKey, intent: StorageWriteIntent) -> Result<Vec<u8>>;

    /// Download an object into a local file with an object-size cap.
    async fn get_to_file(
        &self,
        key: &StorageKey,
        destination: &Path,
        intent: StorageWriteIntent,
    ) -> Result<()>;

    /// Return whether a storage-relative key exists.
    async fn exists(&self, key: &StorageKey) -> Result<bool>;

    /// Delete a storage-relative key if it exists.
    async fn delete(&self, key: &StorageKey) -> Result<()>;

    /// List object keys below a storage-relative prefix.
    async fn list_prefix(&self, prefix: &StorageKey) -> Result<Vec<StorageKey>>;

    /// Delete every object below a storage-relative prefix older than the cutoff.
    async fn delete_prefix_older_than(
        &self,
        prefix: &StorageKey,
        older_than: SystemTime,
    ) -> Result<u64>;

    /// Delete every object below a storage-relative prefix.
    async fn delete_prefix(&self, prefix: &StorageKey) -> Result<u64> {
        let keys = self.list_prefix(prefix).await?;
        let mut deleted = 0u64;
        for key in keys {
            self.delete(&key).await?;
            deleted = deleted.checked_add(1).ok_or_else(|| {
                StorageError::Internal("deleted object count overflow".to_string())
            })?;
        }
        Ok(deleted)
    }
}

/// Build the configured durable storage backend.
pub async fn build_storage(config: &Config) -> anyhow::Result<Arc<dyn Storage>> {
    config.validate_object_storage_config()?;
    match config.storage_backend {
        StorageBackend::Local => Ok(Arc::new(LocalStorage::new(&config.storage_root))),
        StorageBackend::S3 => Ok(Arc::new(S3Storage::from_config(config).await?)),
    }
}

/// Return the host-local work root for object storage staging and materialization.
pub fn storage_work_root(config: &Config) -> Result<PathBuf> {
    let root = config
        .storage_work_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("agentics-storage-work"));
    if !root.is_absolute() {
        return Err(StorageError::InvalidKey(
            "AGENTICS_STORAGE_WORK_ROOT must be an absolute path".to_string(),
        ));
    }
    Ok(root)
}

/// Filesystem-backed storage rooted at a configured directory.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    /// Create local storage rooted at `root`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn resolve(&self, key: &StorageKey) -> (PathBuf, PathBuf) {
        let key_path = key.as_path().to_path_buf();
        (self.root.join(&key_path), key_path)
    }

    fn reject_symlink_prefixes(&self, key: &Path) -> Result<()> {
        let Some(parent) = key.parent() else {
            return Ok(());
        };
        let mut current = self.root.clone();
        for component in parent.components() {
            let Component::Normal(part) = component else {
                return Err(invalid_storage_key());
            };
            current.push(part);
            if let Ok(metadata) = fs::symlink_metadata(&current)
                && metadata.file_type().is_symlink()
            {
                return Err(StorageError::SymlinkRejected(format!(
                    "storage key resolves through a symlink: {}",
                    current.display()
                )));
            }
        }
        Ok(())
    }

    fn reject_symlink_object(&self, full: &Path) -> Result<()> {
        if let Ok(metadata) = fs::symlink_metadata(full)
            && metadata.file_type().is_symlink()
        {
            return Err(StorageError::SymlinkRejected(format!(
                "storage key resolves to a symlink: {}",
                full.display()
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn put(
        &self,
        key: &StorageKey,
        content: &[u8],
        intent: StorageWriteIntent,
    ) -> Result<StorageKey> {
        let len = u64::try_from(content.len()).map_err(|_| StorageError::ObjectTooLarge {
            label: intent.label(),
            actual: u64::MAX,
            limit: intent.max_bytes(),
        })?;
        intent.ensure_len(len)?;
        let (full, key_path) = self.resolve(key);
        put_local_bytes(self, key, &full, &key_path, content).await?;
        Ok(key.clone())
    }

    async fn put_file(
        &self,
        key: &StorageKey,
        source: &Path,
        intent: StorageWriteIntent,
    ) -> Result<StorageKey> {
        let len = tokio::fs::metadata(source).await?.len();
        intent.ensure_len(len)?;
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        let parent = full
            .parent()
            .ok_or_else(|| StorageError::Internal("storage key has no parent".to_string()))?;
        tokio::fs::create_dir_all(parent).await?;
        self.reject_symlink_prefixes(&key_path)?;
        self.reject_symlink_object(&full)?;
        if tokio::fs::try_exists(&full).await? {
            return Err(StorageError::ObjectConflict(key.to_string()));
        }
        let temporary_full = parent.join(format!(".agentics-write-{}", uuid::Uuid::new_v4()));
        let copy_result = async {
            let copied = tokio::fs::copy(source, &temporary_full).await?;
            if copied != len {
                return Err(StorageError::Internal(format!(
                    "local source file length changed while storing {key}: expected {len}, copied {copied}"
                )));
            }
            self.reject_symlink_prefixes(&key_path)?;
            self.reject_symlink_object(&full)?;
            finalize_local_temp_without_overwrite(&temporary_full, &full, key.as_str()).await?;
            Ok::<(), StorageError>(())
        }
        .await;
        cleanup_temp_file_on_error(copy_result, &temporary_full).await?;
        Ok(key.clone())
    }

    async fn promote(
        &self,
        temporary_key: &StorageKey,
        durable_key: &StorageKey,
    ) -> Result<StorageKey> {
        let (temporary_full, temporary_key_path) = self.resolve(temporary_key);
        let (durable_full, durable_key_path) = self.resolve(durable_key);
        self.reject_symlink_prefixes(&temporary_key_path)?;
        self.reject_symlink_object(&temporary_full)?;
        self.reject_symlink_prefixes(&durable_key_path)?;
        if let Some(parent) = durable_full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        self.reject_symlink_prefixes(&durable_key_path)?;
        self.reject_symlink_object(&durable_full)?;

        if !tokio::fs::try_exists(&temporary_full).await? {
            return Err(StorageError::ObjectNotFound(temporary_key.to_string()));
        }
        if tokio::fs::try_exists(&durable_full).await? {
            return Err(StorageError::ObjectConflict(durable_key.to_string()));
        }
        match tokio::fs::hard_link(&temporary_full, &durable_full).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StorageError::ObjectConflict(durable_key.to_string()));
            }
            Err(error) => return Err(error.into()),
        }
        if let Err(error) = tokio::fs::remove_file(&temporary_full).await {
            let cleanup_result = tokio::fs::remove_file(&durable_full).await;
            if let Err(cleanup_error) = cleanup_result
                && cleanup_error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(cleanup_error.into());
            }
            return Err(error.into());
        }
        Ok(durable_key.clone())
    }

    async fn get(&self, key: &StorageKey, intent: StorageWriteIntent) -> Result<Vec<u8>> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        self.reject_symlink_object(&full)?;
        let metadata = match tokio::fs::metadata(&full).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::ObjectNotFound(key.to_string()));
            }
            Err(error) => return Err(error.into()),
        };
        intent.ensure_len(metadata.len())?;
        let bytes = tokio::fs::read(&full).await?;
        let actual = u64::try_from(bytes.len()).map_err(|_| StorageError::ObjectTooLarge {
            label: intent.label(),
            actual: u64::MAX,
            limit: intent.max_bytes(),
        })?;
        intent.ensure_len(actual)?;
        if actual != metadata.len() {
            return Err(StorageError::Internal(format!(
                "local object length changed while reading {key}: expected {}, read {actual}",
                metadata.len()
            )));
        }
        Ok(bytes)
    }

    async fn get_to_file(
        &self,
        key: &StorageKey,
        destination: &Path,
        intent: StorageWriteIntent,
    ) -> Result<()> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        self.reject_symlink_object(&full)?;
        let metadata = match tokio::fs::metadata(&full).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::ObjectNotFound(key.to_string()));
            }
            Err(error) => return Err(error.into()),
        };
        intent.ensure_len(metadata.len())?;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let temporary =
            destination.with_extension(format!("agentics-download-{}", uuid::Uuid::new_v4()));
        let write_result = async {
            if tokio::fs::try_exists(destination).await? {
                return Err(StorageError::ObjectConflict(
                    destination.display().to_string(),
                ));
            }
            let mut source = tokio::fs::File::open(&full).await?;
            let mut file = create_private_file(&temporary).await?;
            let copied = tokio::io::copy(&mut source, &mut file).await?;
            if copied != metadata.len() {
                return Err(StorageError::Internal(format!(
                    "local object length changed while downloading {key}: expected {}, copied {copied}",
                    metadata.len()
                )));
            }
            file.flush().await?;
            drop(file);
            finalize_local_temp_without_overwrite(
                &temporary,
                destination,
                &destination.display().to_string(),
            )
            .await?;
            Ok::<(), StorageError>(())
        }
        .await;
        cleanup_temp_file_on_error(write_result, &temporary).await
    }

    async fn exists(&self, key: &StorageKey) -> Result<bool> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        if let Ok(metadata) = tokio::fs::symlink_metadata(&full).await {
            return Ok(!metadata.file_type().is_symlink());
        }
        Ok(false)
    }

    async fn delete(&self, key: &StorageKey) -> Result<()> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        self.reject_symlink_object(&full)?;
        match tokio::fs::remove_file(&full).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    async fn list_prefix(&self, prefix: &StorageKey) -> Result<Vec<StorageKey>> {
        let root = self.root.clone();
        let prefix = prefix.as_str().to_string();
        tokio::task::spawn_blocking(move || list_local_prefix(&root, &prefix))
            .await
            .map_err(|e| StorageError::Internal(e.to_string()))?
    }

    async fn delete_prefix_older_than(
        &self,
        prefix: &StorageKey,
        older_than: SystemTime,
    ) -> Result<u64> {
        let root = self.root.clone();
        let prefix = prefix.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            delete_local_prefix_older_than(&root, &prefix, older_than)
        })
        .await
        .map_err(|e| StorageError::Internal(e.to_string()))?
    }
}

/// S3-compatible durable object storage.
#[derive(Debug, Clone)]
pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: Option<String>,
    work_root: PathBuf,
}

/// Connection settings for S3-compatible durable object storage.
#[derive(Debug, Clone)]
pub struct S3StorageOptions {
    pub bucket: String,
    pub prefix: Option<String>,
    pub region: String,
    pub endpoint_url: Option<url::Url>,
    pub force_path_style: bool,
    pub work_root: Option<PathBuf>,
}

impl S3Storage {
    /// Build an S3 storage client from runtime configuration.
    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        Self::from_options(S3StorageOptions {
            bucket: config
                .s3_bucket
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("AGENTICS_S3_BUCKET must be set"))?
                .to_string(),
            prefix: config.s3_prefix.clone(),
            region: config.s3_region.clone(),
            endpoint_url: config.s3_endpoint_url.clone(),
            force_path_style: config.s3_force_path_style,
            work_root: Some(storage_work_root(config)?),
        })
        .await
    }

    /// Build an S3 storage client from explicit options.
    pub async fn from_options(options: S3StorageOptions) -> anyhow::Result<Self> {
        let bucket = options.bucket.trim().to_string();
        if bucket.is_empty() {
            anyhow::bail!("S3 bucket must be set");
        }
        let region = Region::new(options.region.trim().to_string());
        let mut loader = aws_config::defaults(BehaviorVersion::latest()).region(region);
        if let Some(endpoint) = options.endpoint_url.as_ref() {
            loader = loader.endpoint_url(endpoint.as_str());
        }
        let shared_config = loader.load().await;
        let mut s3_config = aws_sdk_s3::config::Builder::from(&shared_config);
        if options.force_path_style {
            s3_config = s3_config.force_path_style(true);
        }
        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(s3_config.build()),
            bucket,
            prefix: normalized_s3_prefix(options.prefix.as_deref())?,
            work_root: options
                .work_root
                .unwrap_or_else(|| std::env::temp_dir().join("agentics-storage-work")),
        })
    }

    fn object_key(&self, key: &StorageKey) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}/{}", key.as_str()),
            None => key.as_str().to_string(),
        }
    }

    async fn object_len(&self, key: &StorageKey) -> Result<Option<u64>> {
        let object_key = self.object_key(key);
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
        {
            Ok(output) => {
                let len = output.content_length().unwrap_or(0);
                u64::try_from(len)
                    .map(Some)
                    .map_err(|_| StorageError::Internal("negative S3 content length".to_string()))
            }
            Err(error) if s3_error_is_not_found(&error) => Ok(None),
            Err(error) => Err(StorageError::Backend(format!("{error:?}"))),
        }
    }

    async fn verify_object_len(&self, key: &StorageKey, expected: u64) -> Result<()> {
        let actual = self
            .object_len(key)
            .await?
            .ok_or_else(|| StorageError::ObjectNotFound(key.to_string()))?;
        if actual != expected {
            return Err(StorageError::Internal(format!(
                "S3 object length mismatch for {key}: expected {expected}, got {actual}"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn put(
        &self,
        key: &StorageKey,
        content: &[u8],
        intent: StorageWriteIntent,
    ) -> Result<StorageKey> {
        let len = u64::try_from(content.len()).map_err(|_| StorageError::ObjectTooLarge {
            label: intent.label(),
            actual: u64::MAX,
            limit: intent.max_bytes(),
        })?;
        intent.ensure_len(len)?;
        let object_key = self.object_key(key);
        let put_request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(ByteStream::from(content.to_vec()))
            .if_none_match("*");
        let put_result = put_request.send().await;
        if let Err(error) = put_result {
            if s3_error_is_conflict(&error) {
                return Err(StorageError::ObjectConflict(key.to_string()));
            }
            return Err(StorageError::Backend(format!("{error:?}")));
        }
        if let Err(error) = self.verify_object_len(key, len).await {
            drop(self.delete(key).await);
            return Err(error);
        }
        Ok(key.clone())
    }

    async fn put_file(
        &self,
        key: &StorageKey,
        source: &Path,
        intent: StorageWriteIntent,
    ) -> Result<StorageKey> {
        let len = tokio::fs::metadata(source).await?.len();
        intent.ensure_len(len)?;
        let object_key = self.object_key(key);
        let body = ByteStream::from_path(source)
            .await
            .map_err(|e| StorageError::Io(std::io::Error::other(e)))?;
        let put_request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(body)
            .if_none_match("*");
        let put_result = put_request.send().await;
        if let Err(error) = put_result {
            if s3_error_is_conflict(&error) {
                return Err(StorageError::ObjectConflict(key.to_string()));
            }
            return Err(StorageError::Backend(format!("{error:?}")));
        }
        if let Err(error) = self.verify_object_len(key, len).await {
            drop(self.delete(key).await);
            return Err(error);
        }
        Ok(key.clone())
    }

    async fn promote(
        &self,
        temporary_key: &StorageKey,
        durable_key: &StorageKey,
    ) -> Result<StorageKey> {
        let source_len = self
            .object_len(temporary_key)
            .await?
            .ok_or_else(|| StorageError::ObjectNotFound(temporary_key.to_string()))?;
        ensure_private_directory(&self.work_root).await?;
        let local_temp = self
            .work_root
            .join(format!("agentics-s3-promote-{}", uuid::Uuid::new_v4()));
        let intent = StorageWriteIntent::new("temporary storage object", source_len);
        let promote_result = async {
            self.get_to_file(temporary_key, &local_temp, intent).await?;
            self.put_file(durable_key, &local_temp, intent).await?;
            self.delete(temporary_key).await?;
            Ok(durable_key.clone())
        }
        .await;
        let cleanup_result = tokio::fs::remove_file(&local_temp).await;
        match cleanup_result {
            Ok(()) => promote_result,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => promote_result,
            Err(_cleanup_error) => promote_result,
        }
    }

    async fn get(&self, key: &StorageKey, intent: StorageWriteIntent) -> Result<Vec<u8>> {
        let object_len = self
            .object_len(key)
            .await?
            .ok_or_else(|| StorageError::ObjectNotFound(key.to_string()))?;
        intent.ensure_len(object_len)?;
        let object_key = self.object_key(key);
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        let mut body = output.body.into_async_read();
        let mut bytes = Vec::new();
        let mut read_total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let len = body
                .read(&mut buffer)
                .await
                .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
            if len == 0 {
                break;
            }
            let len_u64 = u64::try_from(len).map_err(|_| {
                StorageError::Internal("S3 download chunk length overflow".to_string())
            })?;
            read_total =
                read_total
                    .checked_add(len_u64)
                    .ok_or_else(|| StorageError::ObjectTooLarge {
                        label: intent.label(),
                        actual: u64::MAX,
                        limit: intent.max_bytes(),
                    })?;
            intent.ensure_len(read_total)?;
            let chunk = buffer.get(..len).ok_or_else(|| {
                StorageError::Internal("S3 download chunk range invalid".to_string())
            })?;
            bytes.extend_from_slice(chunk);
        }
        if read_total != object_len {
            return Err(StorageError::Internal(format!(
                "S3 object length mismatch while reading {key}: expected {object_len}, read {read_total}"
            )));
        }
        Ok(bytes)
    }

    async fn get_to_file(
        &self,
        key: &StorageKey,
        destination: &Path,
        intent: StorageWriteIntent,
    ) -> Result<()> {
        let expected_len = self
            .object_len(key)
            .await?
            .ok_or_else(|| StorageError::ObjectNotFound(key.to_string()))?;
        intent.ensure_len(expected_len)?;
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.object_key(key))
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let temporary =
            destination.with_extension(format!("agentics-download-{}", uuid::Uuid::new_v4()));
        let write_result = async {
            if tokio::fs::try_exists(destination).await? {
                return Err(StorageError::ObjectConflict(
                    destination.display().to_string(),
                ));
            }
            let mut file = create_private_file(&temporary).await?;
            let mut body = output.body.into_async_read();
            let mut written = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let len = body
                    .read(&mut buffer)
                    .await
                    .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
                if len == 0 {
                    break;
                }
                let len_u64 = u64::try_from(len).map_err(|_| {
                    StorageError::Internal("S3 download chunk length overflow".to_string())
                })?;
                written = written.checked_add(len_u64).ok_or_else(|| {
                    StorageError::ObjectTooLarge {
                        label: intent.label(),
                        actual: u64::MAX,
                        limit: intent.max_bytes(),
                    }
                })?;
                intent.ensure_len(written)?;
                let chunk = buffer.get(..len).ok_or_else(|| {
                    StorageError::Internal("S3 download chunk range invalid".to_string())
                })?;
                file.write_all(chunk).await?;
            }
            if written != expected_len {
                return Err(StorageError::Internal(format!(
                    "S3 object length mismatch while downloading {key}: expected {expected_len}, wrote {written}"
                )));
            }
            file.flush().await?;
            drop(file);
            finalize_local_temp_without_overwrite(
                &temporary,
                destination,
                &destination.display().to_string(),
            )
            .await?;
            Ok::<(), StorageError>(())
        }
        .await;
        cleanup_temp_file_on_error(write_result, &temporary).await
    }

    async fn exists(&self, key: &StorageKey) -> Result<bool> {
        self.object_len(key).await.map(|value| value.is_some())
    }

    async fn delete(&self, key: &StorageKey) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.object_key(key))
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        Ok(())
    }

    async fn list_prefix(&self, prefix: &StorageKey) -> Result<Vec<StorageKey>> {
        let mut continuation_token = None;
        let mut keys = Vec::new();
        let physical_prefix = self.object_key(prefix);
        loop {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&physical_prefix)
                .set_continuation_token(continuation_token.clone())
                .send()
                .await
                .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
            for object in output.contents() {
                if let Some(key) = object.key() {
                    let logical_key = self.strip_prefix(key)?;
                    if storage_key_is_within_prefix(&logical_key, prefix) {
                        keys.push(logical_key);
                    }
                }
            }
            continuation_token = output.next_continuation_token().map(ToOwned::to_owned);
            if continuation_token.is_none() {
                break;
            }
        }
        Ok(keys)
    }

    async fn delete_prefix_older_than(
        &self,
        prefix: &StorageKey,
        older_than: SystemTime,
    ) -> Result<u64> {
        let mut continuation_token = None;
        let physical_prefix = self.object_key(prefix);
        let mut keys_to_delete = Vec::new();
        loop {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&physical_prefix)
                .set_continuation_token(continuation_token.clone())
                .send()
                .await
                .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
            for object in output.contents() {
                let Some(key) = object.key() else {
                    continue;
                };
                let logical_key = self.strip_prefix(key)?;
                if !storage_key_is_within_prefix(&logical_key, prefix) {
                    continue;
                }
                let Some(last_modified) = object.last_modified() else {
                    continue;
                };
                let modified = SystemTime::try_from(*last_modified)
                    .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
                if modified < older_than {
                    keys_to_delete.push(logical_key);
                }
            }
            continuation_token = output.next_continuation_token().map(ToOwned::to_owned);
            if continuation_token.is_none() {
                break;
            }
        }
        let mut deleted = 0u64;
        for key in keys_to_delete {
            self.delete(&key).await?;
            deleted = deleted.checked_add(1).ok_or_else(|| {
                StorageError::Internal("deleted object count overflow".to_string())
            })?;
        }
        Ok(deleted)
    }
}

impl S3Storage {
    fn strip_prefix(&self, physical_key: &str) -> Result<StorageKey> {
        let logical = match &self.prefix {
            Some(prefix) => physical_key
                .strip_prefix(prefix)
                .and_then(|value| value.strip_prefix('/'))
                .ok_or_else(|| {
                    StorageError::Internal(format!(
                        "S3 list returned key outside configured prefix: {physical_key}"
                    ))
                })?,
            None => physical_key,
        };
        StorageKey::try_new(logical).map_err(|e| StorageError::InvalidKey(e.to_string()))
    }

    /// Create the configured bucket when a test harness owns the object store.
    ///
    /// Application startup intentionally does not create buckets; production
    /// Compose and external S3 deployments provision storage outside the app.
    pub async fn create_bucket_if_missing_for_tests(&self) -> Result<()> {
        let create_bucket = self
            .client
            .create_bucket()
            .bucket(&self.bucket)
            .send()
            .await;
        if let Err(error) = create_bucket {
            let text = format!("{error} {error:?}");
            if !(text.contains("BucketAlreadyOwnedByYou")
                || text.contains("BucketAlreadyExists")
                || text.contains("Conflict")
                || text.contains("409"))
            {
                return Err(StorageError::Backend(format!("{error:?}")));
            }
        }
        Ok(())
    }
}

async fn put_local_bytes(
    storage: &LocalStorage,
    key: &StorageKey,
    full: &Path,
    key_path: &Path,
    content: &[u8],
) -> Result<()> {
    storage.reject_symlink_prefixes(key_path)?;
    let parent = full
        .parent()
        .ok_or_else(|| StorageError::Internal("storage key has no parent".to_string()))?;
    tokio::fs::create_dir_all(parent).await?;
    storage.reject_symlink_prefixes(key_path)?;
    storage.reject_symlink_object(full)?;
    if tokio::fs::try_exists(full).await? {
        return Err(StorageError::ObjectConflict(key.to_string()));
    }
    let temporary_full = parent.join(format!(".agentics-write-{}", uuid::Uuid::new_v4()));
    let write_result = async {
        let mut temporary = create_private_file(&temporary_full).await?;
        temporary.write_all(content).await?;
        temporary.flush().await?;
        drop(temporary);
        storage.reject_symlink_prefixes(key_path)?;
        storage.reject_symlink_object(full)?;
        finalize_local_temp_without_overwrite(&temporary_full, full, key.as_str()).await?;
        Ok::<(), StorageError>(())
    }
    .await;
    cleanup_temp_file_on_error(write_result, &temporary_full).await
}

async fn ensure_private_directory(path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || ensure_private_directory_sync(&path))
        .await
        .map_err(|e| StorageError::Internal(e.to_string()))?
}

#[cfg(unix)]
fn ensure_private_directory_sync(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(StorageError::InvalidKey(format!(
            "storage work root is not a directory: {}",
            path.display()
        )));
    }
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & !0o077))?;
    }
    let tightened_mode = std::fs::metadata(path)?.permissions().mode();
    if tightened_mode & 0o077 != 0 {
        return Err(StorageError::InvalidKey(format!(
            "storage work root must not be group/world accessible: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_directory_sync(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

async fn create_private_file(path: &Path) -> Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    Ok(options.open(path).await?)
}

async fn finalize_local_temp_without_overwrite(
    temporary: &Path,
    destination: &Path,
    conflict_label: &str,
) -> Result<()> {
    match tokio::fs::hard_link(temporary, destination).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(StorageError::ObjectConflict(conflict_label.to_string()));
        }
        Err(error) => return Err(error.into()),
    }
    if let Err(error) = tokio::fs::remove_file(temporary).await {
        let cleanup = tokio::fs::remove_file(destination).await;
        if let Err(cleanup_error) = cleanup
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(cleanup_error.into());
        }
        return Err(error.into());
    }
    Ok(())
}

async fn cleanup_temp_file_on_error(result: Result<()>, temporary: &Path) -> Result<()> {
    if let Err(error) = result {
        if let Err(cleanup_error) = tokio::fs::remove_file(temporary).await
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(cleanup_error.into());
        }
        return Err(error);
    }
    Ok(())
}

fn list_local_prefix(root: &Path, prefix: &str) -> Result<Vec<StorageKey>> {
    let start = root.join(prefix);
    if !start.exists() {
        return Ok(Vec::new());
    }
    let mut keys = Vec::new();
    let mut stack = vec![start];
    while let Some(path) = stack.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                stack.push(entry?.path());
            }
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| StorageError::Internal(e.to_string()))?;
            let key = relative
                .to_str()
                .ok_or_else(|| StorageError::InvalidKey("storage key is not UTF-8".to_string()))?
                .replace('\\', "/");
            keys.push(
                StorageKey::try_new(&key).map_err(|e| StorageError::InvalidKey(e.to_string()))?,
            );
        }
    }
    keys.sort();
    Ok(keys)
}

fn delete_local_prefix_older_than(
    root: &Path,
    prefix: &str,
    older_than: SystemTime,
) -> Result<u64> {
    let keys = list_local_prefix(root, prefix)?;
    let mut deleted = 0u64;
    for key in keys {
        let path = root.join(key.as_path());
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified()?;
        if modified < older_than {
            fs::remove_file(&path)?;
            deleted = deleted.checked_add(1).ok_or_else(|| {
                StorageError::Internal("deleted object count overflow".to_string())
            })?;
        }
    }
    Ok(deleted)
}

fn storage_key_is_within_prefix(key: &StorageKey, prefix: &StorageKey) -> bool {
    key == prefix
        || key
            .as_str()
            .strip_prefix(prefix.as_str())
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn normalized_s3_prefix(value: Option<&str>) -> Result<Option<String>> {
    let Some(prefix) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    StorageKey::try_new(prefix)
        .map_err(|e| StorageError::InvalidKey(e.to_string()))
        .map(|key| Some(key.to_string()))
}

fn s3_error_is_not_found<E: std::fmt::Debug + std::fmt::Display>(error: &E) -> bool {
    let text = format!("{error} {error:?}");
    text.contains("NotFound")
        || text.contains("NoSuchKey")
        || text.contains("NoSuchBucket")
        || text.contains("404")
}

fn s3_error_is_conflict<E: std::fmt::Debug + std::fmt::Display>(error: &E) -> bool {
    let text = format!("{error} {error:?}");
    text.contains("PreconditionFailed") || text.contains("AlreadyExists") || text.contains("412")
}

fn invalid_storage_key() -> StorageError {
    StorageError::InvalidKey(
        "storage key must be a non-empty relative path with safe ASCII components and no `.` or `..` components".to_string(),
    )
}

#[cfg(test)]
mod tests;
