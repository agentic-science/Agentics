use std::path::PathBuf;
use std::sync::Arc;

use agentics_config::{Config, StorageBackend};

use crate::{LocalStorage, Result, S3Storage, Storage, StorageError};

/// Build the configured durable storage backend.
pub async fn build_storage(config: &Config) -> anyhow::Result<Arc<dyn Storage>> {
    config.validate_object_storage_config()?;
    match config.storage.backend {
        StorageBackend::Local => Ok(Arc::new(LocalStorage::new(&config.storage.root))),
        StorageBackend::S3 => Ok(Arc::new(S3Storage::from_config(config).await?)),
    }
}

/// Return the host-local work root for object storage staging and materialization.
pub fn storage_work_root(config: &Config) -> Result<PathBuf> {
    let root = config
        .storage
        .work_root
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
