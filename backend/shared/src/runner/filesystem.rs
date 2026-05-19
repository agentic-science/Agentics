use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::validation::archive::extract_zip_file_to_dir;
use crate::zip_project::{
    ZipProjectPhaseFailureReason, ZipProjectPhaseName, zip_project_archive_policy,
};

use super::errors::phase_error;

/// Platform-owned limits for the scorer-visible run tree.
#[derive(Debug, Clone, Copy)]
pub(super) struct OutputTreeLimits {
    pub(super) max_files: u64,
    pub(super) max_dirs: u64,
    pub(super) max_depth: u64,
}

/// Counted filesystem usage for a runner-owned tree.
#[derive(Debug, Default, Clone, Copy)]
struct TreeUsage {
    bytes: u64,
    files: u64,
    dirs: u64,
    max_depth: u64,
}

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
    inspect_tree(path, None, None).map(|usage| usage.bytes)
}

/// Validate scorer-visible output tree bounds before copying it to the scorer.
pub(super) fn validate_scorer_visible_output_tree(
    path: &Path,
    visible_run_name: &str,
    limits: OutputTreeLimits,
) -> Result<()> {
    inspect_tree(
        path,
        Some((visible_run_name, limits)),
        Some(visible_run_name),
    )
    .map(|_| ())
}

/// Inspect a filesystem tree without following symlinks.
fn inspect_tree(
    path: &Path,
    output_limits: Option<(&str, OutputTreeLimits)>,
    reject_non_regular_for_run: Option<&str>,
) -> Result<TreeUsage> {
    let mut usage = TreeUsage::default();
    let mut pending = vec![(path.to_path_buf(), 0u64)];

    while let Some((current, depth)) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&current)?;
        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            if let Some(visible_run_name) = reject_non_regular_for_run {
                return Err(non_regular_output_error(
                    visible_run_name,
                    "produced a symlink in its output tree",
                ));
            }
            usage.bytes = usage
                .bytes
                .checked_add(metadata.len())
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
            continue;
        }

        usage.max_depth = usage.max_depth.max(depth);
        if let Some((visible_run_name, limits)) = output_limits
            && depth > limits.max_depth
        {
            return Err(output_limit_error(
                visible_run_name,
                "depth",
                depth,
                limits.max_depth,
                "levels",
            ));
        }

        if metadata.is_dir() {
            usage.dirs = usage
                .dirs
                .checked_add(1)
                .ok_or_else(|| AppError::Runner("directory count overflow".to_string()))?;
            if let Some((visible_run_name, limits)) = output_limits
                && usage.dirs > limits.max_dirs
            {
                return Err(output_limit_error(
                    visible_run_name,
                    "directory",
                    usage.dirs,
                    limits.max_dirs,
                    "directories",
                ));
            }

            let child_depth = depth
                .checked_add(1)
                .ok_or_else(|| AppError::Runner("directory depth overflow".to_string()))?;
            for entry in std::fs::read_dir(&current)? {
                let entry = entry?;
                pending.push((entry.path(), child_depth));
            }
        } else if metadata.is_file() {
            usage.files = usage
                .files
                .checked_add(1)
                .ok_or_else(|| AppError::Runner("file count overflow".to_string()))?;
            if let Some((visible_run_name, limits)) = output_limits
                && usage.files > limits.max_files
            {
                return Err(output_limit_error(
                    visible_run_name,
                    "file",
                    usage.files,
                    limits.max_files,
                    "files",
                ));
            }
            usage.bytes = usage
                .bytes
                .checked_add(metadata.len())
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
        } else if let Some(visible_run_name) = reject_non_regular_for_run {
            return Err(non_regular_output_error(
                visible_run_name,
                "produced a non-regular file in its output tree",
            ));
        } else {
            usage.bytes = usage
                .bytes
                .checked_add(metadata.len())
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
        }
    }

    Ok(usage)
}

/// Build a resource-limit error for scorer-visible run tree bounds.
fn output_limit_error(
    visible_run_name: &str,
    limit_name: &str,
    actual: u64,
    limit: u64,
    unit: &str,
) -> AppError {
    phase_error(
        ZipProjectPhaseName::Run,
        ZipProjectPhaseFailureReason::ResourceLimit,
        format!(
            "run `{visible_run_name}` exceeded output {limit_name} limit: {actual} > {limit} {unit}"
        ),
        None,
    )
}

