//! Shared operational support primitives.
//!
//! These helpers keep operational binaries consistent without merging unrelated
//! tasks into one executable. They provide deterministic check output, bounded
//! external process calls at true OS boundaries, cancellation-aware command
//! execution, and small filesystem safety utilities.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, ExitStatus, Stdio};
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

/// Default byte cap for command output captured into diagnostics.
pub const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 16 * 1024;

/// Exit code used by POSIX shells for Ctrl-C.
pub const INTERRUPTED_EXIT: u8 = 130;

/// Environment variable for the Docker host URI used by ops commands.
pub const ENV_DOCKER_HOST: &str = "AGENTICS_DOCKER_HOST";

/// Environment variable for the host Docker socket path mounted into containers.
pub const ENV_DOCKER_SOCKET_PATH: &str = "AGENTICS_DOCKER_SOCKET_PATH";

/// Default host Docker socket path for production Compose.
pub const DEFAULT_DOCKER_SOCKET_PATH: &str = "/var/run/docker.sock";

/// Read a non-empty environment variable.
pub fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// A displayable operational check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportLine {
    name: String,
    status: ReportStatus,
}

impl ReportLine {
    /// Build a passing report.
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ReportStatus::Pass(message.into()),
        }
    }

    /// Build a skipped report.
    pub fn skip(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ReportStatus::Skip(message.into()),
        }
    }

    /// Build a failing report.
    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ReportStatus::Fail(message.into()),
        }
    }

    /// Whether this line is a failure.
    pub fn is_failure(&self) -> bool {
        matches!(self.status, ReportStatus::Fail(_))
    }

    /// Print one line using the requested prefix.
    pub fn print(&self, prefix: &str) {
        let (label, message) = match &self.status {
            ReportStatus::Pass(message) => ("PASS", message),
            ReportStatus::Skip(message) => ("SKIP", message),
            ReportStatus::Fail(message) => ("FAIL", message),
        };
        println!("[{prefix}] {label} {} - {message}", self.name);
    }
}

/// Status for one operational report line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportStatus {
    Pass(String),
    Skip(String),
    Fail(String),
}

/// Print reports in their existing order and return a process exit code.
pub fn print_reports(prefix: &str, reports: &[ReportLine]) -> ExitCode {
    for report in reports {
        report.print(prefix);
    }
    if reports.iter().any(ReportLine::is_failure) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Return Ctrl-C exit if cancellation wins the race.
pub async fn run_with_ctrl_c<F>(prefix: &'static str, future: F) -> ExitCode
where
    F: Future<Output = ExitCode>,
{
    tokio::select! {
        code = future => code,
        signal = tokio::signal::ctrl_c() => {
            match signal {
                Ok(()) => eprintln!("[{prefix}] interrupted by Ctrl-C"),
                Err(error) => eprintln!("[{prefix}] failed to listen for Ctrl-C: {error}"),
            }
            ExitCode::from(INTERRUPTED_EXIT)
        }
    }
}

/// Bounded command output captured from a process boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

impl CommandOutput {
    /// Whether the command exited successfully.
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }

    /// Combined bounded stdout and stderr with newlines normalized for errors.
    pub fn combined(&self) -> String {
        let mut text = String::new();
        if !self.stdout.trim().is_empty() {
            text.push_str(self.stdout.trim());
        }
        if !self.stderr.trim().is_empty() {
            if !text.is_empty() {
                text.push_str("; ");
            }
            text.push_str(self.stderr.trim());
        }
        if self.truncated {
            if !text.is_empty() {
                text.push_str("; ");
            }
            text.push_str("output truncated");
        }
        text
    }
}

/// Run one process with bounded output and optional timeout.
pub async fn run_process<I, S>(
    program: &str,
    args: I,
    timeout: Option<Duration>,
    limit_bytes: usize,
) -> Result<CommandOutput, SupportError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    run_command(command, program, timeout, limit_bytes).await
}

/// Run one configured process with bounded output and optional timeout.
pub async fn run_command(
    mut command: Command,
    program: &str,
    timeout: Option<Duration>,
    limit_bytes: usize,
) -> Result<CommandOutput, SupportError> {
    let mut child = command
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| SupportError::ProcessStart {
            program: program.to_string(),
            message: error.to_string(),
        })?;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    let completion = wait_command_completion(&mut child, &mut stdout, &mut stderr, limit_bytes);
    let (status, stdout, stdout_truncated, stderr, stderr_truncated) = match timeout {
        Some(duration) => match tokio::time::timeout(duration, completion).await {
            Ok(result) => result,
            Err(_) => {
                let _ignored = child.kill().await;
                let _ignored = child.wait().await;
                return Err(SupportError::ProcessTimeout {
                    program: program.to_string(),
                    seconds: duration.as_secs(),
                });
            }
        },
        None => completion.await,
    }
    .map_err(|error| SupportError::ProcessWait {
        program: program.to_string(),
        message: error.to_string(),
    })?;

    Ok(CommandOutput {
        status: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated: stdout_truncated || stderr_truncated,
    })
}

async fn wait_command_completion(
    child: &mut Child,
    stdout: &mut Option<ChildStdout>,
    stderr: &mut Option<ChildStderr>,
    limit_bytes: usize,
) -> Result<(ExitStatus, Vec<u8>, bool, Vec<u8>, bool), std::io::Error> {
    let (status, stdout, stderr) = tokio::join!(
        child.wait(),
        read_bounded(stdout, limit_bytes),
        read_bounded(stderr, limit_bytes)
    );
    let status = status?;
    let (stdout, stdout_truncated) = stdout?;
    let (stderr, stderr_truncated) = stderr?;

    Ok((status, stdout, stdout_truncated, stderr, stderr_truncated))
}

