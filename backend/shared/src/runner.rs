//! Docker-backed evaluation runner for submitted artifacts.
//!
//! The runner mounts the immutable problem bundle, a safely extracted
//! submission archive, and a writable output directory into a network-isolated
//! Python container. The scorer must write a TS-compatible `result.json`; this
//! module validates that output before workers persist it.

use std::path::Path;
use std::time::Duration;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, KillContainerOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions, WaitContainerOptionsBuilder,
};
use futures::StreamExt;
use tokio::time::timeout;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::evaluation::{EvaluationJobPayload, ScorerRunResult, ScoringMode};
use crate::storage::Storage;

const MAX_RUNNER_ARTIFACT_BYTES: u64 = 20 * 1024 * 1024;
const MAX_RUNNER_FILE_COUNT: usize = 256;
const MAX_RUNNER_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024;

/// Validated scorer result plus the persisted runner log location.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Parsed and normalized `result.json` emitted by the scorer.
    pub result: ScorerRunResult,
    /// Storage-relative path to stdout and stderr captured from the container.
    pub log_path: String,
}

/// Execute one evaluation job in Docker and return the validated scorer result.
///
/// The scorer result may omit `mode` for compatibility with older bundles, but
/// if it declares a mode it must match the job type before normalization.
pub async fn execute_evaluation_job(
    docker: &Docker,
    config: &Config,
    job_id: &str,
    eval_type: ScoringMode,
    payload: &EvaluationJobPayload,
    storage: &dyn Storage,
) -> Result<ExecutionResult> {
    let working_root = Path::new(&config.storage_root)
        .join("eval-artifacts")
        .join(job_id);
    let extraction_root = std::env::temp_dir()
        .join("agentics-submissions")
        .join(job_id);
    let result_path = working_root.join("result.json");
    let log_path_rel = format!("eval-artifacts/{}/runner.log", job_id);

    tokio::fs::create_dir_all(&working_root).await?;
    tokio::fs::create_dir_all(&extraction_root).await?;

    let execution = async {
        extract_zip_safe(&payload.artifact_path, &extraction_root).await?;

        let container_name = format!("agentics-{}", job_id);
        let mode_str = eval_type.scorer_mode_arg();

        let mounts = vec![
            bollard::models::Mount {
                target: Some("/problem".to_string()),
                source: Some(payload.bundle_path.clone()),
                typ: Some(bollard::models::MountTypeEnum::BIND),
                read_only: Some(true),
                ..Default::default()
            },
            bollard::models::Mount {
                target: Some("/submission".to_string()),
                source: Some(extraction_root.to_string_lossy().to_string()),
                typ: Some(bollard::models::MountTypeEnum::BIND),
                read_only: Some(true),
                ..Default::default()
            },
            bollard::models::Mount {
                target: Some("/output".to_string()),
                source: Some(working_root.to_string_lossy().to_string()),
                typ: Some(bollard::models::MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            },
        ];

        let memory_bytes = config.runner_memory_limit_mb * 1024 * 1024;
        let nano_cpus = (config.runner_cpu_limit * 1_000_000_000.0) as i64;

        // Keep runner containers hermetic: no network, read-only inputs, and a
        // single writable output mount for result.json and captured artifacts.
        let host_config = bollard::models::HostConfig {
            network_mode: Some("none".to_string()),
            mounts: Some(mounts),
            auto_remove: Some(false),
            memory: Some(memory_bytes as i64),
            nano_cpus: Some(nano_cpus),
            ..Default::default()
        };

        let container_config = ContainerCreateBody {
            image: Some(config.runner_python_image.clone()),
            cmd: Some(vec![
                "python".to_string(),
                "/problem/scorer/run.py".to_string(),
                "--problem-dir".to_string(),
                "/problem".to_string(),
                "--submission-dir".to_string(),
                "/submission".to_string(),
                "--output-path".to_string(),
                "/output/result.json".to_string(),
                "--mode".to_string(),
                mode_str.to_string(),
            ]),
            working_dir: Some("/problem".to_string()),
            host_config: Some(host_config),
            labels: Some({
                let mut labels = std::collections::HashMap::new();
                labels.insert("agentics.job_id".to_string(), job_id.to_string());
                labels.insert("agentics.test".to_string(), "false".to_string());
                labels
            }),
            ..Default::default()
        };

        let create_opts = CreateContainerOptionsBuilder::default()
            .name(&container_name)
            .build();

        let create_resp = docker
            .create_container(Some(create_opts), container_config)
            .await
            .map_err(|e| AppError::Docker(format!("create container failed: {e}")))?;
        let container_id = create_resp.id;

        let run_result = run_created_container(
            docker,
            config,
            &container_id,
            &result_path,
            &log_path_rel,
            eval_type,
            storage,
        )
        .await;

        let remove_result = remove_container(docker, &container_id).await;
        match (run_result, remove_result) {
            (Ok(result), Ok(())) => Ok(result),
            (Ok(_), Err(cleanup_err)) => Err(cleanup_err),
            (Err(run_err), Ok(())) => Err(run_err),
            (Err(run_err), Err(cleanup_err)) => Err(AppError::Docker(format!(
                "{run_err}; additionally failed to remove runner container: {cleanup_err}"
            ))),
        }
    }
    .await;

    let cleanup = remove_extraction_root(&extraction_root).await;
    match (execution, cleanup) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(_), Err(cleanup_err)) => Err(cleanup_err),
        (Err(run_err), Ok(())) => Err(run_err),
        (Err(run_err), Err(cleanup_err)) => Err(AppError::Runner(format!(
            "{run_err}; additionally failed to remove extracted submission: {cleanup_err}"
        ))),
    }
}

