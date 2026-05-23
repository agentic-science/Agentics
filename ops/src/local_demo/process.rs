use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use super::{LocalDemoConfig, LocalDemoError};
use crate::support::ReportLine;

#[derive(Debug, Clone, Copy)]
pub(super) enum DemoProcess {
    Api,
    Web,
}

impl DemoProcess {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Web => "web",
        }
    }

    pub(super) fn pid_path(self, config: &LocalDemoConfig) -> PathBuf {
        config.pid_dir.join(format!("{}.pid", self.as_str()))
    }

    pub(super) fn log_path(self, config: &LocalDemoConfig) -> PathBuf {
        config.log_dir.join(format!("{}.log", self.as_str()))
    }

    pub(super) fn port(self, config: &LocalDemoConfig) -> u16 {
        match self {
            Self::Api => config.api_port,
            Self::Web => config.web_port,
        }
    }
}

pub(super) async fn start_named_process(
    config: &LocalDemoConfig,
    process: DemoProcess,
) -> Result<ReportLine, LocalDemoError> {
    let pid_path = process.pid_path(config);
    if let Some(pid) = running_pid_from_file(&pid_path)? {
        return Ok(ReportLine::pass(
            process.as_str(),
            format!("already running with pid {pid}"),
        ));
    }
    prepare_process_dirs(config).await?;
    let log_path = process.log_path(config);
    let (log, log_err) = open_process_logs(&log_path)?;
    let pid = spawn_demo_process(config, process, log, log_err)?;
    tokio::fs::write(&pid_path, pid.to_string()).await?;
    Ok(ReportLine::pass(
        process.as_str(),
        format!("started pid {pid}; log {}", log_path.display()),
    ))
}

pub(super) async fn stop_named_process(
    config: &LocalDemoConfig,
    process: DemoProcess,
) -> Result<ReportLine, LocalDemoError> {
    let pid_path = process.pid_path(config);
    let Ok(pid) = read_pid(&pid_path) else {
        let _ignored = tokio::fs::remove_file(&pid_path).await;
        return Ok(ReportLine::skip(process.as_str(), "not running"));
    };
    if !pid_is_running(pid) {
        let _ignored = tokio::fs::remove_file(&pid_path).await;
        return Ok(ReportLine::skip(process.as_str(), "stale pid removed"));
    }
    terminate_process_group(pid).await?;
    wait_for_process_port_closed(config, process).await?;
    let _ignored = tokio::fs::remove_file(&pid_path).await;
    Ok(ReportLine::pass(
        process.as_str(),
        format!("stopped pid {pid}"),
    ))
}

pub(super) fn read_log_tail(path: &Path, max_bytes: u64) -> Result<String, LocalDemoError> {
    if !path.exists() {
        return Ok(format!("{} is absent", path.display()));
    }
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(std::io::SeekFrom::Start(start))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn running_pid_from_file(pid_path: &Path) -> Result<Option<u32>, LocalDemoError> {
    match read_pid(pid_path) {
        Ok(pid) if pid_is_running(pid) => Ok(Some(pid)),
        Ok(_) | Err(LocalDemoError::Io(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn prepare_process_dirs(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    tokio::fs::create_dir_all(&config.pid_dir).await?;
    tokio::fs::create_dir_all(&config.log_dir).await?;
    Ok(())
}

fn open_process_logs(log_path: &Path) -> Result<(File, File), LocalDemoError> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let log_err = log.try_clone()?;
    Ok((log, log_err))
}

fn spawn_demo_process(
    config: &LocalDemoConfig,
    process: DemoProcess,
    log: File,
    log_err: File,
) -> Result<u32, LocalDemoError> {
    let mut command = demo_process_command(config, process);
    command
        .envs(config.child_env())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn()?;
    child
        .id()
        .ok_or_else(|| LocalDemoError::Process("spawned child has no pid".to_string()))
}

fn demo_process_command(config: &LocalDemoConfig, process: DemoProcess) -> Command {
    match process {
        DemoProcess::Api => {
            let mut cmd = Command::new("cargo");
            cmd.args(["run", "-p", "api-server", "--bin", "api"])
                .current_dir(&config.repo_root);
            cmd
        }
        DemoProcess::Web => {
            let mut cmd = Command::new("bun");
            let web_port = config.web_port.to_string();
            cmd.args(["run", "dev", "--", "-H", &config.web_host, "-p", &web_port])
                .current_dir(config.repo_root.join("frontends/web"));
            cmd
        }
    }
}

pub(super) fn read_pid(path: &Path) -> Result<u32, LocalDemoError> {
    let text = std::fs::read_to_string(path)?;
    text.trim().parse::<u32>().map_err(|error| {
        LocalDemoError::Process(format!("invalid pid file {}: {error}", path.display()))
    })
}

pub(super) fn pid_is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid.cast_signed()), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _pid = pid;
        false
    }
}

async fn terminate_process_group(pid: u32) -> Result<(), LocalDemoError> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let process_id = i32::try_from(pid)
            .map_err(|_| LocalDemoError::Process(format!("pid {pid} is too large to signal")))?;
        let group_id = process_id.checked_neg().ok_or_else(|| {
            LocalDemoError::Process(format!("pid {pid} cannot form a process group id"))
        })?;
        let group = Pid::from_raw(group_id);
        let process = Pid::from_raw(process_id);
        let _ignored = kill(group, Signal::SIGTERM);
        let _ignored = kill(process, Signal::SIGTERM);
        for _ in 0..20 {
            if !pid_is_running(pid) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let _ignored = kill(group, Signal::SIGKILL);
        let _ignored = kill(process, Signal::SIGKILL);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _pid = pid;
        Err(LocalDemoError::Process(
            "process termination is unsupported on this platform".to_string(),
        ))
    }
}

async fn wait_for_process_port_closed(
    config: &LocalDemoConfig,
    process: DemoProcess,
) -> Result<(), LocalDemoError> {
    let port = process.port(config);
    for _ in 0..25 {
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await;
        if !matches!(result, Ok(Ok(_))) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(LocalDemoError::Process(format!(
        "{} port {port} is still accepting connections after stop",
        process.as_str()
    )))
}
