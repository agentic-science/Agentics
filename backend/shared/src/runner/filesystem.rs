use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::zip_project::{
    MAX_ZIP_PROJECT_ARTIFACT_BYTES, MAX_ZIP_PROJECT_FILE_COUNT, MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    ZipProjectPhaseFailureReason, ZipProjectPhaseName,
};

use super::errors::phase_error;

/// Handles extract zip safe for this module.
pub(super) async fn extract_zip_safe(artifact_path: &Path, target_dir: &Path) -> Result<()> {
    let artifact_size = tokio::fs::metadata(artifact_path).await?.len();
    if artifact_size > MAX_ZIP_PROJECT_ARTIFACT_BYTES {
        return Err(AppError::Validation(format!(
            "solution archive must be at most {} bytes",
            MAX_ZIP_PROJECT_ARTIFACT_BYTES
        )));
    }

    let artifact_path = artifact_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_zip_safe_blocking(&artifact_path, &target_dir))
        .await
        .map_err(|e| AppError::Internal(format!("zip extraction task failed: {e}")))?
}

/// Ensures disk limit before continuing.
pub(super) async fn ensure_disk_limit(
    path: &Path,
    disk_limit_mb: u64,
    phase: ZipProjectPhaseName,
) -> Result<()> {
    let path = path.to_path_buf();
    let bytes = tokio::task::spawn_blocking(move || directory_size(&path))
        .await
        .map_err(|e| AppError::Internal(format!("disk usage task failed: {e}")))??;
    let limit_bytes = disk_limit_mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| AppError::Runner("disk limit overflow".to_string()))?;
    if bytes > limit_bytes {
        return Err(phase_error(
            phase,
            ZipProjectPhaseFailureReason::ResourceLimit,
            format!("phase exceeded disk limit: {bytes} > {limit_bytes} bytes"),
            None,
        ));
    }
    Ok(())
}

/// Ensures prepare disk limit before continuing.
pub(super) async fn ensure_prepare_disk_limit(path: &Path, disk_limit_mb: u64) -> Result<()> {
    let path = path.to_path_buf();
    let bytes = tokio::task::spawn_blocking(move || directory_size(&path))
        .await
        .map_err(|e| AppError::Internal(format!("prepare disk usage task failed: {e}")))??;
    let limit_bytes = disk_limit_mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| AppError::Runner("prepare disk limit overflow".to_string()))?;
    if bytes > limit_bytes {
        return Err(AppError::Runner(format!(
            "prepare phase exceeded disk limit: {bytes} > {limit_bytes} bytes"
        )));
    }
    Ok(())
}

/// Copies dir all while preserving the module invariants.
pub(super) async fn copy_dir_all(source: &Path, destination: &Path) -> Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || copy_dir_all_blocking(&source, &destination))
        .await
        .map_err(|e| AppError::Internal(format!("copy task failed: {e}")))?
}

/// Handles cleanup paths for this module.
pub(super) async fn cleanup_paths<const N: usize>(paths: [PathBuf; N]) -> Result<()> {
    for path in paths {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AppError::Io(e)),
        }
    }
    Ok(())
}

/// Handles extract zip safe blocking for this module.
fn extract_zip_safe_blocking(artifact_path: &Path, target_dir: &Path) -> Result<()> {
    let reader = std::fs::File::open(artifact_path)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_ZIP_PROJECT_FILE_COUNT {
        return Err(AppError::Validation(format!(
            "solution archive must contain at most {} entries",
            MAX_ZIP_PROJECT_FILE_COUNT
        )));
    }

    let mut total_uncompressed_size = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        total_uncompressed_size = total_uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| AppError::Validation("solution archive is too large".to_string()))?;
        if total_uncompressed_size > MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES {
            return Err(AppError::Validation(format!(
                "solution archive must expand to at most {} bytes",
                MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES
            )));
        }

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

/// Handles directory size for this module.
fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            total = total
                .checked_add(directory_size(&entry.path())?)
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
        } else {
            // Count symlink directory entries as links, never as their host targets.
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
        }
    }
    Ok(total)
}

/// Copies dir all blocking while preserving the module invariants.
fn copy_dir_all_blocking(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all_blocking(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// Handles temp path for this module.
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-runner-{name}-{}", uuid::Uuid::new_v4()))
    }

    /// Writes zip to the target path.
    fn write_zip(path: &Path, entries: Vec<(String, Vec<u8>)>) {
        let file = std::fs::File::create(path).expect("failed to create test zip");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            archive
                .start_file(name, options)
                .expect("failed to start zip entry");
            archive
                .write_all(&bytes)
                .expect("failed to write zip entry");
        }

        archive.finish().expect("failed to finish test zip");
    }

    /// Verifies that extract zip safe skips unsafe entry names.
    #[tokio::test]
    async fn extract_zip_safe_skips_unsafe_entry_names() {
        let zip_path = temp_path("unsafe-entry.zip");
        let target_dir = temp_path("unsafe-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        write_zip(
            &zip_path,
            vec![
                ("../escape.py".to_string(), b"print('bad')\n".to_vec()),
                ("main.py".to_string(), b"print('ok')\n".to_vec()),
                ("scripts/setup.sh".to_string(), b"true\n".to_vec()),
            ],
        );

        extract_zip_safe(&zip_path, &target_dir)
            .await
            .expect("extraction should succeed");

        let extracted_files = std::fs::read_dir(&target_dir)
            .expect("failed to read target dir")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("failed to collect target dir entries");
        assert_eq!(extracted_files.len(), 2);
        assert!(target_dir.join("main.py").is_file());
        assert!(target_dir.join("scripts/setup.sh").is_file());

        drop(std::fs::remove_file(zip_path));
        drop(std::fs::remove_dir_all(target_dir));
    }

    /// Verifies that extract zip safe rejects too many entries.
    #[tokio::test]
    async fn extract_zip_safe_rejects_too_many_entries() {
        let zip_path = temp_path("too-many.zip");
        let target_dir = temp_path("too-many-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        let entries = (0..=MAX_ZIP_PROJECT_FILE_COUNT)
            .map(|i| (format!("file-{i}.txt"), Vec::new()))
            .collect();
        write_zip(&zip_path, entries);

        let result = extract_zip_safe(&zip_path, &target_dir).await;

        assert!(
            matches!(result, Err(AppError::Validation(message)) if message.contains("at most"))
        );

        drop(std::fs::remove_file(zip_path));
        drop(std::fs::remove_dir_all(target_dir));
    }

    #[cfg(unix)]
    /// Verifies that directory size does not follow symlinks.
    #[test]
    fn directory_size_does_not_follow_symlinks() {
        let root = temp_path("symlink-size-root");
        let outside = temp_path("symlink-size-outside.txt");
        std::fs::create_dir_all(&root).expect("failed to create root");
        std::fs::write(&outside, vec![b'x'; 1024 * 1024]).expect("failed to write outside file");
        std::os::unix::fs::symlink(&outside, root.join("outside-link"))
            .expect("failed to create symlink");

        let bytes = directory_size(&root).expect("directory size should succeed");

        assert!(
            bytes < 1024 * 1024,
            "symlink target should not be counted: {bytes}"
        );

        drop(std::fs::remove_file(outside));
        drop(std::fs::remove_dir_all(root));
    }
}
