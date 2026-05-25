use sqlx::{PgPool, Postgres, Row, Transaction};

use chrono::{DateTime, Utc};

use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{ChallengeBundleSpec, ChallengeEligibilityType};
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AgentId, ChallengeId};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::storage::StorageKey;

use super::challenges::{
    ChallengeRecord, agent_is_shortlisted, challenge_has_shortlist, get_published_challenge,
    localized_text_from_row,
};
use super::ids::challenge_id_from_row;
use super::ids::challenge_name_from_row;

/// Published challenge admission data needed by API preflight checks.
#[derive(Debug, Clone)]
pub struct PublishedChallengeAdmission {
    pub challenge_id: ChallengeId,
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
    challenge_id: &ChallengeId,
    target: &TargetName,
    eval_type: ScoringMode,
    agent_id: &AgentId,
) -> Result<PublishedChallengeAdmission> {
    let challenge = get_published_challenge(pool, challenge_id).await?;
    let challenge =
        challenge.ok_or_else(|| ServiceError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type(
        pool,
        &challenge.challenge_id,
        &spec,
        target,
        eval_type,
        agent_id,
    )
    .await?;
    ensure_validation_uses_public_bundle(
        eval_type,
        &spec,
        &challenge.bundle_key,
        &challenge.public_bundle_key,
    )?;
    Ok(PublishedChallengeAdmission {
        challenge_id: challenge.challenge_id,
        challenge_name: challenge.challenge_name,
        validation_submission_limit: spec.validation_submission_limit,
        official_submission_limit: spec.official_submission_limit,
    })
}

/// Ensures challenge supports eval type before continuing.
pub(super) async fn ensure_challenge_supports_eval_type(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    spec: &ChallengeBundleSpec,
    target: &TargetName,
    eval_type: ScoringMode,
    agent_id: &AgentId,
) -> Result<()> {
    ensure_challenge_accepts_submissions(spec)?;
    ensure_challenge_eligibility(pool, challenge_id, spec, agent_id).await?;
    ensure_target_supports_eval_type(spec, target, eval_type)
}

/// Validate target and evaluation-mode support using a parsed challenge contract.
fn ensure_target_supports_eval_type(
    spec: &ChallengeBundleSpec,
    target: &TargetName,
    eval_type: ScoringMode,
) -> Result<()> {
    let target = spec.target(target).ok_or_else(|| {
        ServiceError::BadRequest(format!("challenge does not support target `{target}`"))
    })?;

    if eval_type == ScoringMode::Validation && !target.validation_enabled {
        return Err(ServiceError::BadRequest(
            "validation pass is disabled for this challenge and target".to_string(),
        ));
    }
    if eval_type == ScoringMode::Official && !spec.datasets.private_benchmark_enabled {
        return Err(ServiceError::BadRequest(
            "challenge does not have private benchmark data enabled".to_string(),
        ));
    }

    Ok(())
}

/// Lock an active challenge row for an admission transaction.
pub(super) async fn lock_active_challenge_for_admission_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
) -> Result<ChallengeRecord> {
    let row = sqlx::query(
        r#"
        SELECT challenge_id, name AS challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, moltbook_discussion_url
        FROM challenges
        WHERE challenge_id = $1::uuid
          AND status = 'active'
          AND spec_json IS NOT NULL
        FOR UPDATE
        "#,
    )
    .bind(challenge_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let row = row.ok_or_else(|| ServiceError::BadRequest("challenge not found".to_string()))?;
    Ok(ChallengeRecord {
        challenge_id: challenge_id_from_row(&row, "challenge_id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        title: row.try_get("title")?,
        summary: localized_text_from_row(&row, "summary")?,
        bundle_key: storage_key_from_row(&row, "bundle_key")?,
        public_bundle_key: storage_key_from_row(&row, "public_bundle_key")?,
        statement_key: storage_key_from_row(&row, "statement_key")?,
        spec_json: row.try_get("spec_json")?,
        moltbook_discussion_url: optional_moltbook_post_url_from_row(
            &row,
            "moltbook_discussion_url",
        )?,
    })
}

/// Authoritatively verify challenge admission while holding the challenge row lock.
pub(super) async fn ensure_challenge_supports_eval_type_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
    spec: &ChallengeBundleSpec,
    target: &TargetName,
    eval_type: ScoringMode,
    agent_id: &AgentId,
) -> Result<()> {
    ensure_challenge_accepts_submissions(spec)?;
    ensure_challenge_eligibility_tx(tx, challenge_id, spec, agent_id).await?;
    ensure_target_supports_eval_type(spec, target, eval_type)
}

/// Reject validation when the stored public bundle aliases private benchmark data.
pub(super) fn ensure_validation_uses_public_bundle(
    eval_type: ScoringMode,
    spec: &ChallengeBundleSpec,
    bundle_key: &StorageKey,
    public_bundle_key: &StorageKey,
) -> Result<()> {
    if eval_type == ScoringMode::Validation
        && spec.datasets.private_benchmark_enabled
        && bundle_key == public_bundle_key
    {
        return Err(ServiceError::BadRequest(
            "validation is unavailable because this private-benchmark challenge does not have a distinct public bundle key"
                .to_string(),
        ));
    }

    Ok(())
}

/// Ensures challenge accepts submissions before continuing.
fn ensure_challenge_accepts_submissions(spec: &ChallengeBundleSpec) -> Result<()> {
    let now = Utc::now();
    let starts_at = parse_required_challenge_time(&spec.starts_at, "starts_at")?;
    if now < starts_at {
        return Err(ServiceError::Forbidden(
            "challenge has not started yet".to_string(),
        ));
    }
    if let Some(closes_at) = parse_challenge_time(spec.closes_at.as_deref(), "closes_at")?
        && now >= closes_at
    {
        return Err(ServiceError::Forbidden("challenge has closed".to_string()));
    }
    Ok(())
}

/// Parses required challenge time from persisted challenge policy.
fn parse_required_challenge_time(value: &str, field: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|e| ServiceError::Internal(format!("{field} is not valid RFC3339: {e}")))
}

