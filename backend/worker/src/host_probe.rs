//! Hosted runner profile probe enforcement.

use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use agentics_config::{Config, HostProbeMode, WorkerAccelerators};
use agentics_contracts::zip_project::DockerNetworkMode;
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{ContainerCreateBody, DeviceRequest, HostConfig, HostConfigLogConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions, WaitContainerOptionsBuilder,
};
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::time::timeout;
use tracing::{info, warn};
use uuid::Uuid;

const MAX_PROBE_OUTPUT_BYTES: usize = 8192;
const GPU_PROBE_TIMEOUT_SECS: u64 = 30;
const HOST_PROBE_TIMEOUT_SECS: u64 = 60;

/// Run the configured hosted profile probe before the worker accepts jobs.
pub(crate) async fn enforce_host_probe(config: &Config) -> anyhow::Result<()> {
    match config.host_probe_mode {
        HostProbeMode::Off => Ok(()),
        HostProbeMode::Warn | HostProbeMode::Require => {
            let mode = config.host_probe_mode;
            let command = config.host_probe_command.as_str();
            let output = run_host_probe_command(command, mode).await;
            match output {
                Ok(output)
                    if output.status.success() && !output_contains_failure(&output.stdout) =>
                {
                    info!("host profile probe passed");
                    Ok(())
                }
                Ok(output) if output.status.success() => handle_probe_failure(
                    mode,
                    format_probe_failure(
                        Some(output.status),
                        &output.stdout,
                        &output.stderr,
                        output.truncated,
                    ),
                ),
                Ok(output) => handle_probe_failure(
                    mode,
                    format_probe_failure(
                        Some(output.status),
                        &output.stdout,
                        &output.stderr,
                        output.truncated,
                    ),
                ),
                Err(error) => handle_probe_failure(
                    mode,
                    format!("failed to run host profile probe `{command}`: {error}"),
                ),
            }
        }
    }
}

#[derive(Debug)]
struct HostProbeCommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}

async fn run_host_probe_command(
    command: &str,
    mode: HostProbeMode,
) -> anyhow::Result<HostProbeCommandOutput> {
    let mut child = Command::new(command)
        .env("AGENTICS_HOST_PROBE_MODE", mode.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            anyhow::anyhow!("failed to run host profile probe `{command}`: {error}")
        })?;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let completion = wait_host_probe_completion(&mut child, &mut stdout, &mut stderr);
    let (status, stdout, stdout_truncated, stderr, stderr_truncated) =
        match timeout(Duration::from_secs(HOST_PROBE_TIMEOUT_SECS), completion).await {
            Ok(result) => result.map_err(|error| {
                anyhow::anyhow!("failed to wait for host profile probe `{command}`: {error}")
            })?,
            Err(_) => {
                let _ignored = child.kill().await;
                let _ignored = child.wait().await;
                anyhow::bail!(
                    "host profile probe `{command}` timed out after {HOST_PROBE_TIMEOUT_SECS}s"
                );
            }
        };

    Ok(HostProbeCommandOutput {
        status,
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

async fn wait_host_probe_completion(
    child: &mut Child,
    stdout: &mut Option<ChildStdout>,
    stderr: &mut Option<ChildStderr>,
) -> Result<(ExitStatus, Vec<u8>, bool, Vec<u8>, bool), std::io::Error> {
    let (status, stdout, stderr) = tokio::join!(
        child.wait(),
        read_bounded_output(stdout),
        read_bounded_output(stderr)
    );
    let status = status?;
    let (stdout, stdout_truncated) = stdout?;
    let (stderr, stderr_truncated) = stderr?;
    Ok((status, stdout, stdout_truncated, stderr, stderr_truncated))
}

async fn read_bounded_output<R>(stream: &mut Option<R>) -> Result<(Vec<u8>, bool), std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut truncated = false;
    let Some(stream) = stream.as_mut() else {
        return Ok((output, truncated));
    };
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let slice = chunk
            .get(..read)
            .ok_or_else(|| std::io::Error::other("read exceeded buffer size"))?;
        append_bounded_bytes(&mut output, slice, &mut truncated);
    }
    Ok((output, truncated))
}

fn output_contains_failure(stdout: &[u8]) -> bool {
    String::from_utf8_lossy(stdout)
        .lines()
        .any(|line| line.contains("] FAIL "))
}

