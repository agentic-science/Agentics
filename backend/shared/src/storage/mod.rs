//! Storage abstraction for uploaded solution_submissions and runner logs.
//!
//! A storage key is an opaque object locator inside the configured storage
//! backend. It is intentionally not called a path or URI: local development may
//! map it onto a filesystem path, but callers must not rely on host filesystem
//! semantics, absolute paths, parent traversal, schemes, authorities, or URL
//! parsing. Storage backends own the mapping from `StorageKey` to physical
//! storage.

use std::borrow::Cow;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::AppError;

pub type Result<T> = std::result::Result<T, AppError>;

/// Opaque object key relative to the configured Agentics storage namespace.
///
/// `StorageKey` values identify stored blobs such as solution archives, private
/// challenge assets, shortlist uploads, and runner logs. The allowed syntax is
/// deliberately narrower than a filesystem path so keys can be safely resolved
/// by local and future remote storage backends.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    /// Parse and validate a storage-relative object key.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self> {
        validate_storage_key(value.as_ref()).map(Self)
    }

    /// Borrow the storage key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns path in the representation required by callers.
    fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl fmt::Display for StorageKey {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StorageKey {
    type Err = AppError;

    /// Handles from str for this module.
    fn from_str(value: &str) -> Result<Self> {
        Self::try_new(value)
    }
}

impl Serialize for StorageKey {
    /// Handles serialize for this module.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StorageKey {
    /// Handles deserialize for this module.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for StorageKey {
    /// Handles inline schema for this module.
    fn inline_schema() -> bool {
        true
    }

    /// Handles schema name for this module.
    fn schema_name() -> Cow<'static, str> {
        "StorageKey".into()
    }

    /// Handles json schema for this module.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$"
        })
    }
}

/// Minimal object-storage interface used by handlers and workers.
#[async_trait]
pub trait Storage: std::fmt::Debug + Send + Sync {
    /// Store content at a storage-relative key and return that opaque key.
    async fn put(&self, key: &StorageKey, content: &[u8]) -> Result<StorageKey>;
    /// Atomically promote a temporary object to a durable storage-relative key.
    ///
    /// Implementations must not overwrite an existing durable object.
    async fn promote(
        &self,
        temporary_key: &StorageKey,
        durable_key: &StorageKey,
    ) -> Result<StorageKey>;
    /// Read content from a storage-relative key.
    async fn get(&self, key: &StorageKey) -> Result<Vec<u8>>;
    /// Return whether a storage-relative key exists.
    async fn exists(&self, key: &StorageKey) -> Result<bool>;
    /// Delete a storage-relative key if it exists.
    async fn delete(&self, key: &StorageKey) -> Result<()>;
}

/// Filesystem-backed storage rooted at a configured directory.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    root: std::path::PathBuf,
}

impl LocalStorage {
    /// Create local storage rooted at `root`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Handles resolve for this module.
    fn resolve(&self, key: &StorageKey) -> (PathBuf, PathBuf) {
        let key_path = key.as_path().to_path_buf();
        (self.root.join(&key_path), key_path)
    }

    /// Handles reject symlink prefixes for this module.
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
            if let Ok(metadata) = std::fs::symlink_metadata(&current)
                && metadata.file_type().is_symlink()
            {
                return Err(AppError::BadRequest(format!(
                    "storage key resolves through a symlink: {}",
                    current.display()
                )));
            }
        }
        Ok(())
    }

    /// Handles reject symlink object for this module.
    fn reject_symlink_object(&self, full: &Path) -> Result<()> {
        if let Ok(metadata) = std::fs::symlink_metadata(full)
            && metadata.file_type().is_symlink()
        {
            return Err(AppError::BadRequest(format!(
                "storage key resolves to a symlink: {}",
                full.display()
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl Storage for LocalStorage {
    /// Handles put for this module.
    async fn put(&self, key: &StorageKey, content: &[u8]) -> Result<StorageKey> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        self.reject_symlink_prefixes(&key_path)?;
        self.reject_symlink_object(&full)?;
        tokio::fs::write(&full, content).await?;
        Ok(key.clone())
    }

    /// Handles promote for this module.
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

        tokio::fs::hard_link(&temporary_full, &durable_full).await?;
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

    /// Handles get for this module.
    async fn get(&self, key: &StorageKey) -> Result<Vec<u8>> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        self.reject_symlink_object(&full)?;
        Ok(tokio::fs::read(&full).await?)
    }

    /// Handles exists for this module.
    async fn exists(&self, key: &StorageKey) -> Result<bool> {
        let (full, key_path) = self.resolve(key);
        self.reject_symlink_prefixes(&key_path)?;
        if let Ok(metadata) = tokio::fs::symlink_metadata(&full).await {
            return Ok(!metadata.file_type().is_symlink());
        }
        Ok(false)
    }

    /// Handles delete for this module.
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
}

/// Validates storage key invariants for this contract.
fn validate_storage_key(value: &str) -> Result<String> {
    if value.is_empty()
        || value.trim() != value
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(invalid_storage_key());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(invalid_storage_key());
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let Some(part) = part.to_str() else {
                    return Err(invalid_storage_key());
                };
                if part.is_empty()
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    })
                {
                    return Err(invalid_storage_key());
                }
                parts.push(part);
            }
            _ => return Err(invalid_storage_key()),
        }
    }
    if parts.is_empty() || parts.join("/") != value {
        return Err(invalid_storage_key());
    }
    Ok(value.to_string())
}