/// Build a non-regular-file output error.
fn non_regular_output_error(visible_run_name: &str, message: &str) -> AppError {
    phase_error(
        ZipProjectPhaseName::Run,
        ZipProjectPhaseFailureReason::RunnerError,
        format!("run `{visible_run_name}` {message}"),
        None,
    )
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

    /// Verifies byte accounting still sums regular file payloads.
    #[test]
    fn directory_size_counts_regular_file_bytes() {
        let root = temp_path("regular-file-size-root");
        std::fs::create_dir_all(root.join("nested")).expect("failed to create nested dir");
        std::fs::write(root.join("one.bin"), vec![b'a'; 7]).expect("failed to write first file");
        std::fs::write(root.join("nested/two.bin"), vec![b'b'; 11])
            .expect("failed to write nested file");

        let bytes = directory_size(&root).expect("directory size should succeed");

        assert_eq!(bytes, 18);
        drop(std::fs::remove_dir_all(root));
    }

    /// Verifies output tree validation rejects excessive regular files.
    #[test]
    fn output_tree_rejects_too_many_files() {
        let root = temp_path("too-many-output-files");
        std::fs::create_dir_all(&root).expect("failed to create root");
        std::fs::write(root.join("one.txt"), b"1").expect("failed to write first file");
        std::fs::write(root.join("two.txt"), b"2").expect("failed to write second file");

        let error = validate_scorer_visible_output_tree(
            &root,
            "run-0001",
            OutputTreeLimits {
                max_files: 1,
                max_dirs: 8,
                max_depth: 8,
            },
        )
        .expect_err("output tree should reject excessive files");

        assert!(error.to_string().contains("output file limit"));
        drop(std::fs::remove_dir_all(root));
    }

    /// Verifies output tree validation rejects excessive directories.
    #[test]
    fn output_tree_rejects_too_many_directories() {
        let root = temp_path("too-many-output-dirs");
        std::fs::create_dir_all(root.join("a")).expect("failed to create first dir");
        std::fs::create_dir_all(root.join("b")).expect("failed to create second dir");

        let error = validate_scorer_visible_output_tree(
            &root,
            "run-0001",
            OutputTreeLimits {
                max_files: 8,
                max_dirs: 2,
                max_depth: 8,
            },
        )
        .expect_err("output tree should reject excessive directories");

        assert!(error.to_string().contains("output directory limit"));
        drop(std::fs::remove_dir_all(root));
    }

    /// Verifies output tree validation rejects excessive path depth.
    #[test]
    fn output_tree_rejects_excessive_depth() {
        let root = temp_path("too-deep-output");
        std::fs::create_dir_all(root.join("a/b")).expect("failed to create deep dir");

        let error = validate_scorer_visible_output_tree(
            &root,
            "run-0001",
            OutputTreeLimits {
                max_files: 8,
                max_dirs: 8,
                max_depth: 1,
            },
        )
        .expect_err("output tree should reject excessive depth");

        assert!(error.to_string().contains("output depth limit"));
        drop(std::fs::remove_dir_all(root));
    }

    /// Verifies output tree validation rejects symlinks.
    #[cfg(unix)]
    #[test]
    fn output_tree_rejects_symlinks() {
        let root = temp_path("output-symlink-root");
        let outside = temp_path("output-symlink-target.txt");
        std::fs::create_dir_all(&root).expect("failed to create root");
        std::fs::write(&outside, b"outside").expect("failed to write outside file");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("failed to create symlink");

        let error = validate_scorer_visible_output_tree(
            &root,
            "run-0001",
            OutputTreeLimits {
                max_files: 8,
                max_dirs: 8,
                max_depth: 8,
            },
        )
        .expect_err("output tree should reject symlinks");

        assert!(error.to_string().contains("produced a symlink"));
        drop(std::fs::remove_file(outside));
        drop(std::fs::remove_dir_all(root));
    }

    /// Verifies output tree validation rejects special files.
    #[cfg(unix)]
    #[test]
    fn output_tree_rejects_special_files() {
        let root = temp_path("output-special-root");
        std::fs::create_dir_all(&root).expect("failed to create root");
        let socket_path = root.join("socket");
        let _listener =
            std::os::unix::net::UnixListener::bind(&socket_path).expect("failed to bind socket");

        let error = validate_scorer_visible_output_tree(
            &root,
            "run-0001",
            OutputTreeLimits {
                max_files: 8,
                max_dirs: 8,
                max_depth: 8,
            },
        )
        .expect_err("output tree should reject special files");

        assert!(error.to_string().contains("non-regular file"));
        drop(std::fs::remove_dir_all(root));
    }
}
