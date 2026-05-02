use sqlx::PgPool;

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::evaluation::ScoringMode;

use super::challenges::get_published_challenge;

/// Verify that a published challenge accepts the requested evaluation mode.
///
/// API handlers call this before storing uploaded artifacts so disabled
/// validation does not consume storage; write paths repeat the same check inside
/// their transaction as the authoritative guard.
pub async fn ensure_published_challenge_supports_eval_type(
    pool: &PgPool,
    challenge_id: &str,
    eval_type: ScoringMode,
) -> Result<()> {
    let challenge = get_published_challenge(pool, challenge_id).await?;
    let challenge =
        challenge.ok_or_else(|| AppError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type(&spec, eval_type)
}

pub(super) fn ensure_challenge_supports_eval_type(
    spec: &ChallengeBundleSpec,
    eval_type: ScoringMode,
) -> Result<()> {
    if eval_type == ScoringMode::Validation && !spec.datasets.validation_enabled {
        return Err(AppError::BadRequest(
            "validation pass is disabled for this challenge version".to_string(),
        ));
    }
    if eval_type == ScoringMode::Official && !spec.datasets.private_benchmark_enabled {
        return Err(AppError::BadRequest(
            "challenge version does not have private benchmark data enabled".to_string(),
        ));
    }

    Ok(())
}
