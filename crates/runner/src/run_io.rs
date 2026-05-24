use std::path::Path;

use serde::Serialize;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufReader};

use super::errors::phase_error;
use super::filesystem::{OutputTreeLimits, validate_evaluator_visible_output_tree};
use super::{ContainerOutcome, ScoringMode};
use agentics_contracts::zip_project::{ZipProjectPhaseFailureReason, ZipProjectPhaseName};
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{
    ChallengeRunInputFile, ChallengeRunInterface, ChallengeRunSpec,
};
use agentics_domain::models::names::RunName;

/// Per-run metadata written by the worker for challenge-owned evaluators.
#[derive(Debug, Clone, Serialize)]
struct SolutionRunMetadata {
    run_name: String,
    interface: ChallengeRunInterface,
    exit_code: i64,
    timed_out: bool,
    wall_time_ms: u64,
    stdout_path: String,
    stderr_path: String,
    output_dir: String,
}

#[cfg(unix)]
/// Handles make container writable tree for this module.
pub(super) async fn make_container_writable_tree(root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut pending = vec![root];
        while let Some(path) = pending.pop() {
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if !metadata.is_dir() && !metadata.is_file() {
                continue;
            }

            let mut permissions = metadata.permissions();
            let writable_bits = if metadata.is_dir() { 0o777 } else { 0o666 };
            permissions.set_mode(permissions.mode() | writable_bits);
            std::fs::set_permissions(&path, permissions)?;

            if metadata.is_dir() {
                for entry in std::fs::read_dir(&path)? {
                    let entry = entry?;
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("container writable chmod task failed: {e}")))?
}

#[cfg(unix)]
/// Handles make container readable tree for read-only Docker bind mounts.
pub(super) async fn make_container_readable_tree(root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut pending = vec![root];
        while let Some(path) = pending.pop() {
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if !metadata.is_dir() && !metadata.is_file() {
                continue;
            }

            let mut permissions = metadata.permissions();
            let current_mode = permissions.mode();
            let readable_bits = if metadata.is_dir() {
                0o755
            } else if current_mode & 0o111 != 0 {
                0o555
            } else {
                0o444
            };
            let new_mode = current_mode | readable_bits;
            if new_mode != current_mode {
                permissions.set_mode(new_mode);
                std::fs::set_permissions(&path, permissions)?;
            }

            if metadata.is_dir() {
                for entry in std::fs::read_dir(&path)? {
                    let entry = entry?;
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("container readable chmod task failed: {e}")))?
}

#[cfg(not(unix))]
/// Handles make container writable tree for this module.
pub(super) async fn make_container_writable_tree(_root: &Path) -> Result<()> {
    Ok(())
}

#[cfg(not(unix))]
/// Handles make container readable tree for read-only Docker bind mounts.
pub(super) async fn make_container_readable_tree(_root: &Path) -> Result<()> {
    Ok(())
}

/// Build an opaque solution-visible run name for one invocation index.
pub(super) fn run_alias(index: usize) -> Result<RunName> {
    let display_index = index
        .checked_add(1)
        .ok_or_else(|| ServiceError::Internal("run alias index overflowed".to_string()))?;
    RunName::try_new(format!("run-{display_index:04}"))
        .map_err(|e| ServiceError::Internal(format!("generated invalid run alias: {e}")))
}

/// Copy a solution run tree into the evaluator-visible area while rejecting symlinks and devices.
pub(super) async fn copy_evaluator_visible_run_tree(
    source: &Path,
    destination: &Path,
    visible_run_name: &str,
    limits: OutputTreeLimits,
) -> Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    let visible_run_name = visible_run_name.to_string();
    tokio::task::spawn_blocking(move || {
        copy_evaluator_visible_run_tree_blocking(&source, &destination, &visible_run_name, limits)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("evaluator output copy task failed: {e}")))?
}

/// Blocking implementation for evaluator-visible run tree sanitization and copy.
fn copy_evaluator_visible_run_tree_blocking(
    source: &Path,
    destination: &Path,
    visible_run_name: &str,
    limits: OutputTreeLimits,
) -> Result<()> {
    validate_evaluator_visible_output_tree(source, visible_run_name, limits)?;

    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    while let Some((current_source, current_destination)) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&current_source)?;
        if metadata.file_type().is_symlink() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!("run `{visible_run_name}` produced a symlink in its output tree"),
                None,
            ));
        }
        if metadata.is_dir() {
            std::fs::create_dir_all(&current_destination)?;
            for entry in std::fs::read_dir(&current_source)? {
                let entry = entry?;
                pending.push((entry.path(), current_destination.join(entry.file_name())));
            }
        } else if metadata.is_file() {
            if let Some(parent) = current_destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&current_source, &current_destination)?;
        } else {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!("run `{visible_run_name}` produced a non-regular file in its output tree"),
                None,
            ));
        }
    }

    Ok(())
}

