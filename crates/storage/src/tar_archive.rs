use std::fs;
use std::io::{Read, Write};
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
pub async fn unpack_tar_to_directory(
    archive_path: &Path,
    destination_dir: &Path,
    intent: StorageWriteIntent,
) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let destination_dir = destination_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        unpack_tar_to_directory_blocking(
            &archive_path,
            &destination_dir,
            TarUnpackLimits::bundle(intent),
        )
    })
    .await
    .map_err(|e| StorageError::Internal(e.to_string()))?
}

#[cfg(test)]
pub(crate) async fn unpack_tar_to_directory_with_limits(
    archive_path: &Path,
    destination_dir: &Path,
    intent: StorageWriteIntent,
    max_entries: u64,
    max_depth: usize,
) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let destination_dir = destination_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        unpack_tar_to_directory_blocking(
            &archive_path,
            &destination_dir,
            TarUnpackLimits {
                intent,
                max_entries,
                max_depth,
            },
        )
    })
    .await
    .map_err(|e| StorageError::Internal(e.to_string()))?
}

#[derive(Debug, Clone, Copy)]
struct TarUnpackLimits {
    intent: StorageWriteIntent,
    max_entries: u64,
    max_depth: usize,
}

impl TarUnpackLimits {
    const fn bundle(intent: StorageWriteIntent) -> Self {
        Self {
            intent,
            max_entries: MAX_BUNDLE_TAR_ENTRIES,
            max_depth: MAX_BUNDLE_TAR_DEPTH,
        }
    }
}

fn pack_directory_to_tar_blocking(
    source_dir: &Path,
    archive_path: &Path,
    intent: StorageWriteIntent,
) -> Result<()> {
    validate_tar_source_limits(source_dir, intent)?;
    if let Some(parent) = archive_path.parent() {
        create_private_dir_all(parent)?;
    }
    let temporary = archive_path.with_extension(format!("agentics-tar-{}", uuid::Uuid::new_v4()));
    let file = create_private_file_sync(&temporary)?;
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

fn unpack_tar_to_directory_blocking(
    archive_path: &Path,
    destination_dir: &Path,
    limits: TarUnpackLimits,
) -> Result<()> {
    create_private_dir_all(destination_dir)?;
    let file = fs::File::open(archive_path)?;
    let mut archive = tar::Archive::new(file);
    let mut entry_count = 0u64;
    let mut copied_bytes = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let relative = entry.path()?.into_owned();
        validate_tar_path(&relative, limits.max_depth)?;
        entry_count = entry_count.checked_add(1).ok_or_else(|| {
            StorageError::Internal("bundle archive entry count overflow".to_string())
        })?;
        if entry_count > limits.max_entries {
            return Err(StorageError::InvalidKey(format!(
                "{} contains too many tar entries: {entry_count} > {}",
                limits.intent.label(),
                limits.max_entries
            )));
        }
        let target = destination_dir.join(&relative);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            create_private_dir_all(&target)?;
        } else if entry_type == tar::EntryType::Regular {
            if let Some(parent) = target.parent() {
                create_private_dir_all(parent)?;
            }
            let mut file = create_private_file_sync(&target)?;
            copy_tar_file_with_limit(&mut entry, &mut file, limits.intent, &mut copied_bytes)?;
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

#[cfg(unix)]
fn create_private_dir_all(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(StorageError::InvalidKey(format!(
            "bundle archive path is not a directory: {}",
            path.display()
        )));
    }
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & !0o077))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_private_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

#[cfg(unix)]
fn create_private_file_sync(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    Ok(fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}

#[cfg(not(unix))]
fn create_private_file_sync(path: &Path) -> Result<fs::File> {
    Ok(fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?)
}

fn copy_tar_file_with_limit<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    intent: StorageWriteIntent,
    copied_bytes: &mut u64,
) -> Result<()> {
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        let read_u64 = u64::try_from(read)
            .map_err(|_| StorageError::Internal("tar entry read size overflow".to_string()))?;
        let new_total = copied_bytes
            .checked_add(read_u64)
            .ok_or(StorageError::ObjectTooLarge {
                label: intent.label(),
                actual: u64::MAX,
                limit: intent.max_bytes(),
            })?;
        intent.ensure_len(new_total)?;
        let chunk = buffer
            .get(..read)
            .ok_or_else(|| StorageError::Internal("tar entry read exceeded buffer".to_string()))?;
        writer.write_all(chunk)?;
        *copied_bytes = new_total;
    }
}

fn validate_tar_path(path: &Path, max_depth: usize) -> Result<()> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(StorageError::InvalidKey(
            "bundle archive contains unsafe path".to_string(),
        ));
    }
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => {
                depth = depth.checked_add(1).ok_or_else(|| {
                    StorageError::Internal("bundle archive path depth overflow".to_string())
                })?;
            }
            _ => {
                return Err(StorageError::InvalidKey(
                    "bundle archive contains unsafe path".to_string(),
                ));
            }
        }
    }
    if depth > max_depth {
        return Err(StorageError::InvalidKey(format!(
            "bundle archive contains a path deeper than {max_depth} components: {}",
            path.display()
        )));
    }
    Ok(())
}