/// Run a Docker-backed GPU probe before GPU-capable workers accept jobs.
pub(crate) async fn enforce_worker_gpu_probe(
    config: &Config,
    docker: &Docker,
) -> anyhow::Result<()> {
    if config.worker_accelerators != WorkerAccelerators::Gpu {
        return Ok(());
    }
    if !cfg!(target_os = "linux") {
        anyhow::bail!("AGENTICS_WORKER_ACCELERATORS=gpu is Linux-only");
    }

    let image = config.worker_gpu_probe_image()?;
    pull_probe_image(docker, image).await?;
    let container_id = create_gpu_probe_container(docker, image).await?;
    let probe_result = run_gpu_probe_container(docker, &container_id).await;
    let cleanup_result = remove_probe_container(docker, &container_id).await;

    match (probe_result, cleanup_result) {
        (Ok(logs), Ok(())) => {
            info!(probe_output = %logs.trim(), "worker GPU probe passed");
            Ok(())
        }
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(probe_error), Ok(())) => Err(probe_error),
        (Err(probe_error), Err(cleanup_error)) => {
            anyhow::bail!(
                "{probe_error}; additionally failed to remove GPU probe container: {cleanup_error}"
            )
        }
    }
}

/// Pull the configured GPU probe image when it is not already present locally.
async fn pull_probe_image(docker: &Docker, image: &str) -> anyhow::Result<()> {
    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .platform("linux/arm64")
        .build();
    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        item.map_err(|error| anyhow::anyhow!("failed to pull GPU probe image `{image}`: {error}"))?;
    }
    Ok(())
}

/// Create a minimal GPU device-request probe container.
async fn create_gpu_probe_container(docker: &Docker, image: &str) -> anyhow::Result<String> {
    let name = format!("agentics-gpu-probe-{}", Uuid::new_v4());
    let host_config = HostConfig {
        network_mode: Some(DockerNetworkMode::None.as_str().to_string()),
        auto_remove: Some(false),
        device_requests: Some(vec![DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(1),
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        }]),
        readonly_rootfs: Some(true),
        cap_drop: Some(vec!["ALL".to_string()]),
        security_opt: Some(vec!["no-new-privileges:true".to_string()]),
        privileged: Some(false),
        publish_all_ports: Some(false),
        init: Some(true),
        oom_kill_disable: Some(false),
        log_config: Some(probe_log_config()),
        ..Default::default()
    };
    let body = ContainerCreateBody {
        image: Some(image.to_string()),
        entrypoint: Some(Vec::<String>::new()),
        cmd: Some(vec![
            "sh".to_string(),
            "-lc".to_string(),
            "nvidia-smi -L && nvidia-smi >/dev/null".to_string(),
        ]),
        host_config: Some(host_config),
        labels: Some(std::collections::HashMap::from([(
            "ai.agentics.worker.gpu_probe".to_string(),
            "true".to_string(),
        )])),
        ..Default::default()
    };
    let opts = CreateContainerOptionsBuilder::default()
        .name(&name)
        .platform("linux/arm64")
        .build();
    let response = docker
        .create_container(Some(opts), body)
        .await
        .map_err(|error| anyhow::anyhow!("failed to create GPU probe container: {error}"))?;
    Ok(response.id)
}

/// Start the GPU probe container, wait for completion, and return bounded logs.
async fn run_gpu_probe_container(docker: &Docker, container_id: &str) -> anyhow::Result<String> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await
        .map_err(|error| anyhow::anyhow!("failed to start GPU probe container: {error}"))?;

    let exit_code = match timeout(
        Duration::from_secs(GPU_PROBE_TIMEOUT_SECS),
        wait_probe_container(docker, container_id),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            anyhow::bail!("GPU probe container timed out after {GPU_PROBE_TIMEOUT_SECS}s");
        }
    };
    let logs = collect_probe_logs(docker, container_id).await?;
    if exit_code != 0 {
        anyhow::bail!("GPU probe container exited with {exit_code}\n{logs}");
    }
    Ok(logs)
}

/// Wait for the probe container to stop.
async fn wait_probe_container(docker: &Docker, container_id: &str) -> anyhow::Result<i64> {
    let opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();
    let mut stream = docker.wait_container(container_id, Some(opts));
    let mut exit_code = 1;
    while let Some(result) = stream.next().await {
        let status = result
            .map_err(|error| anyhow::anyhow!("failed to wait for GPU probe container: {error}"))?;
        exit_code = status.status_code;
    }
    Ok(exit_code)
}

/// Collect bounded stdout and stderr from the probe container.
async fn collect_probe_logs(docker: &Docker, container_id: &str) -> anyhow::Result<String> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();
    let mut logs = docker.logs(container_id, Some(opts));
    let mut output = Vec::new();
    let mut truncated = false;

    while let Some(chunk) = logs.next().await {
        match chunk {
            Ok(LogOutput::StdOut { message })
            | Ok(LogOutput::StdErr { message })
            | Ok(LogOutput::Console { message }) => {
                append_bounded_bytes(&mut output, &message, &mut truncated);
            }
            Ok(_) => {}
            Err(error) => anyhow::bail!("failed to collect GPU probe logs: {error}"),
        }
    }

    let mut text = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        text.push_str("\n[agentics] GPU probe logs truncated\n");
    }
    Ok(text)
}