/// Handles materialize run io for this module.
pub(super) async fn materialize_run_io(
    run: &ChallengeRunSpec,
    visible_run_name: &str,
    eval_type: ScoringMode,
    input_source_root: &Path,
    io_root: &Path,
    input_dir: &Path,
) -> Result<()> {
    let stdin = match (&run.stdin_json, &run.stdin_text) {
        (Some(value), None) => serde_json::to_string(value)
            .map_err(|e| ServiceError::Internal(format!("serialize stdin_json failed: {e}")))?,
        (None, Some(value)) => value.clone(),
        _ => String::new(),
    };
    tokio::fs::write(io_root.join("stdin.txt"), stdin).await?;
    materialize_input_files(
        &run.input_files,
        visible_run_name,
        eval_type,
        input_source_root,
        input_dir,
    )
    .await
}

/// Materialize declared challenge-owned input files into a container input directory.
pub(super) async fn materialize_input_files(
    input_files: &[ChallengeRunInputFile],
    visible_name: &str,
    eval_type: ScoringMode,
    input_source_root: &Path,
    input_dir: &Path,
) -> Result<()> {
    for input in input_files {
        write_run_input_file(input_source_root, input_dir, input, visible_name, eval_type).await?;
    }
    Ok(())
}

/// Writes run input file to the target path.
async fn write_run_input_file(
    input_source_root: &Path,
    input_dir: &Path,
    input: &ChallengeRunInputFile,
    visible_run_name: &str,
    eval_type: ScoringMode,
) -> Result<()> {
    let path = input_dir.join(input.path.as_path());
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(source_path) = &input.source_path {
        let source = tokio::fs::File::open(input_source_root.join(source_path.as_path()))
            .await
            .map_err(|e| {
                let source = match eval_type {
                    ScoringMode::Validation => format!(" source `{source_path}`"),
                    ScoringMode::Official => String::new(),
                };
                ServiceError::Runner(format!(
                    "copy run `{visible_run_name}` input{source} failed: {e}"
                ))
            })?;
        let mut target = create_run_input_destination(&path, input, visible_run_name).await?;
        tokio::io::copy(&mut BufReader::new(source), &mut target)
            .await
            .map_err(|e| {
                ServiceError::Runner(format!(
                    "copy run `{visible_run_name}` input `{}` failed: {e}",
                    input.path
                ))
            })?;
        return Ok(());
    }

    let content = if let Some(value) = &input.content {
        value.clone()
    } else if let Some(value) = &input.content_json {
        serde_json::to_string(value)
            .map_err(|e| ServiceError::Internal(format!("serialize content_json failed: {e}")))?
    } else {
        String::new()
    };
    let mut target = create_run_input_destination(&path, input, visible_run_name).await?;
    target.write_all(content.as_bytes()).await.map_err(|e| {
        ServiceError::Runner(format!(
            "write run `{visible_run_name}` input `{}` failed: {e}",
            input.path
        ))
    })?;
    Ok(())
}

