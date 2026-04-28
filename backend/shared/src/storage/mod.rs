use std::path::Path;

use async_trait::async_trait;

use crate::error::AppError;

pub type Result<T> = std::result::Result<T, AppError>;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn put(&self, path: &str, content: &[u8]) -> Result<String>;
    async fn get(&self, path: &str) -> Result<Vec<u8>>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn delete(&self, path: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct LocalStorage {
    root: std::path::PathBuf,
}

impl LocalStorage {
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
        Ok(full.exists())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full = self.resolve(path);
        if full.exists() {
            tokio::fs::remove_file(&full).await?;
        }
        Ok(())
    }
}