/// Remove the probe container, forcing cleanup after timeouts or failures.
async fn remove_probe_container(docker: &Docker, container_id: &str) -> anyhow::Result<()> {
    let opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker
        .remove_container(container_id, Some(opts))
        .await
        .map_err(|error| anyhow::anyhow!("failed to remove GPU probe container: {error}"))
}

/// Bound probe logs before they reach startup errors or service logs.
fn append_bounded_bytes(output: &mut Vec<u8>, chunk: &[u8], truncated: &mut bool) {
    if output.len() >= MAX_PROBE_OUTPUT_BYTES {
        *truncated = !chunk.is_empty();
        return;
    }
    let remaining = MAX_PROBE_OUTPUT_BYTES.saturating_sub(output.len());
    if chunk.len() > remaining {
        output.extend(chunk.iter().take(remaining).copied());
        *truncated = true;
    } else {
        output.extend_from_slice(chunk);
    }
}

/// Docker log cap for GPU probe containers.
fn probe_log_config() -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert(
        "max-size".to_string(),
        format!("{}b", MAX_PROBE_OUTPUT_BYTES),
    );
    config.insert("max-file".to_string(), "1".to_string());
    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

/// Convert a probe failure into either a startup error or a warning.
fn handle_probe_failure(mode: HostProbeMode, message: String) -> anyhow::Result<()> {
    match mode {
        HostProbeMode::Off => Ok(()),
        HostProbeMode::Warn => {
            warn!("{message}");
            Ok(())
        }
        HostProbeMode::Require => anyhow::bail!("{message}"),
    }
}

/// Format bounded probe output for worker logs and startup errors.
fn format_probe_failure(
    status: Option<ExitStatus>,
    stdout: &[u8],
    stderr: &[u8],
    truncated: bool,
) -> String {
    let status = status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "unknown status".to_string());
    let stdout = bounded_utf8(stdout);
    let stderr = bounded_utf8(stderr);
    let truncation = if truncated {
        "\n[agentics] host profile probe output truncated"
    } else {
        ""
    };
    format!(
        "host profile probe failed with {status}\nstdout:\n{stdout}\nstderr:\n{stderr}{truncation}"
    )
}

/// Convert command output to bounded UTF-8 text.
fn bounded_utf8(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if text.len() > MAX_PROBE_OUTPUT_BYTES {
        let mut boundary = MAX_PROBE_OUTPUT_BYTES.min(text.len());
        while !text.is_char_boundary(boundary) {
            boundary = boundary.saturating_sub(1);
        }
        text.truncate(boundary);
        text.push_str("\n[agentics] host profile probe output truncated\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use agentics_config::HostProbeMode;

    use super::{
        append_bounded_bytes, bounded_utf8, handle_probe_failure, output_contains_failure,
    };

    /// Verifies require mode fails closed when the hosted probe fails.
    #[test]
    fn require_mode_fails_on_probe_failure() {
        let error = handle_probe_failure(HostProbeMode::Require, "probe failed".to_string())
            .expect_err("require mode must fail worker startup");

        assert!(error.to_string().contains("probe failed"));
    }

    /// Verifies warn mode logs and continues when the hosted probe fails.
    #[test]
    fn warn_mode_allows_probe_failure() {
        assert!(handle_probe_failure(HostProbeMode::Warn, "probe failed".to_string()).is_ok());
    }

    /// Verifies probe output is bounded before it reaches startup errors or logs.
    #[test]
    fn probe_output_is_bounded() {
        let text = bounded_utf8(&vec![b'x'; 9000]);

        assert!(text.len() < 9000);
        assert!(text.contains("truncated"));
    }

    /// Verifies GPU probe log collection is bounded by bytes.
    #[test]
    fn gpu_probe_log_append_truncates_by_byte_limit() {
        let mut output = Vec::new();
        let mut truncated = false;

        append_bounded_bytes(
            &mut output,
            &vec![b'x'; super::MAX_PROBE_OUTPUT_BYTES + 1],
            &mut truncated,
        );

        assert_eq!(output.len(), super::MAX_PROBE_OUTPUT_BYTES);
        assert!(truncated);
    }

    /// Verifies warn-mode checker output that exits zero still surfaces failures.
    #[test]
    fn host_probe_output_failure_lines_are_detected() {
        assert!(output_contains_failure(
            b"[agentics-dgx-check] FAIL mutating probes - missing\n"
        ));
        assert!(!output_contains_failure(
            b"[agentics-dgx-check] PASS runner profile modes - ok\n"
        ));
    }
}
