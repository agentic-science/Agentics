//! Filesystem hashing and copy helpers for challenge bundles.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{Result, ServiceError};
use crate::models::hashes::Sha256Digest;

/// Return a deterministic SHA-256 digest of all files in a bundle tree.
pub async fn challenge_bundle_tree_sha256(bundle_root: &Path) -> Result<Sha256Digest> {
    let bundle_root = bundle_root.to_path_buf();
    tokio::task::spawn_blocking(move || challenge_bundle_tree_sha256_blocking(&bundle_root))
        .await
        .map_err(|e| ServiceError::Internal(format!("bundle digest task failed: {e}")))?
}

/// Copy a challenge bundle directory while rejecting symlinks.
pub async fn copy_challenge_bundle_dir(
    source: &Path,
    target: &Path,
    replace_existing: bool,
) -> Result<()> {
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    tokio::task::spawn_blocking(move || {
        copy_challenge_bundle_dir_blocking(&source, &target, replace_existing)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("bundle copy task failed: {e}")))?
}

/// Copy a challenge bundle directory while excluding one bundle-relative tree.
pub async fn copy_challenge_bundle_dir_excluding(
    source: &Path,
    target: &Path,
    excluded_relative_tree: &Path,
    replace_existing: bool,
) -> Result<()> {
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    let excluded_relative_tree = excluded_relative_tree.to_path_buf();
    tokio::task::spawn_blocking(move || {
        copy_challenge_bundle_dir_excluding_blocking(
            &source,
            &target,
            &excluded_relative_tree,
            replace_existing,
        )
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("public bundle copy task failed: {e}")))?
}

/// Compute a bundle-tree digest synchronously for execution on a blocking thread.
fn challenge_bundle_tree_sha256_blocking(bundle_root: &Path) -> Result<Sha256Digest> {
    let mut hasher = Sha256::new();
    hash_bundle_tree(&mut hasher, bundle_root)?;
    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}

/// Walk a bundle tree in deterministic order and feed path and file bytes into the digest.
fn hash_bundle_tree(hasher: &mut Sha256, bundle_root: &Path) -> Result<()> {
    let mut stack = vec![bundle_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = std::fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let relative_path = path.strip_prefix(bundle_root).map_err(|e| {
                ServiceError::Internal(format!("failed to build bundle digest: {e}"))
            })?;
            let relative_path = relative_path.to_str().ok_or_else(|| {
                ServiceError::Validation(format!(
                    "bundle path must be UTF-8 for digesting: {}",
                    path.display()
                ))
            })?;

            if metadata.file_type().is_symlink() {
                return Err(ServiceError::Validation(format!(
                    "challenge bundle must not contain symlinks: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                hash_field(hasher, "dir", relative_path.as_bytes());
                stack.push(path);
            } else if metadata.is_file() {
                hash_field(hasher, "file", relative_path.as_bytes());
                hash_file(hasher, &path)?;
            }
        }
    }

    Ok(())
}

/// Feed one file's length and bytes into the bundle digest.
fn hash_file(hasher: &mut Sha256, path: &Path) -> Result<()> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    hash_field(hasher, "file_size", &size.to_be_bytes());

    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        let chunk = buffer.get(..bytes_read).ok_or_else(|| {
            ServiceError::Internal("file read exceeded digest buffer bounds".to_string())
        })?;
        hasher.update(chunk);
    }

    Ok(())
}

/// Add one length-delimited digest field to avoid ambiguous concatenation.
fn hash_field(hasher: &mut Sha256, label: &str, bytes: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// Copy a challenge bundle synchronously for execution on a blocking thread.
fn copy_challenge_bundle_dir_blocking(
    source: &Path,
    target: &Path,
    replace_existing: bool,
) -> Result<()> {
    if target.exists() {
        if !replace_existing {
            if target.is_dir() {
                return Ok(());
            }
            return Err(ServiceError::Validation(format!(
                "managed bundle target exists and is not a directory: {}",
                target.display()
            )));
        }
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;

    let mut stack = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((current_source, current_target)) = stack.pop() {
        for entry in std::fs::read_dir(&current_source)? {
            let entry = entry?;
            let source_path = entry.path();
            let target_path = current_target.join(entry.file_name());
            let meta = std::fs::symlink_metadata(&source_path)?;
            if meta.file_type().is_symlink() {
                return Err(ServiceError::Validation(format!(
                    "challenge bundle must not contain symlinks: {}",
                    source_path.display()
                )));
            }
            if meta.is_dir() {
                std::fs::create_dir_all(&target_path)?;
                stack.push((source_path, target_path));
            } else if meta.is_file() {
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&source_path, &target_path)?;
            }
        }
    }

    Ok(())
}

/// Copy a challenge bundle while excluding one relative subtree.
fn copy_challenge_bundle_dir_excluding_blocking(
    source: &Path,
    target: &Path,
    excluded_relative_tree: &Path,
    replace_existing: bool,
) -> Result<()> {
    if excluded_relative_tree.as_os_str().is_empty() || excluded_relative_tree.is_absolute() {
        return Err(ServiceError::Validation(
            "excluded challenge bundle path must be relative and non-empty".to_string(),
        ));
    }

    if target.exists() {
        if !replace_existing {
            if target.is_dir() {
                return Ok(());
            }
            return Err(ServiceError::Validation(format!(
                "managed public bundle target exists and is not a directory: {}",
                target.display()
            )));
        }
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;

    let mut stack = vec![source.to_path_buf()];
    while let Some(current_source) = stack.pop() {
        let relative_dir = current_source
            .strip_prefix(source)
            .map_err(|e| ServiceError::Internal(format!("failed to copy public bundle: {e}")))?;
        if is_path_within_tree(relative_dir, excluded_relative_tree) {
            continue;
        }
        let current_target = target.join(relative_dir);
        std::fs::create_dir_all(&current_target)?;

        for entry in std::fs::read_dir(&current_source)? {
            let entry = entry?;
            let source_path = entry.path();
            let relative_path = source_path.strip_prefix(source).map_err(|e| {
                ServiceError::Internal(format!("failed to copy public bundle: {e}"))
            })?;
            if is_path_within_tree(relative_path, excluded_relative_tree) {
                continue;
            }

            let meta = std::fs::symlink_metadata(&source_path)?;
            if meta.file_type().is_symlink() {
                return Err(ServiceError::Validation(format!(
                    "challenge bundle must not contain symlinks: {}",
                    source_path.display()
                )));
            }
            if meta.is_dir() {
                stack.push(source_path);
            } else if meta.is_file() {
                let target_path = target.join(relative_path);
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&source_path, &target_path)?;
            }
        }
    }

    Ok(())
}

/// Return whether a relative path is the excluded tree itself or a child of it.
fn is_path_within_tree(path: &Path, tree: &Path) -> bool {
    path == tree || path.starts_with(tree)
}
