use sqlx::PgPool;

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::evaluation::ScoringMode;

use super::challenges::get_published_challenge;

/// Verify that a published challenge accepts the requested evaluation mode and
/// return the canonical challenge id.
///
/// API handlers call this before storing uploaded artifacts so disabled
/// validation does not consume storage; write paths repeat the same check inside
/// their transaction as the authoritative guard.
pub async fn ensure_published_challenge_supports_eval_type(
    pool: &PgPool,
    challenge_id_or_slug: &str,
    benchmark_target_id: &str,
    eval_type: ScoringMode,
) -> Result<String> {
    let challenge = get_published_challenge(pool, challenge_id_or_slug).await?;
    let challenge =
        challenge.ok_or_else(|| AppError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type(&spec, benchmark_target_id, eval_type)?;
    Ok(challenge.challenge_id)
}

pub(super) fn ensure_challenge_supports_eval_type(
    spec: &ChallengeBundleSpec,
    benchmark_target_id: &str,
    eval_type: ScoringMode,
) -> Result<()> {
    let target = spec.benchmark_target(benchmark_target_id).ok_or_else(|| {
        AppError::BadRequest(format!(
            "challenge version does not support benchmark target `{benchmark_target_id}`"
        ))
    })?;

    if eval_type == ScoringMode::Validation && !target.validation_enabled {
        return Err(AppError::BadRequest(
            "validation pass is disabled for this challenge version and benchmark target"
                .to_string(),
        ));
    }
    if eval_type == ScoringMode::Official && !spec.datasets.private_benchmark_enabled {
        return Err(AppError::BadRequest(
            "challenge version does not have private benchmark data enabled".to_string(),
        ));
    }

    Ok(())
}