/// Creates a run input destination without overwriting any existing file.
async fn create_run_input_destination(
    path: &Path,
    input: &ChallengeRunInputFile,
    visible_run_name: &str,
) -> Result<tokio::fs::File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .map_err(|e| {
            ServiceError::Runner(format!(
                "create run `{visible_run_name}` input `{}` failed: {e}",
                input.path
            ))
        })
}

/// Writes run metadata to the target path.
pub(super) async fn write_run_metadata(
    io_root: &Path,
    run: &ChallengeRunSpec,
    visible_run_name: &str,
    outcome: &ContainerOutcome,
) -> Result<()> {
    let metadata = SolutionRunMetadata {
        run_name: run.run_name.to_string(),
        interface: run.interface,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        wall_time_ms: outcome.wall_time_ms,
        stdout_path: format!("/solution-runs/{}/stdout.txt", run.run_name),
        stderr_path: format!("/solution-runs/{}/stderr.txt", run.run_name),
        output_dir: format!("/solution-runs/{}/output", run.run_name),
    };
    let bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|e| ServiceError::Internal(format!("serialize run metadata failed: {e}")))?;
    let metadata_path = io_root.join("agentics-run.json");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&metadata_path)
        .await
        .map_err(|e| {
            phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{visible_run_name}` used reserved metadata path `agentics-run.json`: {e}"
                ),
                None,
            )
        })?;
    file.write_all(&bytes).await?;
    Ok(())
}

/// Ensures declared outputs exist before continuing.
pub(super) async fn ensure_declared_outputs_exist(
    run: &ChallengeRunSpec,
    visible_run_name: &str,
    output_dir: &Path,
) -> Result<()> {
    for output in &run.output_files {
        let output_path = output_dir.join(output.as_path());
        let metadata = tokio::fs::symlink_metadata(&output_path)
            .await
            .map_err(|_| {
                phase_error(
                    ZipProjectPhaseName::Run,
                    ZipProjectPhaseFailureReason::RunnerError,
                    format!(
                        "run `{visible_run_name}` did not produce declared output file `{output}`"
                    ),
                    None,
                )
            })?;
        if metadata.file_type().is_symlink() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!("run `{visible_run_name}` declared output file `{output}` is a symlink"),
                None,
            ));
        }
        if !metadata.is_file() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{visible_run_name}` declared output path `{output}` is not a regular file"
                ),
                None,
            ));
        }
    }
    Ok(())
}

