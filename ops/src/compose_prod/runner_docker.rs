//! Dedicated Docker daemon management for production runner containers.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use nix::unistd::{Gid, Uid, chown};
use tokio::process::Command;

use crate::support::{
    DEFAULT_DOCKER_SOCKET_PATH, DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, print_reports,
};

use super::bridge_egress::ensure_bridge_egress;
use super::{ComposeContext, ComposeProdError, PREFIX};

const DEFAULT_RUNNER_DOCKER_SOCKET_PATH: &str = "/srv/agentics/docker.sock";
const DEFAULT_RUNNER_DOCKER_DATA_ROOT: &str = "/srv/agentics/docker-data-root";
const DEFAULT_RUNNER_DOCKER_EXEC_ROOT: &str = "/srv/agentics/docker-exec";
const DEFAULT_RUNNER_DOCKER_PIDFILE: &str = "/srv/agentics/docker.pid";
const DEFAULT_RUNNER_DOCKER_LOG: &str = "/srv/agentics/dockerd.log";
const DEFAULT_RUNNER_DOCKER_BRIDGE: &str = "agentics0";
const DEFAULT_RUNNER_DOCKER_BRIDGE_CIDR: &str = "172.30.0.1/16";
pub(super) async fn runner_docker_up(
    context: &ComposeContext,
    dry_run: bool,
) -> Result<ExitCode, ComposeProdError> {
    let config = RunnerDockerConfig::from_context(context)?;
    config.validate_dedicated_socket()?;
    if dry_run {
        return Ok(print_reports(PREFIX, &config.dry_run_start_reports()));
    }
    require_root_for_runner_docker()?;

    if runner_docker_ready(&config).await? {
        let bridge_report = runner_docker_bridge_report(&config).await?;
        let mut reports = vec![
            ReportLine::pass(
                "runner Docker daemon",
                format!("already reachable at {}", config.docker_host),
            ),
            bridge_report,
        ];
        reports.extend(ensure_runner_docker_bridge_egress(context).await?);
        return Ok(print_reports(PREFIX, &reports));
    }

    if !config.data_root.is_dir() {
        return Err(ComposeProdError::InvalidConfig(format!(
            "prepared Docker data root is required at {}; run agentics-prepare-dgx-spark-storage first",
            config.data_root.display()
        )));
    }

    create_runner_docker_dirs(&config)?;
    remove_stale_runner_docker_files(&config)?;
    ensure_runner_docker_bridge(&config).await?;
    spawn_runner_dockerd(&config)?;
    wait_for_runner_docker(&config).await?;
    repair_runner_docker_socket_permissions(&config)?;
    let bridge_report = runner_docker_bridge_report(&config).await?;
    let mut reports = vec![
        ReportLine::pass(
            "runner Docker daemon",
            format!(
                "ready at {} using bridge {} ({})",
                config.docker_host, config.bridge_name, config.bridge_cidr
            ),
        ),
        bridge_report,
    ];
    reports.extend(ensure_runner_docker_bridge_egress(context).await?);
    Ok(print_reports(PREFIX, &reports))
}

pub(super) async fn ensure_runner_docker_bridge_egress(
    context: &ComposeContext,
) -> Result<Vec<ReportLine>, ComposeProdError> {
    let config = RunnerDockerConfig::from_context(context)?;
    config.validate_dedicated_socket()?;
    ensure_bridge_egress(
        context,
        "runner Docker bridge egress",
        &format!("runner Docker bridge {}", config.bridge_name),
        &config.bridge_name,
    )
    .await
}

