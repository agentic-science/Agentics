use std::fs;
use std::io::Write;
use std::path::{Component, Path};

use super::{Result, StorageError, StorageWriteIntent};

const MAX_BUNDLE_TAR_ENTRIES: u64 = 100_000;
const MAX_BUNDLE_TAR_DEPTH: usize = 128;

/// Create an immutable tar archive from a validated bundle directory.
pub async fn pack_directory_to_tar(
    source_dir: &Path,
    archive_path: &Path,
    intent: StorageWriteIntent,
) -> Result<()> {
    let source_dir = source_dir.to_path_buf();
    let archive_path = archive_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        pack_directory_to_tar_blocking(&source_dir, &archive_path, intent)
    })
    .await
    .map_err(|e| StorageError::Internal(e.to_string()))?
}

/// Extract an Agentics-managed bundle tar archive into a destination directory.
pub async fn unpack_tar_to_directory(archive_path: &Path, destination_dir: &Path) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let destination_dir = destination_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        unpack_tar_to_directory_blocking(&archive_path, &destination_dir)
    })
    .await
    .map_err(|e| StorageError::Internal(e.to_string()))?
}

fn pack_directory_to_tar_blocking(
    source_dir: &Path,
    archive_path: &Path,
    intent: StorageWriteIntent,
) -> Result<()> {
    validate_tar_source_limits(source_dir, intent)?;
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = archive_path.with_extension(format!("agentics-tar-{}", uuid::Uuid::new_v4()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        let mut builder = tar::Builder::new(file);
        append_bundle_dir(&mut builder, source_dir, source_dir)?;
        builder.finish()?;
        intent.ensure_len(fs::metadata(&temporary)?.len())?;
        finalize_temp_without_overwrite_blocking(
            &temporary,
            archive_path,
            &archive_path.display().to_string(),
        )?;
        Ok::<(), StorageError>(())
    })();
    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_file(&temporary)
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(cleanup_error.into());
        }
        return Err(error);
    }
    Ok(())
}

fn validate_tar_source_limits(source_dir: &Path, intent: StorageWriteIntent) -> Result<()> {
    let mut stack = vec![source_dir.to_path_buf()];
    let mut entry_count = 0u64;
    let mut projected_bytes = 1024u64;
    while let Some(dir) = stack.pop() {
        let mut entries = fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(StorageError::InvalidKey(format!(
                    "bundle archive contains unsupported filesystem entry: {}",
                    path.display()
                )));
            }
            entry_count = entry_count.checked_add(1).ok_or_else(|| {
                StorageError::Internal("bundle archive entry count overflow".to_string())
            })?;
            if entry_count > MAX_BUNDLE_TAR_ENTRIES {
                return Err(StorageError::InvalidKey(format!(
                    "{} contains too many filesystem entries: {entry_count} > {MAX_BUNDLE_TAR_ENTRIES}",
                    intent.label()
                )));
            }
            let relative = path
                .strip_prefix(source_dir)
                .map_err(|e| StorageError::Internal(e.to_string()))?;
            if relative.components().count() > MAX_BUNDLE_TAR_DEPTH {
                return Err(StorageError::InvalidKey(format!(
                    "{} contains a path deeper than {MAX_BUNDLE_TAR_DEPTH} components: {}",
                    intent.label(),
                    relative.display()
                )));
            }
            projected_bytes =
                projected_bytes
                    .checked_add(4096)
                    .ok_or_else(|| StorageError::ObjectTooLarge {
                        label: intent.label(),
                        actual: u64::MAX,
                        limit: intent.max_bytes(),
                    })?;
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                let file_bytes = padded_tar_file_size(metadata.len())?;
                projected_bytes = projected_bytes.checked_add(file_bytes).ok_or_else(|| {
                    StorageError::ObjectTooLarge {
                        label: intent.label(),
                        actual: u64::MAX,
                        limit: intent.max_bytes(),
                    }
                })?;
            } else {
                return Err(StorageError::InvalidKey(format!(
                    "bundle archive contains unsupported filesystem entry: {}",
                    path.display()
                )));
            }
            intent.ensure_len(projected_bytes)?;
        }
    }
    Ok(())
}

fn padded_tar_file_size(size: u64) -> Result<u64> {
    let remainder = size % 512;
    if remainder == 0 {
        return Ok(size);
    }
    let padding = 512u64
        .checked_sub(remainder)
        .ok_or_else(|| StorageError::Internal("bundle archive file size overflow".to_string()))?;
    size.checked_add(padding)
        .ok_or_else(|| StorageError::Internal("bundle archive file size overflow".to_string()))
}

fn append_bundle_dir(builder: &mut tar::Builder<fs::File>, root: &Path, dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|e| StorageError::Internal(e.to_string()))?;
        if metadata.is_dir() {
            builder.append_dir(relative, &path)?;
            append_bundle_dir(builder, root, &path)?;
        } else if metadata.is_file() {
            builder.append_path_with_name(&path, relative)?;
        } else {
            return Err(StorageError::InvalidKey(format!(
                "bundle archive contains unsupported filesystem entry: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn finalize_temp_without_overwrite_blocking(
    temporary: &Path,
    destination: &Path,
    conflict_label: &str,
) -> Result<()> {
    match fs::hard_link(temporary, destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(StorageError::ObjectConflict(conflict_label.to_string()));
        }
        Err(error) => return Err(error.into()),
    }
    if let Err(error) = fs::remove_file(temporary) {
        let cleanup = fs::remove_file(destination);
        if let Err(cleanup_error) = cleanup
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(cleanup_error.into());
        }
        return Err(error.into());
    }
    Ok(())
}

fn unpack_tar_to_directory_blocking(archive_path: &Path, destination_dir: &Path) -> Result<()> {
    fs::create_dir_all(destination_dir)?;
    let file = fs::File::open(archive_path)?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let relative = entry.path()?.into_owned();
        validate_tar_path(&relative)?;
        let target = destination_dir.join(&relative);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            std::io::copy(&mut entry, &mut file)?;
            file.flush()?;
        } else {
            return Err(StorageError::InvalidKey(format!(
                "bundle archive contains unsupported tar entry: {}",
                relative.display()
            )));
        }
    }
    Ok(())
}

fn validate_tar_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(StorageError::InvalidKey(
            "bundle archive contains unsafe path".to_string(),
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(StorageError::InvalidKey(
                    "bundle archive contains unsafe path".to_string(),
                ));
            }
        }
    }
    Ok(())
}
