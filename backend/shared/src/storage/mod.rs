//! Storage abstraction for uploaded solution_submissions and runner logs.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;

use crate::error::AppError;

pub type Result<T> = std::result::Result<T, AppError>;

/// Minimal object-storage interface used by handlers and workers.
#[async_trait]
pub trait Storage: std::fmt::Debug + Send + Sync {
    /// Store content at a storage-relative key and return that opaque key.
    async fn put(&self, path: &str, content: &[u8]) -> Result<String>;
    /// Read content from a storage-relative key.
    async fn get(&self, path: &str) -> Result<Vec<u8>>;
    /// Return whether a storage-relative key exists.
    async fn exists(&self, path: &str) -> Result<bool>;
    /// Delete a storage-relative key if it exists.
    async fn delete(&self, path: &str) -> Result<()>;
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

    fn resolve(&self, path: &str) -> Result<(PathBuf, PathBuf)> {
        let key = validate_storage_key(path)?;
        Ok((self.root.join(&key), key))
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
    async fn put(&self, path: &str, content: &[u8]) -> Result<String> {
        let (full, key) = self.resolve(path)?;
        self.reject_symlink_prefixes(&key)?;
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        self.reject_symlink_prefixes(&key)?;
        self.reject_symlink_object(&full)?;
        tokio::fs::write(&full, content).await?;
        Ok(key.to_string_lossy().to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let (full, key) = self.resolve(path)?;
        self.reject_symlink_prefixes(&key)?;
        self.reject_symlink_object(&full)?;
        Ok(tokio::fs::read(&full).await?)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let (full, key) = self.resolve(path)?;
        self.reject_symlink_prefixes(&key)?;
        if let Ok(metadata) = tokio::fs::symlink_metadata(&full).await {
            return Ok(!metadata.file_type().is_symlink());
        }
        Ok(false)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let (full, key) = self.resolve(path)?;
        self.reject_symlink_prefixes(&key)?;
        self.reject_symlink_object(&full)?;
        match tokio::fs::remove_file(&full).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }
}

fn validate_storage_key(path: &str) -> Result<PathBuf> {
    if path.trim().is_empty() {
        return Err(invalid_storage_key());
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(invalid_storage_key());
    }

    let mut key = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => key.push(part),
            _ => return Err(invalid_storage_key()),
        }
    }
    if key.as_os_str().is_empty() {
        return Err(invalid_storage_key());
    }
    Ok(key)
}

fn invalid_storage_key() -> AppError {
    AppError::BadRequest(
        "storage key must be a non-empty relative path without `.` or `..` components".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{LocalStorage, Storage};
    use crate::error::AppError;

    #[tokio::test]
    async fn local_storage_returns_relative_keys() {
        let root = temp_storage_root("relative-keys");
        let storage = LocalStorage::new(&root);

        let key = storage
            .put("objects/a.txt", b"hello")
            .await
            .expect("put should succeed");

        assert_eq!(key, "objects/a.txt");
        assert_eq!(
            storage.get(&key).await.expect("get should succeed"),
            b"hello"
        );
        assert!(root.join("objects/a.txt").is_file());
        drop(std::fs::remove_dir_all(root));
    }

    #[tokio::test]
    async fn local_storage_rejects_absolute_and_parent_keys() {
        let root = temp_storage_root("bad-keys");
        let storage = LocalStorage::new(&root);

        for key in ["/tmp/escape.txt", "../escape.txt", "a/../escape.txt", "."] {
            let result = storage.put(key, b"bad").await;
            assert!(matches!(result, Err(AppError::BadRequest(_))));
        }
        drop(std::fs::remove_dir_all(root));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_storage_rejects_symlink_prefixes() {
        let root = temp_storage_root("symlink-root");
        let outside = temp_storage_root("symlink-outside");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("failed to create symlink");
        let storage = LocalStorage::new(&root);

        let result = storage.put("link/escape.txt", b"bad").await;

        assert!(matches!(result, Err(AppError::BadRequest(_))));
        assert!(!outside.join("escape.txt").exists());
        drop(std::fs::remove_dir_all(root));
        drop(std::fs::remove_dir_all(outside));
    }

    fn temp_storage_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("agentics-storage-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("failed to create storage root");
        root
    }
}
