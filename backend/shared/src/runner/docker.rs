use std::collections::HashMap;
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{
    ContainerCreateBody, DeviceRequest, HostConfig, HostConfigLogConfig, Mount, MountTypeEnum,
    ResourcesUlimits,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, KillContainerOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions, WaitContainerOptionsBuilder,
};
use futures::StreamExt;
use tokio::time::timeout;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::challenge::{BenchmarkAccelerator, DockerPlatform};
use crate::zip_project::ZipProjectPhaseLimits;

#[derive(Debug)]
pub(super) struct ContainerRequest {
    pub(super) name: String,
    pub(super) image: String,
    pub(super) cmd: Vec<String>,
    pub(super) env: Vec<String>,
    pub(super) mounts: Vec<Mount>,
    pub(super) working_dir: String,
    pub(super) docker_platform: DockerPlatform,
    pub(super) accelerator: BenchmarkAccelerator,
    pub(super) limits: ZipProjectPhaseLimits,
    pub(super) docker_layer_quota_mb: Option<u64>,
}

#[derive(Debug)]
pub(super) struct ContainerOutcome {
    pub(super) exit_code: i64,
    pub(super) logs: String,
    pub(super) timed_out: bool,
    pub(super) wall_time_ms: u64,
}

pub(super) async fn run_container(
    docker: &Docker,
    request: ContainerRequest,
) -> Result<ContainerOutcome> {
    let memory_bytes = request
        .limits
        .memory_limit_mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| AppError::Runner("memory limit overflow".to_string()))?;
    let memory = i64::try_from(memory_bytes)
        .map_err(|_| AppError::Runner("memory limit exceeds Docker API range".to_string()))?;
    let nano_cpus = i64::from(request.limits.cpu_limit_millis)
        .checked_mul(1_000_000)
        .ok_or_else(|| AppError::Runner("CPU limit overflow".to_string()))?;
    let log_limit_bytes = request.limits.log_limit_bytes;
    let host_config = HostConfig {
        network_mode: Some(
            request
                .limits
                .network_access
                .docker_network_mode()
                .to_string(),
        ),
        mounts: Some(request.mounts),
        auto_remove: Some(false),
        memory: Some(memory),
        memory_swap: Some(memory),
        nano_cpus: Some(nano_cpus),
        pids_limit: Some(256),
        ulimits: Some(vec![
            ResourcesUlimits {
                name: Some("nofile".to_string()),
                soft: Some(1024),
                hard: Some(1024),
            },
            ResourcesUlimits {
                name: Some("nproc".to_string()),
                soft: Some(256),
                hard: Some(256),
            },
        ]),
        cap_drop: Some(vec!["ALL".to_string()]),
        security_opt: Some(vec!["no-new-privileges:true".to_string()]),
        privileged: Some(false),
        publish_all_ports: Some(false),
        init: Some(true),
        oom_kill_disable: Some(false),
        log_config: Some(docker_log_config(log_limit_bytes)),
        storage_opt: docker_storage_opt(request.docker_layer_quota_mb),
        runtime: gpu_runtime(request.accelerator),
        device_requests: gpu_device_requests(request.accelerator),
        ..Default::default()
    };
    let container_config = ContainerCreateBody {
        image: Some(request.image),
        cmd: Some(request.cmd),
        env: Some(request.env),
        working_dir: Some(request.working_dir),
        host_config: Some(host_config),
        labels: Some({
            let mut labels = std::collections::HashMap::new();
            labels.insert("agentics.runner".to_string(), "zip_project".to_string());
            labels
        }),
        ..Default::default()
    };

    let create_opts = CreateContainerOptionsBuilder::default()
        .name(&request.name)
        .platform(request.docker_platform.as_str())
        .build();
    let create_resp = docker
        .create_container(Some(create_opts), container_config)
        .await
        .map_err(|e| AppError::Docker(format!("create container failed: {e}")))?;
    let container_id = create_resp.id;

    let run_result = run_created_container(
        docker,
        &container_id,
        request.limits.timeout_sec,
        log_limit_bytes,
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

/// Pull an image before creating a runner container.
pub(super) async fn pre_pull_image(
    docker: &Docker,
    image: &str,
    docker_platform: DockerPlatform,
) -> Result<()> {
    use bollard::query_parameters::CreateImageOptionsBuilder;

    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .platform(docker_platform.as_str())
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

pub(super) fn bind_mount(path: &std::path::Path, target: &str, read_only: bool) -> Mount {
    Mount {
        target: Some(target.to_string()),
        source: Some(path.to_string_lossy().to_string()),
        typ: Some(MountTypeEnum::BIND),
        read_only: Some(read_only),
        ..Default::default()
    }
}

async fn run_created_container(
    docker: &Docker,
    container_id: &str,
    timeout_sec: u64,
    log_limit_bytes: u64,
) -> Result<ContainerOutcome> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| AppError::Docker(format!("start container failed: {e}")))?;
    let started = Instant::now();

    let wait_opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();
    let wait_result = timeout(
        Duration::from_secs(timeout_sec),
        docker
            .wait_container(container_id, Some(wait_opts))
            .collect::<Vec<_>>(),
    )
    .await;

    let (exit_code, timed_out) = match wait_result {
        Ok(results) => {
            let exit_code = results
                .into_iter()
                .flatten()
                .last()
                .map(|status| status.status_code)
                .unwrap_or(1);
            (exit_code, false)
        }
        Err(_) => {
            let kill_opts = KillContainerOptionsBuilder::default()
                .signal("SIGKILL")
                .build();
            docker
                .kill_container(container_id, Some(kill_opts))
                .await
                .map_err(|e| AppError::Docker(format!("kill timed out container failed: {e}")))?;
            (124, true)
        }
    };
    let wall_time_ms = duration_millis(started.elapsed());
    let (logs, _logs_truncated) =
        collect_container_logs(docker, container_id, log_limit_bytes).await?;
    Ok(ContainerOutcome {
        exit_code,
        logs,
        timed_out,
        wall_time_ms,
    })
}