/// Handles invalid storage key for this module.
fn invalid_storage_key() -> AppError {
    AppError::BadRequest(
        "storage key must be a non-empty relative path with safe ASCII components and no `.` or `..` components".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{LocalStorage, Storage, StorageKey};
    use crate::error::AppError;

    /// Verifies that local storage returns relative keys.
    #[tokio::test]
    async fn local_storage_returns_relative_keys() {
        let root = temp_storage_root("relative-keys");
        let storage = LocalStorage::new(&root);

        let key = storage
            .put(&storage_key("objects/a.txt"), b"hello")
            .await
            .expect("put should succeed");

        assert_eq!(key.as_str(), "objects/a.txt");
        assert_eq!(
            storage.get(&key).await.expect("get should succeed"),
            b"hello"
        );
        assert!(root.join("objects/a.txt").is_file());
        drop(std::fs::remove_dir_all(root));
    }

    /// Verifies that local storage rejects absolute and parent keys.
    #[tokio::test]
    async fn local_storage_rejects_absolute_and_parent_keys() {
        let root = temp_storage_root("bad-keys");

        for key in [
            "",
            "/tmp/escape.txt",
            "../escape.txt",
            "a/../escape.txt",
            ".",
            "a//b",
            "a\\b",
            "a b",
        ] {
            let result = StorageKey::try_new(key);
            assert!(matches!(result, Err(AppError::BadRequest(_))));
        }
        drop(std::fs::remove_dir_all(root));
    }

    #[cfg(unix)]
    /// Verifies that local storage rejects symlink prefixes.
    #[tokio::test]
    async fn local_storage_rejects_symlink_prefixes() {
        let root = temp_storage_root("symlink-root");
        let outside = temp_storage_root("symlink-outside");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("failed to create symlink");
        let storage = LocalStorage::new(&root);

        let result = storage.put(&storage_key("link/escape.txt"), b"bad").await;

        assert!(matches!(result, Err(AppError::BadRequest(_))));
        assert!(!outside.join("escape.txt").exists());
        drop(std::fs::remove_dir_all(root));
        drop(std::fs::remove_dir_all(outside));
    }

    /// Verifies that local storage promotes without overwriting.
    #[tokio::test]
    async fn local_storage_promotes_without_overwriting() {
        let root = temp_storage_root("promote");
        let storage = LocalStorage::new(&root);

        let temporary_key = storage
            .put(&storage_key("_tmp/object.txt"), b"temporary")
            .await
            .expect("temporary put should succeed");
        let durable_key = storage
            .promote(&temporary_key, &storage_key("objects/object.txt"))
            .await
            .expect("promote should succeed");

        assert_eq!(durable_key.as_str(), "objects/object.txt");
        assert_eq!(
            storage
                .get(&storage_key("objects/object.txt"))
                .await
                .expect("durable object should exist"),
            b"temporary"
        );
        assert!(
            !storage
                .exists(&temporary_key)
                .await
                .expect("temporary existence check should succeed")
        );

        let second_temporary_key = storage
            .put(&storage_key("_tmp/object-2.txt"), b"second")
            .await
            .expect("second temporary put should succeed");
        let overwrite = storage
            .promote(&second_temporary_key, &storage_key("objects/object.txt"))
            .await;
        assert!(
            overwrite.is_err(),
            "promote must not overwrite an existing durable object"
        );
        assert_eq!(
            storage
                .get(&storage_key("objects/object.txt"))
                .await
                .expect("durable object should remain unchanged"),
            b"temporary"
        );
        assert!(
            storage
                .exists(&second_temporary_key)
                .await
                .expect("failed promotion should leave temporary object")
        );

        drop(std::fs::remove_dir_all(root));
    }

    /// Handles storage key for this module.
    fn storage_key(value: &str) -> StorageKey {
        StorageKey::try_new(value).expect("test storage key is valid")
    }

    /// Handles temp storage root for this module.
    fn temp_storage_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("agentics-storage-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("failed to create storage root");
        root
    }
}