async fn run_created_container(
    docker: &Docker,
    config: &Config,
    container_id: &str,
    result_path: &Path,
    log_path_rel: &str,
    eval_type: ScoringMode,
    storage: &dyn Storage,
) -> Result<ExecutionResult> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| AppError::Docker(format!("start container failed: {e}")))?;

    let wait_opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();

    let wait_result = timeout(
        Duration::from_secs(config.runner_timeout_sec),
        docker
            .wait_container(container_id, Some(wait_opts))
            .collect::<Vec<_>>(),
    )
    .await;

    let logs = collect_container_logs(docker, container_id).await?;

    let wait_ok = match wait_result {
        Ok(results) => results
            .into_iter()
            .flatten()
            .last()
            .is_none_or(|s| s.status_code == 0),
        Err(_) => {
            let kill_opts = KillContainerOptionsBuilder::default()
                .signal("SIGKILL")
                .build();
            docker
                .kill_container(container_id, Some(kill_opts))
                .await
                .map_err(|e| AppError::Docker(format!("kill timed out container failed: {e}")))?;
            false
        }
    };

    storage.put(log_path_rel, logs.as_bytes()).await?;

    if !wait_ok {
        return Err(AppError::Runner(
            "container exited with non-zero code or timed out".to_string(),
        ));
    }

    let result_raw = tokio::fs::read_to_string(result_path)
        .await
        .map_err(|e| AppError::Runner(format!("missing result.json: {e}")))?;

    let mut result: ScorerRunResult = serde_json::from_str(&result_raw)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;

    result
        .validate_for_mode(eval_type)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
    result.mode = Some(eval_type);

    Ok(ExecutionResult {
        result,
        log_path: log_path_rel.to_string(),
    })
}

async fn remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker
        .remove_container(container_id, Some(remove_opts))
        .await
        .map_err(|e| AppError::Docker(format!("remove runner container failed: {e}")))?;
    Ok(())
}

async fn remove_extraction_root(extraction_root: &Path) -> Result<()> {
    match tokio::fs::remove_dir_all(extraction_root).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io(e)),
    }
}

/// Connect to Docker using `AGENTICS_DOCKER_HOST` when configured, otherwise the local default.
pub fn connect_docker(config: &Config) -> Result<Docker> {
    match config
        .docker_host
        .as_deref()
        .map(str::trim)
        .filter(|host| !host.is_empty())
    {
        Some(host) => Docker::connect_with_host(host)
            .map_err(|e| AppError::Docker(format!("failed to connect to Docker host {host}: {e}"))),
        None => Docker::connect_with_defaults()
            .map_err(|e| AppError::Docker(format!("failed to connect to Docker: {e}"))),
    }
}

