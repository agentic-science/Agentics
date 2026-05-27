use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use async_trait::async_trait;
use tokio::io::{AsyncWriteExt as _, copy};

use crate::fs_utils::{
    cleanup_temp_file_on_error, create_private_file, finalize_local_temp_without_overwrite,
    write_private_file,
};
use crate::{Result, Storage, StorageError, StorageKey, StorageWriteIntent};

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
            let copied = copy(&mut source, &mut file).await?;
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
        write_private_file(&temporary_full, content).await?;
        storage.reject_symlink_prefixes(key_path)?;
        storage.reject_symlink_object(full)?;
        finalize_local_temp_without_overwrite(&temporary_full, full, key.as_str()).await?;
        Ok::<(), StorageError>(())
    }
    .await;
    cleanup_temp_file_on_error(write_result, &temporary_full).await
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

fn invalid_storage_key() -> StorageError {
    StorageError::InvalidKey(
        "storage key must be a non-empty relative path with safe ASCII components and no `.` or `..` components".to_string(),
    )
}
