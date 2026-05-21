//! Rust-native DGX Spark host inventory checker.
//!
//! This executable oxidizes `scripts/ops/check-dgx-spark-host.sh` while keeping
//! the shell script as an imperfect reference. It performs Linux-gated host,
//! storage, Docker, and NVIDIA inventory checks. Native filesystem/proc
//! inspection and Bollard Docker APIs are used where practical; NVIDIA toolkit
//! commands remain explicit process boundaries because they are host tools.
//!
//! Cancellation: process execution and Docker probes are raced against Ctrl-C
//! in `run_from_process`. The command is read-only unless the optional NVIDIA
//! Docker smoke is requested, and even that creates only a `--rm`-equivalent
//! probe container. There is no rollback or dry-run behavior because this
//! command is an inventory checker, not a mutator.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::Duration;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{ContainerCreateBody, DeviceRequest, HostConfig, HostConfigLogConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
    StartContainerOptions, WaitContainerOptionsBuilder,
};
use clap::Parser;
use futures::StreamExt;
use shared::zip_project::DockerNetworkMode;
use uuid::Uuid;

use crate::dgx::{
    DEFAULT_CUDA_IMAGE, DockerPullPolicy, ENV_DGX_CUDA_IMAGE, ENV_DGX_DOCKER_PULL_POLICY,
    ENV_DGX_RUN_DOCKER_SMOKE,
};
use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, append_bounded_bytes, env_non_empty,
    parse_bool_env, print_reports, run_process, run_with_ctrl_c,
};

const PREFIX: &str = "agentics-dgx-host";
const DOCKER_SMOKE_TIMEOUT_SECS: u64 = 120;

/// CLI for DGX Spark host inventory checks.
#[derive(Debug, Parser)]
#[command(
    about = "Collects DGX Spark host inventory and optional NVIDIA Docker smoke evidence.",
    long_about = "Collects Linux host, storage, Docker, and NVIDIA inventory for DGX Spark operations. Native filesystem/proc inspection and Bollard are used where practical. NVIDIA host tools are invoked directly as bounded process boundaries."
)]
pub struct Cli {
    /// Run the NVIDIA Docker smoke container. Falls back to AGENTICS_DGX_RUN_DOCKER_SMOKE.
    #[arg(long)]
    run_docker_smoke: bool,
    /// CUDA image used for the optional smoke container.
    #[arg(long)]
    cuda_image: Option<String>,
    /// Docker pull policy for smoke image: never, missing, or always.
    #[arg(long)]
    docker_pull_policy: Option<DockerPullPolicyArg>,
    /// Timeout for bounded host tool commands.
    #[arg(long, default_value_t = 30)]
    command_timeout_seconds: u64,
}

/// Clap adapter for typed Docker pull policy.
#[derive(Debug, Clone, Copy)]
pub struct DockerPullPolicyArg(DockerPullPolicy);