pub(super) async fn check_runner_docker_pypi_egress(
    context: &ComposeContext,
) -> Result<Vec<ReportLine>, ComposeProdError> {
    let config = RunnerDockerConfig::from_context(context)?;
    config.validate_dedicated_socket()?;
    let Some(image) = context.string_value(|env| env.worker_gpu_probe_image.as_ref(), "") else {
        return Ok(vec![ReportLine::skip(
            "runner Docker PyPI egress",
            "AGENTICS_WORKER_GPU_PROBE_IMAGE is not configured",
        )]);
    };

    let output = docker_with_host_output(
        &config.docker_host,
        [
            "run",
            "--rm",
            "--network",
            "bridge",
            "--pull",
            "never",
            image.as_str(),
            "python3",
            "-c",
            r#"import socket, ssl
host = "pypi.org"
raw = socket.create_connection((host, 443), timeout=10)
with ssl.create_default_context().wrap_socket(raw, server_hostname=host) as sock:
    sock.version()
print("ok")
"#,
        ],
        Duration::from_secs(30),
    )
    .await?;
    if output.success() {
        return Ok(vec![ReportLine::pass(
            "runner Docker PyPI egress",
            "runner container can open TLS to pypi.org:443",
        )]);
    }
    Ok(vec![ReportLine::fail(
        "runner Docker PyPI egress",
        format!(
            "runner container cannot open TLS to pypi.org:443 through bridge {}: {}",
            config.bridge_name,
            output.combined()
        ),
    )])
}

