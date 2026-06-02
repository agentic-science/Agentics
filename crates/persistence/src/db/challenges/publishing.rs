use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::names::ChallengeName;
use agentics_error::{Result, ServiceError};

use super::helpers::{localized_text_to_json, storage_key_from_row};
use super::records::{PublishChallengeInput, PublishChallengeRecord};
use crate::db::ids::challenge_name_from_row;

/// Publish a validated bundle as the benchmark contract for a challenge name.
pub async fn publish_challenge(
    pool: &PgPool,
    input: &PublishChallengeInput<'_>,
) -> Result<PublishChallengeRecord> {
    let mut tx = pool.begin().await?;
    let response = publish_challenge_tx(&mut tx, input).await?;
    tx.commit().await?;
    Ok(response)
}

/// Handles publish challenge tx for this module.
pub async fn publish_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    input: &PublishChallengeInput<'_>,
) -> Result<PublishChallengeRecord> {
    let spec_json =
        serde_json::to_value(input.spec).map_err(|e| ServiceError::Internal(e.to_string()))?;
    let summary_json = localized_text_to_json(input.summary)?;

    let row = sqlx::query(
        r#"
        INSERT INTO challenges (
            challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json,
            starts_at, closes_at, eligibility_policy_json, validation_submission_limit,
            official_submission_limit, leaderboard_visibility, score_distribution_visibility,
            result_detail_visibility, solution_publication_policy, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, 'active')
        ON CONFLICT (challenge_name) DO UPDATE
        SET title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            bundle_key = EXCLUDED.bundle_key,
            public_bundle_key = EXCLUDED.public_bundle_key,
            statement_key = EXCLUDED.statement_key,
            spec_json = EXCLUDED.spec_json,
            starts_at = EXCLUDED.starts_at,
            closes_at = EXCLUDED.closes_at,
            eligibility_policy_json = EXCLUDED.eligibility_policy_json,
            validation_submission_limit = EXCLUDED.validation_submission_limit,
            official_submission_limit = EXCLUDED.official_submission_limit,
            leaderboard_visibility = EXCLUDED.leaderboard_visibility,
            score_distribution_visibility = EXCLUDED.score_distribution_visibility,
            result_detail_visibility = EXCLUDED.result_detail_visibility,
            solution_publication_policy = EXCLUDED.solution_publication_policy,
            status = 'active',
            updated_at = NOW()
        WHERE challenges.spec_json IS NULL
        RETURNING challenge_name, title, bundle_key, public_bundle_key, statement_key
        "#,
    )
    .bind(input.challenge_name.as_str())
    .bind(input.title)
    .bind(&summary_json)
    .bind(input.bundle_key.as_str())
    .bind(input.public_bundle_key.as_str())
    .bind(input.statement_key.as_str())
    .bind(&spec_json)
    .bind(parse_required_time(&input.spec.starts_at)?)
    .bind(parse_optional_time(input.spec.closes_at.as_deref())?)
    .bind(
        serde_json::to_value(&input.spec.eligibility)
            .map_err(|e| ServiceError::Internal(e.to_string()))?,
    )
    .bind(input.spec.validation_submission_limit)
    .bind(input.spec.official_submission_limit)
    .bind(to_json_string(input.spec.visibility.leaderboard)?)
    .bind(to_json_string(input.spec.visibility.score_distribution)?)
    .bind(to_json_string(input.spec.visibility.result_detail)?)
    .bind(to_json_string(input.spec.solution_publication)?)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => ServiceError::Conflict,
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => ServiceError::Conflict,
        error => ServiceError::Database(error),
    })?;

    Ok(PublishChallengeRecord {
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        title: row.try_get("title")?,
        bundle_key: storage_key_from_row(&row, "bundle_key")?,
        public_bundle_key: storage_key_from_row(&row, "public_bundle_key")?,
        statement_key: storage_key_from_row(&row, "statement_key")?,
    })
}

/// Refresh an existing seeded challenge from a local bundle root.
///
/// This intentionally preserves the historical startup seeding behavior: only
/// the display metadata, bundle keys, statement key, stored spec, and active
/// status are refreshed. Draft-driven publication continues to use the guarded
/// publish/archive state machines.
pub async fn refresh_seeded_challenge(
    pool: &PgPool,
    input: &PublishChallengeInput<'_>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE challenges
        SET title = $2,
            summary = $3,
            bundle_key = $4,
            public_bundle_key = $5,
            statement_key = $6,
            spec_json = $7,
            status = 'active',
            updated_at = NOW()
        WHERE challenge_name = $1
        "#,
    )
    .bind(input.challenge_name.as_str())
    .bind(input.title)
    .bind(serde_json::to_value(input.summary).map_err(|e| ServiceError::Internal(e.to_string()))?)
    .bind(input.bundle_key.as_str())
    .bind(input.public_bundle_key.as_str())
    .bind(input.statement_key.as_str())
    .bind(serde_json::to_value(input.spec).map_err(|e| ServiceError::Internal(e.to_string()))?)
    .execute(pool)
    .await?;

    Ok(())
}

/// Archive a challenge shell while preserving private assets and historical submissions.
pub async fn archive_challenge(pool: &PgPool, challenge_name: &ChallengeName) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE challenge_name = $1
        "#,
    )
    .bind(challenge_name.as_str())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::NotFound);
    }
    Ok(())
}

/// Parses required time from an external boundary string.
fn parse_required_time(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|e| ServiceError::Validation(format!("invalid challenge timestamp: {e}")))
}

/// Parses optional time from an external boundary string.
fn parse_optional_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| ServiceError::Validation(format!("invalid challenge timestamp: {e}")))
        })
        .transpose()
}

/// Converts this value to json string.
fn to_json_string<T: serde::Serialize>(value: T) -> Result<String> {
    let value = serde_json::to_value(value).map_err(|e| ServiceError::Internal(e.to_string()))?;
    value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        ServiceError::Internal("challenge enum did not serialize to string".to_string())
    })
}