impl std::str::FromStr for DockerPullPolicyArg {
    type Err = crate::dgx::DgxConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> Result<Vec<ReportLine>, HostCheckError> {
    if !cfg!(target_os = "linux") {
        return Ok(vec![ReportLine::fail(
            "Linux gate",
            format!(
                "DGX Spark inventory checks are Linux-only; detected {}",
                std::env::consts::OS
            ),
        )]);
    }

    let timeout = Duration::from_secs(cli.command_timeout_seconds.max(1));
    let run_smoke = cli.run_docker_smoke || parse_bool_env(ENV_DGX_RUN_DOCKER_SMOKE, false)?;
    let cuda_image = cli
        .cuda_image
        .or_else(|| env_non_empty(ENV_DGX_CUDA_IMAGE))
        .unwrap_or_else(|| DEFAULT_CUDA_IMAGE.to_string());
    let pull_policy = match cli.docker_pull_policy {
        Some(policy) => policy.0,
        None => env_non_empty(ENV_DGX_DOCKER_PULL_POLICY)
            .as_deref()
            .unwrap_or(DockerPullPolicy::Never.as_str())
            .parse()?,
    };

    let mut reports = Vec::new();
    reports.extend(host_reports(timeout).await);
    reports.extend(storage_reports(timeout).await);
    reports.extend(xfs_tool_reports(timeout).await);
    reports.extend(docker_reports(timeout).await);
    reports.extend(nvidia_reports(timeout).await);
    reports.push(match run_smoke {
        true => docker_smoke_report(&cuda_image, pull_policy).await,
        false => ReportLine::skip(
            "NVIDIA Docker smoke",
            "set AGENTICS_DGX_RUN_DOCKER_SMOKE=1 or --run-docker-smoke to run",
        ),
    });
    Ok(reports)
}

async fn host_reports(timeout: Duration) -> Vec<ReportLine> {
    let mut reports = Vec::new();
    reports.push(process_line("uname -a", "uname", ["-a"], timeout).await);
    reports.push(ReportLine::pass("architecture", std::env::consts::ARCH));
    match tokio::fs::read_to_string("/etc/os-release").await {
        Ok(text) => reports.push(ReportLine::pass("os-release", first_lines(&text, 4))),
        Err(error) => reports.push(ReportLine::fail("os-release", error.to_string())),
    }
    reports
}

async fn storage_reports(timeout: Duration) -> Vec<ReportLine> {
    let mut reports = Vec::new();
    for path in ["/", "/home", "/var/lib/docker"] {
        let report = run_process(
            "findmnt",
            ["-no", "SOURCE,TARGET,FSTYPE,OPTIONS", path],
            Some(timeout),
            DEFAULT_OUTPUT_LIMIT_BYTES,
        )
        .await;
        reports.push(command_report(&format!("mount {path}"), report));
    }
    reports.push(
        process_line(
            "XFS mounts",
            "findmnt",
            ["-t", "xfs", "-o", "SOURCE,TARGET,FSTYPE,OPTIONS"],
            timeout,
        )
        .await,
    );
    reports.push(
        process_line(
            "block devices",
            "lsblk",
            ["-o", "NAME,TYPE,SIZE,FSTYPE,MOUNTPOINTS"],
            timeout,
        )
        .await,
    );
    reports
}

async fn xfs_tool_reports(_timeout: Duration) -> Vec<ReportLine> {
    let mut reports = Vec::new();
    let filesystems = tokio::fs::read_to_string("/proc/filesystems")
        .await
        .unwrap_or_default();
    if filesystems
        .lines()
        .any(|line| line.split_whitespace().last() == Some("xfs"))
    {
        reports.push(ReportLine::pass(
            "kernel XFS support",
            "xfs listed in /proc/filesystems",
        ));
    } else {
        reports.push(ReportLine::fail(
            "kernel XFS support",
            "xfs missing from /proc/filesystems",
        ));
    }
    for tool in ["mkfs.xfs", "xfs_quota", "xfs_info", "losetup", "truncate"] {
        if tool_exists(tool) {
            reports.push(ReportLine::pass(format!("tool {tool}"), "found in PATH"));
        } else {
            reports.push(ReportLine::fail(
                format!("tool {tool}"),
                "missing from PATH",
            ));
        }
    }
    reports
}

async fn docker_reports(timeout: Duration) -> Vec<ReportLine> {
    let _timeout = timeout;
    match Docker::connect_with_defaults() {
        Ok(docker) => match docker.info().await {
            Ok(info) => {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "driver",
                    info.driver.unwrap_or_else(|| "<unknown>".to_string()),
                );
                fields.insert(
                    "os",
                    info.operating_system
                        .unwrap_or_else(|| "<unknown>".to_string()),
                );
                fields.insert(
                    "arch",
                    info.architecture.unwrap_or_else(|| "<unknown>".to_string()),
                );
                vec![ReportLine::pass(
                    "Docker daemon",
                    fields
                        .into_iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                )]
            }
            Err(error) => vec![ReportLine::fail("Docker daemon", error.to_string())],
        },
        Err(error) => vec![ReportLine::fail("Docker daemon", error.to_string())],
    }
}

async fn nvidia_reports(timeout: Duration) -> Vec<ReportLine> {
    vec![
        process_line("nvidia-smi", "nvidia-smi", [], timeout).await,
        process_line(
            "nvidia-container-cli",
            "nvidia-container-cli",
            ["--version"],
            timeout,
        )
        .await,
        process_line(
            "nvidia-ctk dry-run",
            "nvidia-ctk",
            ["runtime", "configure", "--runtime=docker", "--dry-run"],
            timeout,
        )
        .await,
    ]
}

async fn docker_smoke_report(image: &str, pull_policy: DockerPullPolicy) -> ReportLine {
    match run_docker_smoke(image, pull_policy).await {
        Ok(logs) => ReportLine::pass("NVIDIA Docker smoke", logs.trim()),
        Err(error) => ReportLine::fail("NVIDIA Docker smoke", error.to_string()),
    }
}

