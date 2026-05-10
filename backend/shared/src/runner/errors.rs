use crate::error::{AppError, Result};
use crate::zip_project::{
    ZipProjectPhaseFailureReason, ZipProjectPhaseFailureReport, ZipProjectPhaseName,
};

use super::docker::ContainerOutcome;
use super::logs::append_log_excerpt;

pub(super) fn ensure_container_succeeded(
    phase: ZipProjectPhaseName,
    outcome: &ContainerOutcome,
) -> Result<()> {
    if outcome.timed_out {
        let message = append_log_excerpt("phase timed out", &outcome.logs);
        return Err(phase_error(
            phase,
            ZipProjectPhaseFailureReason::TimedOut,
            message,
            None,
        ));
    }
    if outcome.exit_code != 0 {
        let exit_code = i32::try_from(outcome.exit_code).map_err(|_| {
            AppError::Internal(format!(
                "container exit code {} is outside the supported i32 range",
                outcome.exit_code
            ))
        })?;
        let message = append_log_excerpt(
            &format!("phase exited with status {}", outcome.exit_code),
            &outcome.logs,
        );
        return Err(phase_error(
            phase,
            ZipProjectPhaseFailureReason::NonZeroExit,
            message,
            Some(exit_code),
        ));
    }
    Ok(())
}

pub(super) fn ensure_prepare_succeeded(outcome: &ContainerOutcome) -> Result<()> {
    if outcome.timed_out {
        return Err(AppError::Runner(append_log_excerpt(
            "prepare phase timed out",
            &outcome.logs,
        )));
    }
    if outcome.exit_code != 0 {
        return Err(AppError::Runner(append_log_excerpt(
            &format!("prepare phase exited with status {}", outcome.exit_code),
            &outcome.logs,
        )));
    }
    Ok(())
}

pub(super) fn phase_error(
    phase: ZipProjectPhaseName,
    reason: ZipProjectPhaseFailureReason,
    message: String,
    exit_code: Option<i32>,
) -> AppError {
    let report = ZipProjectPhaseFailureReport {
        phase,
        reason,
        message,
        exit_code,
        log_path: None,
    };
    AppError::Runner(format!(
        "zip_project phase failed: {}",
        serde_json::to_string(&report)
            .unwrap_or_else(|_| "unserializable phase failure".to_string())
    ))
}
