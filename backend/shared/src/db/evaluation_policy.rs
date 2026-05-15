use sqlx::PgPool;

use chrono::{DateTime, Utc};

use crate::error::{AppError, Result};
use crate::models::challenge::{ChallengeBundleSpec, ChallengeEligibilityType};
use crate::models::evaluation::ScoringMode;
use crate::models::names::{ChallengeName, TargetName};

use super::challenges::{agent_is_shortlisted, challenge_has_shortlist, get_published_challenge};

/// Published challenge admission data needed by API preflight checks.
#[derive(Debug, Clone)]
pub struct PublishedChallengeAdmission {
    pub challenge_name: ChallengeName,
    pub validation_submission_limit: Option<i64>,
    pub official_submission_limit: Option<i64>,
}

/// Verify that a published challenge accepts the requested evaluation mode and
/// return the canonical challenge name plus challenge-scoped limits.
///
/// API handlers call this before storing uploaded artifacts so disabled
/// validation does not consume storage; write paths repeat the same check before
/// inserting queued work as the authoritative guard.
pub async fn ensure_published_challenge_supports_eval_type(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
    agent_id: &str,
) -> Result<PublishedChallengeAdmission> {
    let challenge = get_published_challenge(pool, challenge_name).await?;
    let challenge =
        challenge.ok_or_else(|| AppError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type(
        pool,
        &challenge.challenge_name,
        &spec,
        target,
        eval_type,
        agent_id,
    )
    .await?;
    Ok(PublishedChallengeAdmission {
        challenge_name: challenge.challenge_name,
        validation_submission_limit: spec.validation_submission_limit,
        official_submission_limit: spec.official_submission_limit,
    })
}

pub(super) async fn ensure_challenge_supports_eval_type(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    spec: &ChallengeBundleSpec,
    target: &TargetName,
    eval_type: ScoringMode,
    agent_id: &str,
) -> Result<()> {
    ensure_challenge_accepts_submissions(spec)?;
    ensure_challenge_eligibility(pool, challenge_name, spec, agent_id).await?;

    let target = spec.target(target).ok_or_else(|| {
        AppError::BadRequest(format!("challenge does not support target `{target}`"))
    })?;

    if eval_type == ScoringMode::Validation && !target.validation_enabled {
        return Err(AppError::BadRequest(
            "validation pass is disabled for this challenge and target".to_string(),
        ));
    }
    if eval_type == ScoringMode::Official && !spec.datasets.private_benchmark_enabled {
        return Err(AppError::BadRequest(
            "challenge does not have private benchmark data enabled".to_string(),
        ));
    }

    Ok(())
}

fn ensure_challenge_accepts_submissions(spec: &ChallengeBundleSpec) -> Result<()> {
    let now = Utc::now();
    if let Some(starts_at) = parse_challenge_time(spec.starts_at.as_deref(), "starts_at")?
        && now < starts_at
    {
        return Err(AppError::Forbidden(
            "challenge has not started yet".to_string(),
        ));
    }
    if let Some(closes_at) = parse_challenge_time(spec.closes_at.as_deref(), "closes_at")?
        && now >= closes_at
    {
        return Err(AppError::Forbidden("challenge has closed".to_string()));
    }
    Ok(())
}

async fn ensure_challenge_eligibility(
    pool: &PgPool,
    challenge_name: &ChallengeName,
    spec: &ChallengeBundleSpec,
    agent_id: &str,
) -> Result<()> {
    match spec.eligibility.eligibility_type {
        ChallengeEligibilityType::Open => Ok(()),
        ChallengeEligibilityType::PrivateShortlist => {
            if !challenge_has_shortlist(pool, challenge_name).await? {
                return Err(AppError::Forbidden(
                    "challenge requires a shortlist, but no shortlist has been uploaded yet"
                        .to_string(),
                ));
            }
            if !agent_is_shortlisted(pool, challenge_name, agent_id).await? {
                return Err(AppError::Forbidden(
                    "agent is not eligible for this challenge".to_string(),
                ));
            }
            Ok(())
        }
    }
}

fn parse_challenge_time(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Internal(format!("invalid challenge {field}: {e}")))
        })
        .transpose()
}
