//! Hosted runner profile probe enforcement.

use std::process::ExitStatus;

use shared::config::{
    Config, DEFAULT_HOST_PROBE_COMMAND, ENV_AGENTICS_HOST_PROBE_COMMAND, HostProbeMode,
};
use tokio::process::Command;
use tracing::{info, warn};

const MAX_PROBE_OUTPUT_BYTES: usize = 8192;

/// Run the configured hosted profile probe before the worker accepts jobs.
pub(crate) async fn enforce_host_probe(config: &Config) -> anyhow::Result<()> {
    match config.host_probe_mode {
        HostProbeMode::Off => Ok(()),
        HostProbeMode::Warn | HostProbeMode::Require => {
            let command = std::env::var(ENV_AGENTICS_HOST_PROBE_COMMAND)
                .unwrap_or_else(|_| DEFAULT_HOST_PROBE_COMMAND.to_string());
            let mode = config.host_probe_mode;
            let output = Command::new(&command)
                .env("AGENTICS_HOST_PROBE_MODE", mode.as_str())
                .output()
                .await;
            match output {
                Ok(output) if output.status.success() => {
                    info!("host profile probe passed");
                    Ok(())
                }
                Ok(output) => handle_probe_failure(
                    mode,
                    format_probe_failure(Some(output.status), &output.stdout, &output.stderr),
                ),
                Err(error) => handle_probe_failure(
                    mode,
                    format!("failed to run host profile probe `{command}`: {error}"),
                ),
            }
        }
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
fn format_probe_failure(status: Option<ExitStatus>, stdout: &[u8], stderr: &[u8]) -> String {
    let status = status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "unknown status".to_string());
    let stdout = bounded_utf8(stdout);
    let stderr = bounded_utf8(stderr);
    format!("host profile probe failed with {status}\nstdout:\n{stdout}\nstderr:\n{stderr}")
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
    use shared::config::HostProbeMode;

    use super::{bounded_utf8, handle_probe_failure};

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
}