pub(super) async fn runner_docker_down(
    context: &ComposeContext,
    dry_run: bool,
) -> Result<ExitCode, ComposeProdError> {
    let config = RunnerDockerConfig::from_context(context)?;
    config.validate_dedicated_socket()?;
    if dry_run {
        return Ok(print_reports(
            PREFIX,
            &[ReportLine::pass(
                "runner Docker daemon",
                format!(
                    "would stop pid from {} and remove {}",
                    config.pidfile.display(),
                    config.socket_path.display()
                ),
            )],
        ));
    }
    require_root_for_runner_docker()?;
    let Some(pid) = read_runner_docker_pid(&config)? else {
        let _ignored = remove_stale_runner_docker_path(&config.socket_path, "socket");
        return Ok(print_reports(
            PREFIX,
            &[ReportLine::pass(
                "runner Docker daemon",
                format!("no pidfile at {}", config.pidfile.display()),
            )],
        ));
    };

    run_process_output("kill", [pid.to_string()], Duration::from_secs(10)).await?;
    for _ in 0..20 {
        if !process_exists(pid).await? {
            let _ignored = remove_stale_runner_docker_path(&config.pidfile, "pidfile");
            let _ignored = remove_stale_runner_docker_path(&config.socket_path, "socket");
            return Ok(print_reports(
                PREFIX,
                &[ReportLine::pass(
                    "runner Docker daemon",
                    format!("stopped pid {pid}"),
                )],
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(ComposeProdError::Process(format!(
        "runner Docker daemon pid {pid} did not stop; inspect {} before retrying",
        config.pidfile.display()
    )))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerDockerConfig {
    socket_path: PathBuf,
    docker_host: String,
    data_root: PathBuf,
    exec_root: PathBuf,
    pidfile: PathBuf,
    logfile: PathBuf,
    bridge_name: String,
    bridge_cidr: String,
    containerd_namespace: String,
    containerd_plugins_namespace: String,
    socket_gid: Option<u32>,
}

impl RunnerDockerConfig {
    fn from_context(context: &ComposeContext) -> Result<Self, ComposeProdError> {
        let socket_path = context
            .docker_socket_path()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_RUNNER_DOCKER_SOCKET_PATH));
        let docker_host = context
            .docker_host()
            .unwrap_or_else(|| format!("unix://{}", socket_path.display()));
        if docker_host != format!("unix://{}", socket_path.display()) {
            return Err(ComposeProdError::InvalidConfig(format!(
                "AGENTICS_DOCKER_HOST must match AGENTICS_DOCKER_SOCKET_PATH for runner Docker management; got {docker_host} and {}",
                socket_path.display()
            )));
        }
        let containerd_namespace = context.resolve_namespace(None)?.as_str().to_string();
        let containerd_plugins_namespace = format!("{containerd_namespace}-plugins");
        Ok(Self {
            socket_path,
            docker_host,
            data_root: context.path_value(
                |env| env.dgx_docker_data_root.as_ref(),
                DEFAULT_RUNNER_DOCKER_DATA_ROOT,
            ),
            exec_root: context.path_value(
                |env| env.dgx_runner_docker_exec_root.as_ref(),
                DEFAULT_RUNNER_DOCKER_EXEC_ROOT,
            ),
            pidfile: context.path_value(
                |env| env.dgx_runner_docker_pidfile.as_ref(),
                DEFAULT_RUNNER_DOCKER_PIDFILE,
            ),
            logfile: context.path_value(
                |env| env.dgx_runner_docker_log.as_ref(),
                DEFAULT_RUNNER_DOCKER_LOG,
            ),
            bridge_name: context
                .string_value(
                    |env| env.dgx_runner_docker_bridge.as_ref(),
                    DEFAULT_RUNNER_DOCKER_BRIDGE,
                )
                .ok_or_else(|| {
                    ComposeProdError::InvalidConfig(
                        "runner Docker bridge name cannot be empty".to_string(),
                    )
                })?,
            bridge_cidr: context
                .string_value(
                    |env| env.dgx_runner_docker_bridge_cidr.as_ref(),
                    DEFAULT_RUNNER_DOCKER_BRIDGE_CIDR,
                )
                .ok_or_else(|| {
                    ComposeProdError::InvalidConfig(
                        "runner Docker bridge CIDR cannot be empty".to_string(),
                    )
                })?,
            containerd_namespace,
            containerd_plugins_namespace,
            socket_gid: context
                .process_env
                .docker_socket_gid
                .or(context.file_env.docker_socket_gid),
        })
    }

    fn dry_run_start_reports(&self) -> Vec<ReportLine> {
        vec![ReportLine::pass(
            "runner Docker daemon",
            format!(
                "would start dockerd at {} with data root {}, exec root {}, bridge {} ({})",
                self.docker_host,
                self.data_root.display(),
                self.exec_root.display(),
                self.bridge_name,
                self.bridge_cidr
            ),
        )]
    }

    fn validate_dedicated_socket(&self) -> Result<(), ComposeProdError> {
        if self.socket_path == Path::new(DEFAULT_DOCKER_SOCKET_PATH) {
            return Err(ComposeProdError::InvalidConfig(format!(
                "refusing to manage the system Docker socket {}; set AGENTICS_DOCKER_SOCKET_PATH to a dedicated runner socket such as {DEFAULT_RUNNER_DOCKER_SOCKET_PATH}",
                DEFAULT_DOCKER_SOCKET_PATH
            )));
        }
        Ok(())
    }
}

fn require_root_for_runner_docker() -> Result<(), ComposeProdError> {
    if Uid::effective().is_root() {
        Ok(())
    } else {
        Err(ComposeProdError::InvalidConfig(
            "runner Docker daemon management requires root; run with sudo".to_string(),
        ))
    }
}

fn create_runner_docker_dirs(config: &RunnerDockerConfig) -> Result<(), ComposeProdError> {
    let socket_parent = config.socket_path.parent().ok_or_else(|| {
        ComposeProdError::InvalidConfig(format!(
            "socket path {} has no parent directory",
            config.socket_path.display()
        ))
    })?;
    let pid_parent = config.pidfile.parent().ok_or_else(|| {
        ComposeProdError::InvalidConfig(format!(
            "pidfile path {} has no parent directory",
            config.pidfile.display()
        ))
    })?;
    let log_parent = config.logfile.parent().ok_or_else(|| {
        ComposeProdError::InvalidConfig(format!(
            "log path {} has no parent directory",
            config.logfile.display()
        ))
    })?;
    fs::create_dir_all(socket_parent)
        .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    fs::create_dir_all(&config.exec_root)
        .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    fs::create_dir_all(pid_parent).map_err(|error| ComposeProdError::Process(error.to_string()))?;
    fs::create_dir_all(log_parent).map_err(|error| ComposeProdError::Process(error.to_string()))?;
    Ok(())
}

fn remove_stale_runner_docker_files(config: &RunnerDockerConfig) -> Result<(), ComposeProdError> {
    remove_stale_runner_docker_path(&config.socket_path, "socket")?;
    remove_stale_runner_docker_path(&config.pidfile, "pidfile")?;
    Ok(())
}

fn remove_stale_runner_docker_path(path: &Path, label: &str) -> Result<(), ComposeProdError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ComposeProdError::Process(format!(
                "failed to inspect stale runner Docker {label} {}: {error}",
                path.display()
            )));
        }
    };
    if metadata.file_type().is_dir() {
        return fs::remove_dir(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::DirectoryNotEmpty {
                ComposeProdError::InvalidConfig(format!(
                    "stale runner Docker {label} path {} is a non-empty directory; remove it manually before starting the runner daemon",
                    path.display()
                ))
            } else {
                ComposeProdError::Process(format!(
                    "failed to remove stale runner Docker {label} directory {}: {error}",
                    path.display()
                ))
            }
        });
    }
    fs::remove_file(path).map_err(|error| {
        ComposeProdError::Process(format!(
            "failed to remove stale runner Docker {label} {}: {error}",
            path.display()
        ))
    })
}

