use std::path::Path;
use std::time::SystemTime;

use async_trait::async_trait;

use crate::{Result, StorageError, StorageKey, StorageWriteIntent};

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
