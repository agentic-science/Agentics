use std::path::{Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::factory::storage_work_root;
use crate::fs_utils::{
    cleanup_temp_file_on_error, create_private_file, ensure_private_directory,
    finalize_local_temp_without_overwrite,
};
use crate::{Result, Storage, StorageError, StorageKey, StorageWriteIntent};

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
            work_root: storage_work_root(options.work_root.as_deref())?,
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