fn duration_millis(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

async fn remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker
        .remove_container(container_id, Some(remove_opts))
        .await
        .map_err(|e| AppError::Docker(format!("remove runner container failed: {e}")))?;
    Ok(())
}

fn docker_log_config(limit_bytes: u64) -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert("max-size".to_string(), format!("{}b", limit_bytes.max(1)));
    config.insert("max-file".to_string(), "1".to_string());

    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

fn docker_storage_opt(limit_mb: Option<u64>) -> Option<HashMap<String, String>> {
    limit_mb.map(|limit_mb| {
        let mut storage_opt = HashMap::new();
        storage_opt.insert("size".to_string(), format!("{limit_mb}m"));
        storage_opt
    })
}

fn gpu_runtime(accelerator: BenchmarkAccelerator) -> Option<String> {
    match accelerator {
        BenchmarkAccelerator::Cpu => None,
        BenchmarkAccelerator::Gpu => Some("nvidia".to_string()),
    }
}

fn gpu_device_requests(accelerator: BenchmarkAccelerator) -> Option<Vec<DeviceRequest>> {
    match accelerator {
        BenchmarkAccelerator::Cpu => None,
        BenchmarkAccelerator::Gpu => Some(vec![DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(-1),
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        }]),
    }
}

async fn collect_container_logs(
    docker: &Docker,
    container_id: &str,
    limit_bytes: u64,
) -> Result<(String, bool)> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();
    let mut logs = docker.logs(container_id, Some(opts));
    let mut output = Vec::new();
    let mut truncated = false;
    let limit = usize::try_from(limit_bytes).unwrap_or(usize::MAX);

    while let Some(chunk) = logs.next().await {
        match chunk {
            Ok(LogOutput::StdOut { message })
            | Ok(LogOutput::StdErr { message })
            | Ok(LogOutput::Console { message }) => {
                append_bounded_log_bytes(&mut output, &message, limit, &mut truncated);
                if output.len() >= limit {
                    truncated = true;
                    break;
                }
            }
            Err(e) => {
                return Err(AppError::Docker(format!(
                    "collect container logs failed: {e}"
                )));
            }
            _ => {}
        }
    }

    let mut output = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        output.push_str(&format!(
            "\n[agentics] container logs truncated at {limit_bytes} bytes\n"
        ));
    }

    Ok((output, truncated))
}

fn append_bounded_log_bytes(
    output: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    truncated: &mut bool,
) {
    if output.len() >= limit {
        *truncated = !chunk.is_empty();
        return;
    }

    let remaining = limit.saturating_sub(output.len());
    if chunk.len() > remaining {
        output.extend(chunk.iter().take(remaining).copied());
        *truncated = true;
    } else {
        output.extend_from_slice(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_log_append_truncates_by_byte_limit() {
        let mut output = Vec::new();
        let mut truncated = false;

        append_bounded_log_bytes(&mut output, b"abcdef", 4, &mut truncated);

        assert_eq!(output, b"abcd");
        assert!(truncated);
    }
}
