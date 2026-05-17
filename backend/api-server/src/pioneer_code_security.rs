//! Shared controls for MVP pioneer-code verification.

use std::time::Duration;

use shared::error::AppError;
use shared::models::pioneer_codes::INVALID_OR_UNAVAILABLE_PIONEER_CODE;

/// Minimum response delay applied to failed pioneer-code verification.
const FAILED_PIONEER_CODE_DELAY: Duration = Duration::from_millis(500);

/// Sleep before returning a failed pioneer-code response.
///
/// The MVP code format is intentionally short, so every failed verification path
/// pays the same delay and returns the same generic message. Successful
/// registrations do not call this helper.
pub(crate) async fn reject_failed_pioneer_code() -> AppError {
    tokio::time::sleep(FAILED_PIONEER_CODE_DELAY).await;
    invalid_pioneer_code()
}

/// Return the generic pioneer-code rejection without timing mitigation.
pub(crate) fn invalid_pioneer_code() -> AppError {
    AppError::Forbidden(INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string())
}

/// Return whether an application error is the generic pioneer-code rejection.
pub(crate) fn is_invalid_pioneer_code(error: &AppError) -> bool {
    matches!(error, AppError::Forbidden(message) if message == INVALID_OR_UNAVAILABLE_PIONEER_CODE)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::reject_failed_pioneer_code;

    /// Verifies that failed pioneer-code paths pay the intended minimum delay.
    #[tokio::test]
    async fn failed_pioneer_code_rejection_waits_before_returning() {
        let started = Instant::now();
        let _error = reject_failed_pioneer_code().await;

        assert!(started.elapsed() >= Duration::from_millis(450));
    }
}