async fn read_bounded<R>(
    stream: &mut Option<R>,
    limit_bytes: usize,
) -> Result<(Vec<u8>, bool), std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut truncated = false;
    let Some(stream) = stream.as_mut() else {
        return Ok((bytes, truncated));
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
        append_bounded_bytes(&mut bytes, slice, limit_bytes, &mut truncated);
    }
    Ok((bytes, truncated))
}

/// Append bytes to a diagnostic buffer without growing past a hard cap.
pub fn append_bounded_bytes(
    output: &mut Vec<u8>,
    chunk: &[u8],
    limit_bytes: usize,
    truncated: &mut bool,
) {
    if output.len() >= limit_bytes {
        *truncated = *truncated || !chunk.is_empty();
        return;
    }
    let remaining = limit_bytes.saturating_sub(output.len());
    if chunk.len() > remaining {
        output.extend(chunk.iter().take(remaining).copied());
        *truncated = true;
    } else {
        output.extend_from_slice(chunk);
    }
}

/// Convert bytes to UTF-8 while enforcing a display limit.
pub fn bounded_utf8(bytes: &[u8], limit: usize) -> (String, bool) {
    let truncated = bytes.len() > limit;
    let visible = if truncated {
        bytes.get(..limit).unwrap_or(bytes)
    } else {
        bytes
    };
    (String::from_utf8_lossy(visible).into_owned(), truncated)
}

/// Require that a path is absolute.
pub fn require_absolute_path(path: &Path, label: &str) -> Result<(), SupportError> {
    if !path.is_absolute() {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "path must be absolute".to_string(),
        });
    }
    Ok(())
}

/// Reject clearly unsafe destructive paths.
pub fn require_safe_destructive_path(
    path: &Path,
    label: &str,
    allowed_roots: &[PathBuf],
) -> Result<(), SupportError> {
    require_absolute_path(path, label)?;
    reject_parent_components(path, label)?;
    if path == Path::new("/") {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "refusing to operate on filesystem root".to_string(),
        });
    }
    let normalized_path = normalize_existing_path(path, label)?;
    let mut matched_root = false;
    for root in allowed_roots {
        require_absolute_path(root, "allowed root")?;
        reject_parent_components(root, "allowed root")?;
        if root == Path::new("/") {
            continue;
        }
        let normalized_root = normalize_existing_path(root, "allowed root")?;
        if normalized_path == normalized_root || normalized_path.starts_with(&normalized_root) {
            matched_root = true;
            break;
        }
    }
    if !matched_root {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "path is outside the allowed operation roots".to_string(),
        });
    }
    Ok(())
}

fn reject_parent_components(path: &Path, label: &str) -> Result<(), SupportError> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "path must not contain parent-directory traversal".to_string(),
        });
    }
    Ok(())
}

fn normalize_existing_path(path: &Path, label: &str) -> Result<PathBuf, SupportError> {
    if std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "refusing to operate on a symlink path".to_string(),
        });
    }
    match std::fs::canonicalize(path) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: format!("failed to canonicalize existing path: {error}"),
        }),
    }
}

/// Errors from shared ops support helpers.
#[derive(Debug, Error)]
pub enum SupportError {
    #[error("failed to start {program}: {message}")]
    ProcessStart { program: String, message: String },
    #[error("failed while waiting for {program}: {message}")]
    ProcessWait { program: String, message: String },
    #[error("{program} timed out after {seconds}s")]
    ProcessTimeout { program: String, seconds: u64 },
    #[error("unsafe {label} path {path:?}: {message}")]
    UnsafePath {
        label: String,
        path: PathBuf,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        SupportError, append_bounded_bytes, bounded_utf8, require_safe_destructive_path,
        run_command,
    };
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;

    /// Verifies byte bounding reports truncation.
    #[test]
    fn bounded_utf8_reports_truncation() {
        let (text, truncated) = bounded_utf8(b"abcdef", 3);
        assert_eq!(text, "abc");
        assert!(truncated);
    }

    /// Verifies destructive path safety is rooted.
    #[test]
    fn destructive_paths_must_stay_under_allowed_roots() {
        let roots = [PathBuf::from("/srv/agentics")];
        assert!(
            require_safe_destructive_path(Path::new("/srv/agentics/runtime"), "x", &roots).is_ok()
        );
        assert!(require_safe_destructive_path(Path::new("/etc"), "x", &roots).is_err());
        assert!(require_safe_destructive_path(Path::new("/"), "x", &roots).is_err());
        assert!(
            require_safe_destructive_path(Path::new("/srv/agentics/../docs"), "x", &roots).is_err()
        );
    }

    /// Verifies diagnostic buffers never grow past their hard cap.
    #[test]
    fn appends_bounded_bytes_without_overallocation() {
        let mut output = Vec::new();
        let mut truncated = false;
        append_bounded_bytes(&mut output, b"abcdef", 3, &mut truncated);
        append_bounded_bytes(&mut output, b"ghij", 3, &mut truncated);
        assert_eq!(output, b"abc");
        assert!(truncated);
    }

    /// Verifies timeout kills a slow child and reports the command timeout.
    #[tokio::test]
    async fn run_command_times_out_slow_child() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("sleep 5").stdin(Stdio::null());

        let error = run_command(command, "sh", Some(Duration::from_millis(50)), 1024)
            .await
            .expect_err("slow command should time out");

        assert!(matches!(
            error,
            SupportError::ProcessTimeout { program, .. } if program == "sh"
        ));
    }
}
