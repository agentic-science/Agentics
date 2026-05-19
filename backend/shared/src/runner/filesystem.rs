use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::validation::archive::extract_zip_file_to_dir;
use crate::zip_project::{
    ZipProjectPhaseFailureReason, ZipProjectPhaseName, zip_project_archive_policy,
};

use super::errors::phase_error;

/// Handles extract zip safe for this module.
pub(super) async fn extract_zip_safe(artifact_path: &Path, target_dir: &Path) -> Result<()> {
    let artifact_path = artifact_path.to_path_buf();
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        extract_zip_file_to_dir(&artifact_path, &target_dir, &zip_project_archive_policy())
    })
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

    use crate::zip_project::MAX_ZIP_PROJECT_FILE_COUNT;

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

    /// Writes a minimal stored ZIP while preserving raw names and Unix mode bits.
    fn write_raw_stored_zip(path: &Path, entries: Vec<(&str, &[u8], u32)>) {
        let mut bytes = Vec::new();
        let mut central_directory = Vec::new();
        let entry_count = u16::try_from(entries.len()).expect("test ZIP entries fit u16");

        for (name, content, unix_mode) in entries {
            let local_header_offset =
                u32::try_from(bytes.len()).expect("test ZIP should fit u32 offsets");
            let name_bytes = name.as_bytes();
            let name_len = u16::try_from(name_bytes.len()).expect("test ZIP names are short");
            let content_len = u32::try_from(content.len()).expect("test ZIP content is small");
            let crc = crc32(content);

            push_u32(&mut bytes, 0x0403_4b50);
            push_u16(&mut bytes, 20);
            push_u16(&mut bytes, 0);
            push_u16(&mut bytes, 0);
            push_u16(&mut bytes, 0);
            push_u16(&mut bytes, 0);
            push_u32(&mut bytes, crc);
            push_u32(&mut bytes, content_len);
            push_u32(&mut bytes, content_len);
            push_u16(&mut bytes, name_len);
            push_u16(&mut bytes, 0);
            bytes.extend_from_slice(name_bytes);
            bytes.extend_from_slice(content);

            push_u32(&mut central_directory, 0x0201_4b50);
            push_u16(&mut central_directory, 0x0314);
            push_u16(&mut central_directory, 20);
            push_u16(&mut central_directory, 0);
            push_u16(&mut central_directory, 0);
            push_u16(&mut central_directory, 0);
            push_u16(&mut central_directory, 0);
            push_u32(&mut central_directory, crc);
            push_u32(&mut central_directory, content_len);
            push_u32(&mut central_directory, content_len);
            push_u16(&mut central_directory, name_len);
            push_u16(&mut central_directory, 0);
            push_u16(&mut central_directory, 0);
            push_u16(&mut central_directory, 0);
            push_u16(&mut central_directory, 0);
            push_u32(&mut central_directory, unix_mode << 16);
            push_u32(&mut central_directory, local_header_offset);
            central_directory.extend_from_slice(name_bytes);
        }

        let central_directory_offset =
            u32::try_from(bytes.len()).expect("test ZIP should fit u32 offsets");
        let central_directory_size =
            u32::try_from(central_directory.len()).expect("test ZIP should fit u32 sizes");
        bytes.extend_from_slice(&central_directory);
        push_u32(&mut bytes, 0x0605_4b50);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, entry_count);
        push_u16(&mut bytes, entry_count);
        push_u32(&mut bytes, central_directory_size);
        push_u32(&mut bytes, central_directory_offset);
        push_u16(&mut bytes, 0);

        std::fs::write(path, bytes).expect("failed to write raw test ZIP");
    }

    /// Append a little-endian u16 to a test ZIP buffer.
    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Append a little-endian u32 to a test ZIP buffer.
    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Compute CRC-32 for tiny stored ZIP test entries.
    fn crc32(content: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for byte in content {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    /// Verifies that extract zip safe rejects unsafe entry names.
    #[tokio::test]
    async fn extract_zip_safe_rejects_unsafe_entry_names() {
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

        let error = extract_zip_safe(&zip_path, &target_dir)
            .await
            .expect_err("unsafe entry should fail extraction");
        assert!(error.to_string().contains("unsafe path"));
        assert!(!target_dir.join("main.py").exists());
        assert!(!target_dir.join("scripts/setup.sh").exists());

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

    /// Verifies that duplicate entries cannot overwrite earlier extracted files.
    #[tokio::test]
    async fn extract_zip_safe_rejects_duplicate_entries() {
        let zip_path = temp_path("duplicate-entry.zip");
        let target_dir = temp_path("duplicate-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        write_raw_stored_zip(
            &zip_path,
            vec![
                ("scripts/main.py", b"print('first')\n", 0o100644),
                ("scripts\\main.py", b"print('second')\n", 0o100644),
            ],
        );

        let error = extract_zip_safe(&zip_path, &target_dir)
            .await
            .expect_err("duplicate entry should fail extraction");
        assert!(error.to_string().contains("duplicate path"));
        assert!(!target_dir.join("scripts/main.py").exists());

        drop(std::fs::remove_file(zip_path));
        drop(std::fs::remove_dir_all(target_dir));
    }

    /// Verifies that Unix-mode symlink entries are rejected before extraction.
    #[tokio::test]
    async fn extract_zip_safe_rejects_symlink_entries() {
        let zip_path = temp_path("symlink-entry.zip");
        let target_dir = temp_path("symlink-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        write_raw_stored_zip(&zip_path, vec![("link.py", b"main.py", 0o120777)]);

        let error = extract_zip_safe(&zip_path, &target_dir)
            .await
            .expect_err("symlink entry should fail extraction");
        assert!(error.to_string().contains("must not contain symlinks"));
        assert!(!target_dir.join("link.py").exists());

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
