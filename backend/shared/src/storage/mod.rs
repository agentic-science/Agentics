//! Storage abstraction for uploaded solution_submissions and runner logs.

use std::path::Path;

use async_trait::async_trait;

use crate::error::AppError;

pub type Result<T> = std::result::Result<T, AppError>;

/// Minimal object-storage interface used by handlers and workers.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Store content at a storage-relative path and return the concrete path.
    async fn put(&self, path: &str, content: &[u8]) -> Result<String>;
    /// Read content from a storage-relative path.
    async fn get(&self, path: &str) -> Result<Vec<u8>>;
    /// Return whether a storage-relative path exists.
    async fn exists(&self, path: &str) -> Result<bool>;
    /// Delete a storage-relative path if it exists.
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

    fn resolve(&self, path: &str) -> std::path::PathBuf {
        self.root.join(path)
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn put(&self, path: &str, content: &[u8]) -> Result<String> {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full, content).await?;
        Ok(full.to_string_lossy().to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.resolve(path);
        Ok(tokio::fs::read(&full).await?)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full = self.resolve(path);
        Ok(tokio::fs::try_exists(full).await?)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full = self.resolve(path);
        match tokio::fs::remove_file(&full).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }
}
