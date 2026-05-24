use agentics_contracts::zip_project::{
    ZipProjectPhaseFailureReason, ZipProjectPhaseFailureReport, ZipProjectPhaseName,
};
use agentics_domain::error::{Result, ServiceError};

use super::docker::ContainerOutcome;
use super::logs::append_log_excerpt;

/// Local runner failures before conversion into backend service errors.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("Docker API failure: {0}")]
    DockerApiFailure(String),
    #[error("participant run failed: {0}")]
    ParticipantRunFailure(String),
    #[error("runner capacity unavailable: {0}")]
    CapacityUnavailable(String),
    #[error("runner timed out: {0}")]
    Timeout(String),
    #[error("runner cleanup failed: {0}")]
    CleanupFailure(String),
}

impl From<RunnerError> for ServiceError {
    fn from(error: RunnerError) -> Self {
        match error {
            RunnerError::DockerApiFailure(message) => ServiceError::Docker(message),
            RunnerError::ParticipantRunFailure(message)
            | RunnerError::Timeout(message)
            | RunnerError::CleanupFailure(message) => ServiceError::Runner(message),
            RunnerError::CapacityUnavailable(message) => ServiceError::RunnerCapacity(message),
        }
    }
}

/// Ensures container succeeded before continuing.
pub(super) fn ensure_container_succeeded(
    phase: ZipProjectPhaseName,
    outcome: &ContainerOutcome,
    include_log_excerpt: bool,
) -> Result<()> {
    if outcome.timed_out {
        let message = append_log_excerpt("phase timed out", &outcome.logs, include_log_excerpt);
        return Err(RunnerError::Timeout(phase_error_message(
            phase,
            ZipProjectPhaseFailureReason::TimedOut,
            message,
            None,
        ))
        .into());
    }
    if outcome.exit_code != 0 {
        let exit_code = i32::try_from(outcome.exit_code).map_err(|_| {
            ServiceError::Internal(format!(
                "container exit code {} is outside the supported i32 range",
                outcome.exit_code
            ))
        })?;
        let message = append_log_excerpt(
            &format!("phase exited with status {}", outcome.exit_code),
            &outcome.logs,
            include_log_excerpt,
        );
        return Err(RunnerError::ParticipantRunFailure(phase_error_message(
            phase,
            ZipProjectPhaseFailureReason::NonZeroExit,
            message,
            Some(exit_code),
        ))
        .into());
    }
    Ok(())
}

/// Ensures setup succeeded before continuing.
pub(super) fn ensure_setup_succeeded(
    outcome: &ContainerOutcome,
    include_log_excerpt: bool,
) -> Result<()> {
    if outcome.timed_out {
        return Err(RunnerError::Timeout(append_log_excerpt(
            "setup phase timed out",
            &outcome.logs,
            include_log_excerpt,
        ))
        .into());
    }
    if outcome.exit_code != 0 {
        return Err(RunnerError::ParticipantRunFailure(append_log_excerpt(
            &format!("setup phase exited with status {}", outcome.exit_code),
            &outcome.logs,
            include_log_excerpt,
        ))
        .into());
    }
    Ok(())
}

/// Serializes a phase failure report for runner-local errors.
pub(super) fn phase_error_message(
    phase: ZipProjectPhaseName,
    reason: ZipProjectPhaseFailureReason,
    message: String,
    exit_code: Option<i32>,
) -> String {
    let report = ZipProjectPhaseFailureReport {
        phase,
        reason,
        message,
        exit_code,
        log_path: None,
    };
    format!(
        "zip_project phase failed: {}",
        serde_json::to_string(&report)
            .unwrap_or_else(|_| "unserializable phase failure".to_string())
    )
}

/// Converts a phase failure report into the transport-neutral service error.
pub(super) fn phase_error(
    phase: ZipProjectPhaseName,
    reason: ZipProjectPhaseFailureReason,
    message: String,
    exit_code: Option<i32>,
) -> ServiceError {
    let message = phase_error_message(phase, reason, message, exit_code);
    match reason {
        ZipProjectPhaseFailureReason::TimedOut => RunnerError::Timeout(message).into(),
        _ => RunnerError::ParticipantRunFailure(message).into(),
    }
}