fn spawn_runner_dockerd(config: &RunnerDockerConfig) -> Result<(), ComposeProdError> {
    let log = File::options()
        .create(true)
        .append(true)
        .open(&config.logfile)
        .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    let log_for_stderr = log
        .try_clone()
        .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    Command::new("dockerd")
        .arg("--data-root")
        .arg(&config.data_root)
        .arg("--exec-root")
        .arg(&config.exec_root)
        .arg("--host")
        .arg(&config.docker_host)
        .arg("--pidfile")
        .arg(&config.pidfile)
        .arg("--storage-driver")
        .arg("overlay2")
        .arg("--bridge")
        .arg(&config.bridge_name)
        .arg("--iptables=true")
        .arg("--ip-forward=true")
        .arg("--ip-masq=true")
        .arg("--live-restore=false")
        .arg("--log-driver")
        .arg("json-file")
        .arg("--log-opt")
        .arg("max-file=3")
        .arg("--log-opt")
        .arg("max-size=10m")
        .arg("--containerd-namespace")
        .arg(&config.containerd_namespace)
        .arg("--containerd-plugins-namespace")
        .arg(&config.containerd_plugins_namespace)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_for_stderr))
        .spawn()
        .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    Ok(())
}

async fn wait_for_runner_docker(config: &RunnerDockerConfig) -> Result<(), ComposeProdError> {
    for _ in 0..30 {
        if runner_docker_ready(config).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(ComposeProdError::Process(format!(
        "runner Docker daemon did not become ready at {}; inspect {}",
        config.docker_host,
        config.logfile.display()
    )))
}

async fn ensure_runner_docker_bridge(config: &RunnerDockerConfig) -> Result<(), ComposeProdError> {
    let link = run_process_output(
        "ip",
        ["link", "show", "dev", &config.bridge_name],
        Duration::from_secs(10),
    )
    .await?;
    if !link.success() {
        let add = run_process_output(
            "ip",
            ["link", "add", "name", &config.bridge_name, "type", "bridge"],
            Duration::from_secs(10),
        )
        .await?;
        if !add.success() {
            return Err(ComposeProdError::Process(format!(
                "failed to create runner Docker bridge {}: {}",
                config.bridge_name,
                add.combined()
            )));
        }
    }

    let addr = run_process_output(
        "ip",
        ["addr", "show", "dev", &config.bridge_name],
        Duration::from_secs(10),
    )
    .await?;
    if !addr.success() {
        return Err(ComposeProdError::Process(format!(
            "failed to inspect runner Docker bridge {}: {}",
            config.bridge_name,
            addr.combined()
        )));
    }
    if !addr.stdout.contains(&config.bridge_cidr) {
        let add_addr = run_process_output(
            "ip",
            [
                "addr",
                "add",
                &config.bridge_cidr,
                "dev",
                &config.bridge_name,
            ],
            Duration::from_secs(10),
        )
        .await?;
        if !add_addr.success() {
            return Err(ComposeProdError::Process(format!(
                "failed to assign {} to runner Docker bridge {}: {}",
                config.bridge_cidr,
                config.bridge_name,
                add_addr.combined()
            )));
        }
    }

    let up = run_process_output(
        "ip",
        ["link", "set", "dev", &config.bridge_name, "up"],
        Duration::from_secs(10),
    )
    .await?;
    if !up.success() {
        return Err(ComposeProdError::Process(format!(
            "failed to bring runner Docker bridge {} up: {}",
            config.bridge_name,
            up.combined()
        )));
    }
    Ok(())
}

async fn runner_docker_ready(config: &RunnerDockerConfig) -> Result<bool, ComposeProdError> {
    let output =
        docker_with_host_output(&config.docker_host, ["info"], Duration::from_secs(10)).await?;
    Ok(output.success())
}

async fn runner_docker_bridge_report(
    config: &RunnerDockerConfig,
) -> Result<ReportLine, ComposeProdError> {
    let output = docker_with_host_output(
        &config.docker_host,
        ["network", "inspect", "bridge", "--format", "{{.Name}}"],
        Duration::from_secs(10),
    )
    .await?;
    if output.success() && output.stdout.trim() == "bridge" {
        return Ok(ReportLine::pass(
            "runner Docker bridge",
            format!(
                "default bridge network is backed by host bridge {}",
                config.bridge_name
            ),
        ));
    }
    Ok(ReportLine::fail(
        "runner Docker bridge",
        format!(
            "default bridge network is unavailable; restart the runner daemon with `just prod::runner-docker-down` then `just prod::runner-docker-up`: {}",
            output.combined()
        ),
    ))
}

fn repair_runner_docker_socket_permissions(
    config: &RunnerDockerConfig,
) -> Result<(), ComposeProdError> {
    let Some(gid) = config.socket_gid else {
        return Ok(());
    };
    chown(
        &config.socket_path,
        Some(Uid::from_raw(0)),
        Some(Gid::from_raw(gid)),
    )
    .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    Ok(())
}

fn read_runner_docker_pid(config: &RunnerDockerConfig) -> Result<Option<u32>, ComposeProdError> {
    let text = match fs::read_to_string(&config.pidfile) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ComposeProdError::Process(error.to_string())),
    };
    let pid = text
        .trim()
        .parse::<u32>()
        .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))?;
    Ok(Some(pid))
}

