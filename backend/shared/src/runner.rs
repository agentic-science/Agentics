use std::path::Path;
use std::time::Duration;

use bollard::container::LogOutput;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptionsBuilder, KillContainerOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder, StartContainerOptions, WaitContainerOptionsBuilder};
use bollard::Docker;
use futures::StreamExt;
use tokio::time::timeout;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::evaluation::{EvaluationJobPayload, ScorerRunResult, ScoringMode};
use crate::storage::Storage;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub result: ScorerRunResult,
    pub log_path: String,
}

pub async fn execute_evaluation_job(
    docker: &Docker,
    config: &Config,
    job_id: &str,
    eval_type: ScoringMode,
    payload: &EvaluationJobPayload,
    storage: &dyn Storage,
) -> Result<ExecutionResult> {
    let working_root = Path::new(&config.storage_root).join("eval-artifacts").join(job_id);
    let extraction_root = std::env::temp_dir().join("llm-oj-submissions").join(job_id);
    let result_path = working_root.join("result.json");
    let log_path_rel = format!("eval-artifacts/{}/runner.log", job_id);

    tokio::fs::create_dir_all(&working_root).await?;
    tokio::fs::create_dir_all(&extraction_root).await?;

    // Extract ZIP
    extract_zip_safe(&payload.artifact_path, &extraction_root).await?;

    // Create container
    let container_name = format!("llm-oj-{}", job_id);
    let mode_str = match eval_type {
        ScoringMode::Official => "official",
        ScoringMode::Public => "public",
    };

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
            labels.insert("llm-oj.job_id".to_string(), job_id.to_string());
            labels.insert("llm-oj.test".to_string(), "false".to_string());
            labels
        }),
        ..Default::default()
    };

    let create_opts = CreateContainerOptionsBuilder::default()
        .name(&container_name)
        .build();

    let create_resp = docker.create_container(Some(create_opts), container_config).await
        .map_err(|e| AppError::Docker(format!("create container failed: {e}")))?;

    docker.start_container(&create_resp.id, None::<StartContainerOptions>).await
        .map_err(|e| AppError::Docker(format!("start container failed: {e}")))?;

    // Wait for container with timeout
    let wait_opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();

    let wait_result = timeout(
        Duration::from_secs(config.runner_timeout_sec),
        docker.wait_container(&create_resp.id, Some(wait_opts)).collect::<Vec<_>>(),
    ).await;

    // Collect logs regardless of outcome
    let logs = collect_container_logs(docker, &create_resp.id).await.unwrap_or_default();

    let wait_ok = match wait_result {
        Ok(results) => {
            results.into_iter().flatten().last().is_none_or(|s| s.status_code == 0)
        }
        Err(_) => {
            // Timeout — kill container
            let kill_opts = KillContainerOptionsBuilder::default()
                .signal("SIGKILL")
                .build();
            let _ = docker.kill_container(&create_resp.id, Some(kill_opts)).await;
            false
        }
    };

    // Write logs
    storage.put(&log_path_rel, logs.as_bytes()).await?;

    // Remove container
    let remove_opts = RemoveContainerOptionsBuilder::default()
        .force(true)
        .build();
    let _ = docker.remove_container(&create_resp.id, Some(remove_opts)).await;

    if !wait_ok {
        return Err(AppError::Runner("container exited with non-zero code or timed out".to_string()));
    }

    // Read result
    let result_raw = tokio::fs::read_to_string(&result_path).await
        .map_err(|e| AppError::Runner(format!("missing result.json: {e}")))?;

    let mut result: ScorerRunResult = serde_json::from_str(&result_raw)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;

    result.mode = Some(eval_type);

    // Cleanup extraction dir
    let _ = tokio::fs::remove_dir_all(&extraction_root).await;

    Ok(ExecutionResult {
        result,
        log_path: log_path_rel,
    })
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
            _ => {}
        }
    }

    Ok(output)
}

async fn extract_zip_safe(artifact_path: &str, target_dir: &Path) -> Result<()> {
    let artifact_bytes = tokio::fs::read(artifact_path).await?;
    let reader = std::io::Cursor::new(artifact_bytes);
    let mut archive = zip::ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        // Security: ensure the extracted path stays within target_dir
        let canonical_target = target_dir.canonicalize().unwrap_or_else(|_| target_dir.to_path_buf());
        let canonical_out = if outpath.exists() {
            outpath.canonicalize().unwrap_or_else(|_| outpath.clone())
        } else {
            let parent = outpath.parent().unwrap_or(target_dir);
            let canonical_parent = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
            canonical_parent.join(outpath.file_name().unwrap_or_default())
        };

        if !canonical_out.starts_with(&canonical_target) {
            continue; // Path traversal attempt, skip
        }

        if file.is_dir() {
            tokio::fs::create_dir_all(&outpath).await?;
        } else {
            if let Some(parent) = outpath.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            use tokio::io::AsyncWriteExt;
            let mut outfile = tokio::fs::File::create(&outpath).await?;
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf)?;
            outfile.write_all(&buf).await?;
        }
    }

    Ok(())
}

pub async fn pre_pull_image(docker: &Docker, image: &str) -> Result<()> {
    use bollard::query_parameters::CreateImageOptionsBuilder;

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .build();

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        if let Err(e) = item {
            return Err(AppError::Docker(format!("failed to pull image {image}: {e}")));
        }
    }

    Ok(())
}