async fn collect_container_logs(docker: &Docker, container_id: &str) -> Result<String> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();

    let mut logs = docker.logs(container_id, Some(opts));
    let mut output = String::new();

    while let Some(chunk) = logs.next().await {
        match chunk {
            Ok(LogOutput::StdOut { message }) | Ok(LogOutput::StdErr { message }) => {
                output.push_str(&String::from_utf8_lossy(&message));
            }
            Ok(LogOutput::Console { message }) => {
                output.push_str(&String::from_utf8_lossy(&message));
            }
            Err(e) => {
                return Err(AppError::Docker(format!(
                    "collect container logs failed: {e}"
                )));
            }
            _ => {}
        }
    }

    Ok(output)
}

/// Extract a submitted ZIP archive while ignoring entries that escape `target_dir`.
async fn extract_zip_safe(artifact_path: &str, target_dir: &Path) -> Result<()> {
    let artifact_size = tokio::fs::metadata(artifact_path).await?.len();
    if artifact_size > MAX_RUNNER_ARTIFACT_BYTES {
        return Err(AppError::Validation(format!(
            "submission archive must be at most {} bytes",
            MAX_RUNNER_ARTIFACT_BYTES
        )));
    }

    let artifact_path = artifact_path.to_string();
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_zip_safe_blocking(&artifact_path, &target_dir))
        .await
        .map_err(|e| AppError::Internal(format!("zip extraction task failed: {e}")))?
}

fn extract_zip_safe_blocking(artifact_path: &str, target_dir: &Path) -> Result<()> {
    let reader = std::fs::File::open(artifact_path)?;
    let mut archive = zip::ZipArchive::new(reader)?;

    if archive.len() > MAX_RUNNER_FILE_COUNT {
        return Err(AppError::Validation(format!(
            "submission archive must contain at most {} entries",
            MAX_RUNNER_FILE_COUNT
        )));
    }

    let canonical_target = target_dir
        .canonicalize()
        .unwrap_or_else(|_| target_dir.to_path_buf());
    let mut total_uncompressed_size = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        total_uncompressed_size = total_uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| AppError::Validation("submission archive is too large".to_string()))?;
        if total_uncompressed_size > MAX_RUNNER_UNCOMPRESSED_BYTES {
            return Err(AppError::Validation(format!(
                "submission archive must expand to at most {} bytes",
                MAX_RUNNER_UNCOMPRESSED_BYTES
            )));
        }

        // ZipArchive::enclosed_name covers obvious traversal. Canonicalization
        // keeps symlinked parent directories from escaping the extraction root.
        let canonical_out = if outpath.exists() {
            outpath.canonicalize().unwrap_or_else(|_| outpath.clone())
        } else {
            let parent = outpath.parent().unwrap_or(target_dir);
            let canonical_parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            canonical_parent.join(outpath.file_name().unwrap_or_default())
        };

        if !canonical_out.starts_with(&canonical_target) {
            continue;
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

/// Pull the configured runner image before the worker starts claiming jobs.
pub async fn pre_pull_image(docker: &Docker, image: &str) -> Result<()> {
    use bollard::query_parameters::CreateImageOptionsBuilder;

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .build();

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        if let Err(e) = item {
            return Err(AppError::Docker(format!(
                "failed to pull image {image}: {e}"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-runner-{name}-{}", uuid::Uuid::new_v4()))
    }

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
            ],
        );

        extract_zip_safe(&zip_path.to_string_lossy(), &target_dir)
            .await
            .expect("extraction should succeed");

        let extracted_files = std::fs::read_dir(&target_dir)
            .expect("failed to read target dir")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("failed to collect target dir entries");
        assert_eq!(extracted_files.len(), 1);
        assert_eq!(extracted_files[0].file_name(), "main.py");

        let _ = std::fs::remove_file(zip_path);
        let _ = std::fs::remove_dir_all(target_dir);
    }

    #[tokio::test]
    async fn extract_zip_safe_rejects_too_many_entries() {
        let zip_path = temp_path("too-many.zip");
        let target_dir = temp_path("too-many-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        let entries = (0..=MAX_RUNNER_FILE_COUNT)
            .map(|i| (format!("file-{i}.txt"), Vec::new()))
            .collect();
        write_zip(&zip_path, entries);

        let result = extract_zip_safe(&zip_path.to_string_lossy(), &target_dir).await;

        assert!(
            matches!(result, Err(AppError::Validation(message)) if message.contains("at most"))
        );

        let _ = std::fs::remove_file(zip_path);
        let _ = std::fs::remove_dir_all(target_dir);
    }
}