async fn run_docker_smoke(
    image: &str,
    pull_policy: DockerPullPolicy,
) -> Result<String, HostCheckError> {
    let docker = Docker::connect_with_defaults()?;
    if pull_policy == DockerPullPolicy::Always
        || (pull_policy == DockerPullPolicy::Missing && docker.inspect_image(image).await.is_err())
    {
        use bollard::query_parameters::CreateImageOptionsBuilder;
        let opts = CreateImageOptionsBuilder::default()
            .from_image(image)
            .build();
        let mut stream = docker.create_image(Some(opts), None, None);
        while let Some(item) = stream.next().await {
            item?;
        }
    }

    let name = format!("agentics-dgx-host-smoke-{}", Uuid::new_v4());
    let body = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["nvidia-smi".to_string()]),
        host_config: Some(HostConfig {
            network_mode: Some(DockerNetworkMode::None.as_str().to_string()),
            auto_remove: Some(false),
            log_config: Some(smoke_log_config()),
            device_requests: Some(vec![DeviceRequest {
                driver: Some("nvidia".to_string()),
                count: Some(-1),
                capabilities: Some(vec![vec!["gpu".to_string()]]),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    };
    let opts = CreateContainerOptionsBuilder::default().name(&name).build();
    let response = docker.create_container(Some(opts), body).await?;
    let container_id = response.id;
    let result = async {
        docker
            .start_container(&container_id, None::<StartContainerOptions>)
            .await?;
        let mut wait = docker.wait_container(
            &container_id,
            Some(
                WaitContainerOptionsBuilder::default()
                    .condition("not-running")
                    .build(),
            ),
        );
        let status = tokio::time::timeout(Duration::from_secs(DOCKER_SMOKE_TIMEOUT_SECS), async {
            let mut code = 1;
            while let Some(item) = wait.next().await {
                code = item?.status_code;
            }
            Ok::<i64, bollard::errors::Error>(code)
        })
        .await
        .map_err(|_| HostCheckError::Timeout(DOCKER_SMOKE_TIMEOUT_SECS))??;

        let logs = collect_container_logs(&docker, &container_id).await?;
        Ok::<(i64, String), HostCheckError>((status, logs))
    }
    .await;
    let cleanup = docker
        .remove_container(
            &container_id,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    match (result, cleanup) {
        (Ok((0, logs)), Ok(())) => Ok(logs),
        (Ok((status, logs)), Ok(())) => Err(HostCheckError::DockerSmoke(format!(
            "container exited with {status}: {}",
            logs.trim()
        ))),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(HostCheckError::Docker(error)),
        (Err(error), Err(cleanup_error)) => Err(HostCheckError::DockerSmoke(format!(
            "{error}; additionally failed to remove Docker smoke container: {cleanup_error}"
        ))),
    }
}

async fn collect_container_logs(
    docker: &Docker,
    id: &str,
) -> Result<String, bollard::errors::Error> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();
    let mut stream = docker.logs(id, Some(opts));
    let mut bytes = Vec::new();
    let mut truncated = false;
    while let Some(item) = stream.next().await {
        match item? {
            LogOutput::StdOut { message }
            | LogOutput::StdErr { message }
            | LogOutput::Console { message } => append_bounded_bytes(
                &mut bytes,
                &message,
                DEFAULT_OUTPUT_LIMIT_BYTES,
                &mut truncated,
            ),
            _ => {}
        }
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text.push_str("\n[agentics] Docker smoke logs truncated\n");
    }
    Ok(text)
}

fn smoke_log_config() -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert(
        "max-size".to_string(),
        format!("{}b", DEFAULT_OUTPUT_LIMIT_BYTES),
    );
    config.insert("max-file".to_string(), "1".to_string());
    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

async fn process_line<const N: usize>(
    label: &str,
    program: &str,
    args: [&str; N],
    timeout: Duration,
) -> ReportLine {
    command_report(
        label,
        run_process(program, args, Some(timeout), DEFAULT_OUTPUT_LIMIT_BYTES).await,
    )
}

fn command_report(
    label: &str,
    result: Result<crate::support::CommandOutput, SupportError>,
) -> ReportLine {
    match result {
        Ok(output) if output.success() => {
            let message = output.combined();
            ReportLine::pass(
                label,
                if message.is_empty() {
                    "ok".to_string()
                } else {
                    message
                },
            )
        }
        Ok(output) => ReportLine::fail(
            label,
            format!("exit={:?} {}", output.status, output.combined()),
        ),
        Err(error) => ReportLine::fail(label, error.to_string()),
    }
}

fn first_lines(text: &str, max: usize) -> String {
    text.lines().take(max).collect::<Vec<_>>().join(" | ")
}

fn tool_exists(tool: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(tool).is_file())
}

/// Host inventory checker errors.
#[derive(Debug, thiserror::Error)]
enum HostCheckError {
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error(transparent)]
    Config(#[from] crate::dgx::DgxConfigError),
    #[error(transparent)]
    Docker(#[from] bollard::errors::Error),
    #[error("Docker smoke timed out after {0}s")]
    Timeout(u64),
    #[error("{0}")]
    DockerSmoke(String),
}

#[cfg(test)]
mod tests {
    use super::first_lines;

    /// Verifies inventory output summary is bounded by line count.
    #[test]
    fn first_lines_limits_output() {
        assert_eq!(first_lines("a\nb\nc", 2), "a | b");
    }
}
