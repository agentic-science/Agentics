use std::path::Path;

use tokio::io::AsyncWriteExt as _;

use crate::{Result, StorageError};

pub async fn ensure_private_directory(path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || ensure_private_directory_sync(&path))
        .await
        .map_err(|e| StorageError::Internal(e.to_string()))?
}

#[cfg(unix)]
fn ensure_private_directory_sync(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(StorageError::InvalidKey(format!(
            "storage work root is not a directory: {}",
            path.display()
        )));
    }
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & !0o077))?;
    }
    let tightened_mode = std::fs::metadata(path)?.permissions().mode();
    if tightened_mode & 0o077 != 0 {
        return Err(StorageError::InvalidKey(format!(
            "storage work root must not be group/world accessible: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_directory_sync(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

pub(crate) async fn create_private_file(path: &Path) -> Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    Ok(options.open(path).await?)
}

pub(crate) async fn finalize_local_temp_without_overwrite(
    temporary: &Path,
    destination: &Path,
    conflict_label: &str,
) -> Result<()> {
    match tokio::fs::hard_link(temporary, destination).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(StorageError::ObjectConflict(conflict_label.to_string()));
        }
        Err(error) => return Err(error.into()),
    }
    if let Err(error) = tokio::fs::remove_file(temporary).await {
        let cleanup = tokio::fs::remove_file(destination).await;
        if let Err(cleanup_error) = cleanup
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(cleanup_error.into());
        }
        return Err(error.into());
    }
    Ok(())
}

pub(crate) async fn cleanup_temp_file_on_error(result: Result<()>, temporary: &Path) -> Result<()> {
    if let Err(error) = result {
        if let Err(cleanup_error) = tokio::fs::remove_file(temporary).await
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(cleanup_error.into());
        }
        return Err(error);
    }
    Ok(())
}

pub(crate) async fn write_private_file(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = create_private_file(path).await?;
    file.write_all(content).await?;
    file.flush().await?;
    Ok(())
}
