use sqlx::PgPool;

use chrono::{DateTime, Utc};

use crate::error::{AppError, Result};
use crate::models::challenge::{ChallengeBundleSpec, ChallengeRoundSpec};
use crate::models::evaluation::ScoringMode;

use super::challenges::get_published_challenge;

/// Verify that a published challenge accepts the requested evaluation mode and
/// return the canonical challenge id.
///
/// API handlers call this before storing uploaded artifacts so disabled
/// validation does not consume storage; write paths repeat the same check inside
/// their transaction as the authoritative guard.
pub async fn ensure_published_challenge_round_supports_eval_type(
    pool: &PgPool,
    challenge_id_or_slug: &str,
    round_id: &str,
    benchmark_target_id: &str,
    eval_type: ScoringMode,
) -> Result<String> {
    let challenge = get_published_challenge(pool, challenge_id_or_slug).await?;
    let challenge =
        challenge.ok_or_else(|| AppError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    ensure_challenge_round_supports_eval_type(&spec, round_id, benchmark_target_id, eval_type)?;
    Ok(challenge.challenge_id)
}

pub(super) fn ensure_challenge_round_supports_eval_type(
    spec: &ChallengeBundleSpec,
    round_id: &str,
    benchmark_target_id: &str,
    eval_type: ScoringMode,
) -> Result<()> {
    let round = spec.round(round_id).ok_or_else(|| {
        AppError::BadRequest(format!("challenge does not declare round `{round_id}`"))
    })?;
    ensure_round_accepts_submissions(round)?;

    let target = spec.benchmark_target(benchmark_target_id).ok_or_else(|| {
        AppError::BadRequest(format!(
            "challenge does not support benchmark target `{benchmark_target_id}`"
        ))
    })?;

    if eval_type == ScoringMode::Validation && !target.validation_enabled {
        return Err(AppError::BadRequest(
            "validation pass is disabled for this challenge round and benchmark target".to_string(),
        ));
    }
    if eval_type == ScoringMode::Official && !spec.datasets.private_benchmark_enabled {
        return Err(AppError::BadRequest(
            "challenge does not have private benchmark data enabled".to_string(),
        ));
    }

    Ok(())
}

fn ensure_round_accepts_submissions(round: &ChallengeRoundSpec) -> Result<()> {
    let now = Utc::now();
    if let Some(opens_at) = parse_round_time(round.opens_at.as_deref(), "opens_at")?
        && now < opens_at
    {
        return Err(AppError::BadRequest(format!(
            "round `{}` is not open yet",
            round.id
        )));
    }
    if let Some(closes_at) = parse_round_time(round.closes_at.as_deref(), "closes_at")?
        && now >= closes_at
    {
        return Err(AppError::BadRequest(format!(
            "round `{}` is closed",
            round.id
        )));
    }
    Ok(())
}

fn parse_round_time(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Internal(format!("invalid round {field}: {e}")))
        })
        .transpose()
}