/// Handles run interface for this module.
pub(super) fn run_interface(interface: ChallengeRunInterface) -> &'static str {
    match interface {
        ChallengeRunInterface::Stdio => "stdio",
        ChallengeRunInterface::FileSystem => "file_system",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use super::{copy_evaluator_visible_run_tree, materialize_input_files, write_run_metadata};
    use crate::ContainerOutcome;
    use crate::filesystem::OutputTreeLimits;
    use agentics_domain::models::challenge::{
        ChallengeRunInputFile, ChallengeRunInterface, ChallengeRunSpec,
    };
    use agentics_domain::models::names::RunName;
    use agentics_domain::models::paths::RunInputPath;

    /// Return generous test output tree limits.
    fn test_output_limits() -> OutputTreeLimits {
        OutputTreeLimits {
            max_files: 8192,
            max_dirs: 1024,
            max_depth: 32,
        }
    }

    /// Verifies that solution-created symlinks cannot redirect worker metadata writes.
    #[cfg(unix)]
    #[tokio::test]
    async fn write_run_metadata_rejects_reserved_symlink() {
        let root =
            std::env::temp_dir().join(format!("agentics-run-metadata-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test root should be created");
        let target = root.join("outside.json");
        std::os::unix::fs::symlink(&target, root.join("agentics-run.json"))
            .expect("reserved symlink should be created");

        let run = ChallengeRunSpec {
            run_name: RunName::try_new("case1").expect("run name should parse"),
            interface: ChallengeRunInterface::FileSystem,
            stdin_json: None,
            stdin_text: None,
            input_files: Vec::new(),
            output_files: Vec::new(),
        };
        let outcome = ContainerOutcome {
            exit_code: 0,
            logs: String::new(),
            timed_out: false,
            wall_time_ms: 1,
        };

        let error = write_run_metadata(&root, &run, "run-0001", &outcome)
            .await
            .expect_err("metadata write should reject a pre-existing symlink");
        assert!(
            error.to_string().contains("reserved metadata path"),
            "unexpected error: {error}"
        );
        assert!(!target.exists());

        fs::remove_dir_all(root).expect("test root should clean up");
    }

    /// Verifies that evaluator-facing copies reject undeclared symlinks anywhere in the run tree.
    #[cfg(unix)]
    #[tokio::test]
    async fn evaluator_visible_run_tree_rejects_extra_symlink() {
        let root =
            std::env::temp_dir().join(format!("agentics-run-tree-symlink-test-{}", Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("output")).expect("source output should be created");
        std::os::unix::fs::symlink("/challenge/private", source.join("output/extra"))
            .expect("extra symlink should be created");

        let error = copy_evaluator_visible_run_tree(
            &source,
            &destination,
            "run-0001",
            test_output_limits(),
        )
        .await
        .expect_err("evaluator-facing copy should reject symlinks");
        assert!(
            error.to_string().contains("produced a symlink"),
            "unexpected error: {error}"
        );
        assert!(!destination.join("output/extra").exists());

        fs::remove_dir_all(root).expect("test root should clean up");
    }

    /// Verifies output tree limits are checked before evaluator-facing copy starts.
    #[tokio::test]
    async fn evaluator_visible_run_tree_limit_rejects_before_copying() {
        let root =
            std::env::temp_dir().join(format!("agentics-run-tree-limit-test-{}", Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("output")).expect("source output should be created");
        fs::write(source.join("stdout.txt"), b"ok").expect("stdout should be created");
        fs::write(source.join("output/result.txt"), b"ok").expect("output should be created");

        let error = copy_evaluator_visible_run_tree(
            &source,
            &destination,
            "run-0001",
            OutputTreeLimits {
                max_files: 1,
                max_dirs: 32,
                max_depth: 32,
            },
        )
        .await
        .expect_err("evaluator-facing copy should reject excessive files");

        assert!(
            error.to_string().contains("output file limit"),
            "unexpected error: {error}"
        );
        assert!(!destination.exists());

        fs::remove_dir_all(root).expect("test root should clean up");
    }

    /// Verifies input materialization never overwrites an existing target path.
    #[tokio::test]
    async fn materialize_input_files_rejects_existing_destination() {
        let root = std::env::temp_dir().join(format!("agentics-run-input-test-{}", Uuid::new_v4()));
        let input_dir = root.join("input");
        fs::create_dir_all(&input_dir).expect("input dir should be created");
        fs::write(input_dir.join("case.txt"), b"existing").expect("existing input should be set");
        let input = ChallengeRunInputFile {
            path: RunInputPath::try_new("case.txt").expect("input path should parse"),
            source_path: None,
            content: Some("replacement".to_string()),
            content_json: None,
        };

        let error = materialize_input_files(
            &[input],
            "run-0001",
            agentics_domain::models::evaluation::ScoringMode::Validation,
            &root,
            &input_dir,
        )
        .await
        .expect_err("existing destination should be rejected");
        assert!(
            error
                .to_string()
                .contains("create run `run-0001` input `case.txt` failed"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read_to_string(input_dir.join("case.txt")).expect("existing input should remain"),
            "existing"
        );

        fs::remove_dir_all(root).expect("test root should clean up");
    }
}
