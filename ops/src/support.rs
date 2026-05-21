//! Shared operational support primitives.
//!
//! These helpers keep operational binaries consistent without merging unrelated
//! tasks into one executable. They provide typed environment parsing,
//! deterministic check output, bounded external process calls at true OS
//! boundaries, cancellation-aware command execution, and small filesystem
//! safety utilities.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Default byte cap for command output captured into diagnostics.
pub const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 16 * 1024;

/// Exit code used by POSIX shells for Ctrl-C.
pub const INTERRUPTED_EXIT: u8 = 130;

/// Read a non-empty environment variable.
pub fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Parse a non-empty environment variable with a domain parser.
pub fn parse_env<T>(name: &str) -> Result<Option<T>, SupportError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    env_non_empty(name)
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|error| SupportError::InvalidEnv {
                    name: name.to_string(),
                    value,
                    message: error.to_string(),
                })
        })
        .transpose()
}

/// Parse an optional boolean environment variable.
pub fn parse_bool_env(name: &str, default: bool) -> Result<bool, SupportError> {
    let Some(value) = env_non_empty(name) else {
        return Ok(default);
    };
    parse_boolish(name, &value)
}

/// Parse a boolean-like value used by legacy operational env flags.
pub fn parse_boolish(name: &str, value: &str) -> Result<bool, SupportError> {
    match value.trim() {
        "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
        "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
        other => Err(SupportError::InvalidEnv {
            name: name.to_string(),
            value: other.to_string(),
            message: "expected true/false or 1/0".to_string(),
        }),
    }
}

/// Parse a positive integer environment variable.
pub fn parse_positive_env<T>(name: &str, default: T) -> Result<T, SupportError>
where
    T: std::str::FromStr + PartialOrd + From<u8> + Copy,
    T::Err: std::fmt::Display,
{
    let Some(value) = env_non_empty(name) else {
        return Ok(default);
    };
    let parsed = value
        .parse::<T>()
        .map_err(|error| SupportError::InvalidEnv {
            name: name.to_string(),
            value: value.clone(),
            message: error.to_string(),
        })?;
    if parsed <= T::from(0) {
        return Err(SupportError::InvalidEnv {
            name: name.to_string(),
            value,
            message: "must be greater than zero".to_string(),
        });
    }
    Ok(parsed)
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
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| SupportError::ProcessStart {
            program: program.to_string(),
            message: error.to_string(),
        })?;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(stream) = stdout.as_mut() {
            stream.read_to_end(&mut bytes).await?;
        }
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(stream) = stderr.as_mut() {
            stream.read_to_end(&mut bytes).await?;
        }
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });

    let status = match timeout {
        Some(duration) => {
            tokio::select! {
                result = child.wait() => {
                    result.map_err(|error| SupportError::ProcessWait {
                        program: program.to_string(),
                        message: error.to_string(),
                    })?
                }
                _ = tokio::time::sleep(duration) => {
                    let _ignored = child.kill().await;
                    return Err(SupportError::ProcessTimeout {
                        program: program.to_string(),
                        seconds: duration.as_secs(),
                    });
                }
            }
        }
        None => child
            .wait()
            .await
            .map_err(|error| SupportError::ProcessWait {
                program: program.to_string(),
                message: error.to_string(),
            })?,
    };

    let stdout = stdout_task
        .await
        .map_err(|error| SupportError::Join(error.to_string()))?
        .map_err(|error| SupportError::ProcessWait {
            program: program.to_string(),
            message: error.to_string(),
        })?;
    let stderr = stderr_task
        .await
        .map_err(|error| SupportError::Join(error.to_string()))?
        .map_err(|error| SupportError::ProcessWait {
            program: program.to_string(),
            message: error.to_string(),
        })?;

    Ok(CommandOutput {
        status: status.code(),
        stdout: bounded_utf8(&stdout, limit_bytes).0,
        stderr: bounded_utf8(&stderr, limit_bytes).0,
        truncated: stdout.len() > limit_bytes || stderr.len() > limit_bytes,
    })
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
    if path == Path::new("/") {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "refusing to operate on filesystem root".to_string(),
        });
    }
    if !allowed_roots
        .iter()
        .any(|root| path == root || path.starts_with(root))
    {
        return Err(SupportError::UnsafePath {
            label: label.to_string(),
            path: path.to_path_buf(),
            message: "path is outside the allowed operation roots".to_string(),
        });
    }
    Ok(())
}

/// Errors from shared ops support helpers.
#[derive(Debug, Error)]
pub enum SupportError {
    #[error("invalid environment variable {name}={value:?}: {message}")]
    InvalidEnv {
        name: String,
        value: String,
        message: String,
    },
    #[error("failed to start {program}: {message}")]
    ProcessStart { program: String, message: String },
    #[error("failed while waiting for {program}: {message}")]
    ProcessWait { program: String, message: String },
    #[error("{program} timed out after {seconds}s")]
    ProcessTimeout { program: String, seconds: u64 },
    #[error("task join failure: {0}")]
    Join(String),
    #[error("unsafe {label} path {path:?}: {message}")]
    UnsafePath {
        label: String,
        path: PathBuf,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{bounded_utf8, parse_boolish, require_safe_destructive_path};
    use std::path::{Path, PathBuf};

    /// Verifies byte bounding reports truncation.
    #[test]
    fn bounded_utf8_reports_truncation() {
        let (text, truncated) = bounded_utf8(b"abcdef", 3);
        assert_eq!(text, "abc");
        assert!(truncated);
    }

    /// Verifies legacy boolean values stay explicit.
    #[test]
    fn parse_boolish_accepts_only_known_values() {
        assert!(parse_boolish("X", "1").unwrap());
        assert!(!parse_boolish("X", "false").unwrap());
        assert!(parse_boolish("X", "maybe").is_err());
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
    }
}