async fn process_exists(pid: u32) -> Result<bool, ComposeProdError> {
    let output = run_process_output(
        "kill",
        ["-0".to_string(), pid.to_string()],
        Duration::from_secs(5),
    )
    .await?;
    Ok(output.success())
}

async fn docker_with_host_output<I, S>(
    host: &str,
    args: I,
    timeout: Duration,
) -> Result<crate::support::CommandOutput, ComposeProdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut full_args = vec![OsString::from("--host"), OsString::from(host)];
    full_args.extend(
        args.into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>(),
    );
    run_process_output("docker", full_args, timeout).await
}

async fn run_process_output<I, S>(
    program: &str,
    args: I,
    timeout: Duration,
) -> Result<crate::support::CommandOutput, ComposeProdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    crate::support::run_process(program, args, Some(timeout), DEFAULT_OUTPUT_LIMIT_BYTES)
        .await
        .map_err(ComposeProdError::from)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::remove_stale_runner_docker_path;
    use crate::compose_prod::ComposeProdError;

    #[test]
    fn remove_stale_runner_docker_path_removes_empty_directory() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let stale_socket = tempdir.path().join("docker.sock");
        fs::create_dir(&stale_socket).expect("stale socket dir");

        remove_stale_runner_docker_path(&stale_socket, "socket").expect("remove stale socket dir");

        assert!(!stale_socket.exists());
    }

    #[test]
    fn remove_stale_runner_docker_path_rejects_non_empty_directory() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let stale_socket = tempdir.path().join("docker.sock");
        fs::create_dir(&stale_socket).expect("stale socket dir");
        fs::write(stale_socket.join("child"), b"not empty").expect("stale child");

        let error = remove_stale_runner_docker_path(&stale_socket, "socket")
            .expect_err("non-empty directory should fail");

        assert!(matches!(error, ComposeProdError::InvalidConfig(_)));
    }
}
