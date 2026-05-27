use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    LocalStorage, LocalStorageOptions, Result, S3Storage, S3StorageOptions, Storage, StorageError,
};

/// Backend-specific durable storage construction options.
#[derive(Debug, Clone)]
pub enum StorageFactoryOptions {
    Local(LocalStorageOptions),
    S3(S3StorageOptions),
}

/// Build the configured durable storage backend.
pub async fn build_storage(options: StorageFactoryOptions) -> anyhow::Result<Arc<dyn Storage>> {
    match options {
        StorageFactoryOptions::Local(options) => Ok(Arc::new(LocalStorage::from_options(options))),
        StorageFactoryOptions::S3(options) => Ok(Arc::new(S3Storage::from_options(options).await?)),
    }
}

/// Return the host-local work root for object storage staging and materialization.
pub fn storage_work_root(work_root: Option<&Path>) -> Result<PathBuf> {
    let root = work_root
        .filter(|value| !value.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("agentics-storage-work"));
    if !root.is_absolute() {
        return Err(StorageError::InvalidKey(
            "AGENTICS_STORAGE_WORK_ROOT must be an absolute path".to_string(),
        ));
    }
    Ok(root)
}
