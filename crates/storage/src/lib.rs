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

//! Storage abstraction for uploaded solution_submissions and runner logs.
//!
//! A storage key is an opaque object locator inside the configured storage
//! backend. It is intentionally not called a path or URI: local development may
//! map it onto a filesystem path, but callers must not rely on host filesystem
//! semantics, absolute paths, parent traversal, schemes, authorities, or URL
//! parsing. Storage backends own the mapping from `StorageKey` to physical
//! storage.

use std::path::{Component, Path, PathBuf};

use agentics_domain::error::ServiceError;
pub use agentics_domain::storage::{StorageKey, StorageKeyError};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

pub type Result<T> = std::result::Result<T, StorageError>;

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
            StorageError::ObjectConflict(_) => ServiceError::Conflict,
            StorageError::ObjectNotFound(_) => ServiceError::NotFound,
            StorageError::Internal(message) => ServiceError::Internal(message),
            StorageError::Io(error) => ServiceError::Io(error),
        }
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
                return Err(StorageError::SymlinkRejected(format!(
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
    /// Handles put for this module.
    async fn put(&self, key: &StorageKey, content: &[u8]) -> Result<StorageKey> {
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
        let write_result = async {
            let mut temporary = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_full)
                .await?;
            temporary.write_all(content).await?;
            temporary.flush().await?;
            drop(temporary);
            self.reject_symlink_prefixes(&key_path)?;
            self.reject_symlink_object(&full)?;
            if tokio::fs::try_exists(&full).await? {
                return Err(StorageError::ObjectConflict(key.to_string()));
            }
            tokio::fs::rename(&temporary_full, &full).await?;
            Ok::<(), StorageError>(())
        }
        .await;
        if let Err(error) = write_result {
            if let Err(cleanup_error) = tokio::fs::remove_file(&temporary_full).await
                && cleanup_error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(cleanup_error.into());
            }
            return Err(error);
        }
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

        if !tokio::fs::try_exists(&temporary_full).await? {
            return Err(StorageError::ObjectNotFound(temporary_key.to_string()));
        }
        if tokio::fs::try_exists(&durable_full).await? {
            return Err(StorageError::ObjectConflict(durable_key.to_string()));
        }
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
        match tokio::fs::read(&full).await {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::ObjectNotFound(key.to_string()))
            }
            Err(error) => Err(error.into()),
        }
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

/// Handles invalid storage key for this module.
fn invalid_storage_key() -> StorageError {
    StorageError::InvalidKey(
        "storage key must be a non-empty relative path with safe ASCII components and no `.` or `..` components".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{LocalStorage, Storage, StorageError, StorageKey, StorageKeyError};

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
            assert!(matches!(result, Err(StorageKeyError::InvalidKey)));
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

        assert!(matches!(result, Err(StorageError::SymlinkRejected(_))));
        assert!(!outside.join("escape.txt").exists());
        drop(std::fs::remove_dir_all(root));
        drop(std::fs::remove_dir_all(outside));
    }

    #[cfg(unix)]
    /// Verifies that local storage rejects symlink objects.
    #[tokio::test]
    async fn local_storage_rejects_symlink_objects() {
        let root = temp_storage_root("symlink-object-root");
        let outside = temp_storage_root("symlink-object-outside");
        std::fs::create_dir_all(root.join("objects")).expect("objects dir should be created");
        std::os::unix::fs::symlink(outside.join("escape.txt"), root.join("objects/object.txt"))
            .expect("failed to create symlink");
        let storage = LocalStorage::new(&root);

        let result = storage
            .put(&storage_key("objects/object.txt"), b"bad")
            .await;

        assert!(matches!(result, Err(StorageError::SymlinkRejected(_))));
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