/// Ensures challenge eligibility before continuing.
async fn ensure_challenge_eligibility(
    pool: &PgPool,
    challenge_id: &ChallengeId,
    spec: &ChallengeBundleSpec,
    agent_id: &AgentId,
) -> Result<()> {
    match spec.eligibility.eligibility_type {
        ChallengeEligibilityType::Open => Ok(()),
        ChallengeEligibilityType::PrivateShortlist => {
            if !challenge_has_shortlist(pool, challenge_id).await? {
                return Err(ServiceError::Forbidden(
                    "challenge requires a shortlist, but no shortlist has been uploaded yet"
                        .to_string(),
                ));
            }
            if !agent_is_shortlisted(pool, challenge_id, agent_id).await? {
                return Err(ServiceError::Forbidden(
                    "agent is not eligible for this challenge".to_string(),
                ));
            }
            Ok(())
        }
    }
}

/// Ensures challenge eligibility inside an admission transaction.
async fn ensure_challenge_eligibility_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
    spec: &ChallengeBundleSpec,
    agent_id: &AgentId,
) -> Result<()> {
    match spec.eligibility.eligibility_type {
        ChallengeEligibilityType::Open => Ok(()),
        ChallengeEligibilityType::PrivateShortlist => {
            let has_shortlist = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM challenge_shortlisted_agents
                    WHERE challenge_id = $1::uuid
                )
                "#,
            )
            .bind(challenge_id.as_str())
            .fetch_one(&mut **tx)
            .await?;
            if !has_shortlist {
                return Err(ServiceError::Forbidden(
                    "challenge requires a shortlist, but no shortlist has been uploaded yet"
                        .to_string(),
                ));
            }

            let is_shortlisted = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM challenge_shortlisted_agents
                    WHERE challenge_id = $1::uuid AND agent_id = $2::uuid
                )
                "#,
            )
            .bind(challenge_id.as_str())
            .bind(agent_id.as_str())
            .fetch_one(&mut **tx)
            .await?;
            if !is_shortlisted {
                return Err(ServiceError::Forbidden(
                    "agent is not eligible for this challenge".to_string(),
                ));
            }
            Ok(())
        }
    }
}

/// Read a storage key from a locked challenge row.
fn storage_key_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value)
        .map_err(|e| ServiceError::Internal(format!("stored invalid {column}: {e}")))
}

/// Read an optional Moltbook post URL from a locked challenge row.
fn optional_moltbook_post_url_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<agentics_domain::models::urls::MoltbookPostUrl>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(agentics_domain::models::urls::MoltbookPostUrl::try_new)
        .transpose()
        .map_err(|e| ServiceError::Internal(format!("stored invalid {column}: {e}")))
}

/// Parses challenge time from an external boundary string.
fn parse_challenge_time(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| ServiceError::Internal(format!("invalid challenge {field}: {e}")))
        })
        .transpose()
}
