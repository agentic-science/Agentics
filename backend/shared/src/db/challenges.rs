//! Challenge shell and published challenge queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::challenge::{
    AdminChallengeListItemDto, ChallengeBundleSpec, ChallengeListItemDto, ChallengeRoundSpec,
    PublishChallengeResponse,
};

/// Published challenge joined with challenge metadata.
#[derive(Debug, Clone)]
pub struct ChallengeRecord {
    pub challenge_id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub bundle_path: String,
    pub statement_path: String,
    pub spec_json: Value,
}

/// Create or update an unpublished challenge shell.
pub async fn create_or_update_challenge(
    pool: &PgPool,
    id: &str,
    slug: &str,
    title: &str,
    summary: &str,
) -> Result<crate::models::challenge::ChallengeAdminResponse> {
    let row = sqlx::query(
        r#"
        INSERT INTO challenges (id, slug, title, summary, status)
        VALUES ($1, $2, $3, $4, 'draft')
        ON CONFLICT (id) DO UPDATE
        SET slug = EXCLUDED.slug,
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            updated_at = NOW()
        WHERE challenges.spec_json IS NULL
        RETURNING id, slug, title, summary, status, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(slug)
    .bind(title)
    .bind(summary)
    .fetch_one(pool)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => AppError::Conflict,
        error => AppError::Database(error),
    })?;

    Ok(crate::models::challenge::ChallengeAdminResponse {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        status: row.try_get("status")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

/// List all challenge shells for admin review.
pub async fn list_admin_challenges(pool: &PgPool) -> Result<Vec<AdminChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT id, slug, title, summary, status, spec_json, created_at, updated_at
        FROM challenges
        ORDER BY updated_at DESC, created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let spec_json: Option<Value> = r.try_get("spec_json")?;
            let spec = spec_json
                .map(serde_json::from_value::<ChallengeBundleSpec>)
                .transpose()
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(AdminChallengeListItemDto {
                id: r.try_get("id")?,
                slug: r.try_get("slug")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                status: r.try_get("status")?,
                benchmark_targets: spec.as_ref().map(|spec| spec.benchmark_targets.clone()),
                rounds: spec.as_ref().map(|spec| spec.rounds.clone()),
                private_benchmark_enabled: spec
                    .as_ref()
                    .map(|spec| spec.datasets.private_benchmark_enabled),
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Publish a validated bundle as the benchmark contract for a challenge id.
pub async fn publish_challenge(
    pool: &PgPool,
    challenge_id: &str,
    bundle_path: &str,
    statement_path: &str,
    spec: &ChallengeBundleSpec,
    title: &str,
    summary: &str,
) -> Result<PublishChallengeResponse> {
    let spec_json = serde_json::to_value(spec).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO challenges (
            id, slug, title, summary, bundle_path, statement_path, spec_json, status
        )
        VALUES ($1, $1, $2, $3, $4, $5, $6, 'active')
        ON CONFLICT (id) DO UPDATE
        SET title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            bundle_path = EXCLUDED.bundle_path,
            statement_path = EXCLUDED.statement_path,
            spec_json = EXCLUDED.spec_json,
            status = 'active',
            updated_at = NOW()
        WHERE challenges.spec_json IS NULL
        RETURNING id AS challenge_id, slug, title, bundle_path, statement_path
        "#,
    )
    .bind(challenge_id)
    .bind(title)
    .bind(summary)
    .bind(bundle_path)
    .bind(statement_path)
    .bind(&spec_json)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => AppError::Conflict,
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => AppError::Conflict,
        error => AppError::Database(error),
    })?;

    insert_rounds(&mut tx, challenge_id, &spec.rounds).await?;
    tx.commit().await?;

    Ok(PublishChallengeResponse {
        challenge_id: row.try_get("challenge_id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        bundle_path: row.try_get("bundle_path")?,
        statement_path: row.try_get("statement_path")?,
    })
}

async fn insert_rounds(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    challenge_id: &str,
    rounds: &[ChallengeRoundSpec],
) -> Result<()> {
    for round in rounds {
        let eligibility = serde_json::to_value(&round.eligibility)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO challenge_rounds (
                challenge_id, round_id, title, opens_at, closes_at,
                eligibility_policy_json, validation_submission_limit,
                official_submission_limit, leaderboard_visibility,
                score_distribution_visibility, result_detail_visibility,
                solution_publication_policy
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(challenge_id)
        .bind(&round.id)
        .bind(&round.title)
        .bind(parse_optional_time(round.opens_at.as_deref())?)
        .bind(parse_optional_time(round.closes_at.as_deref())?)
        .bind(&eligibility)
        .bind(round.validation_submission_limit)
        .bind(round.official_submission_limit)
        .bind(to_json_string(round.visibility.leaderboard)?)
        .bind(to_json_string(round.visibility.score_distribution)?)
        .bind(to_json_string(round.visibility.result_detail)?)
        .bind(to_json_string(round.solution_publication)?)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

fn parse_optional_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .map_err(|e| AppError::Validation(format!("invalid round timestamp: {e}")))
        })
        .transpose()
}

fn to_json_string<T: serde::Serialize>(value: T) -> Result<String> {
    let value = serde_json::to_value(value).map_err(|e| AppError::Internal(e.to_string()))?;
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Internal("round enum did not serialize to string".to_string()))
}

/// Archive a challenge shell while preserving private assets and historical submissions.
pub async fn archive_challenge(pool: &PgPool, challenge_id: &str) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(challenge_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// List active challenges with their published benchmark contract.
pub async fn list_published_challenges(pool: &PgPool) -> Result<Vec<ChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT id, slug, title, summary, spec_json
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let spec: ChallengeBundleSpec = serde_json::from_value(r.try_get("spec_json")?)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(ChallengeListItemDto {
                id: r.try_get("id")?,
                slug: r.try_get("slug")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                rounds: spec.rounds,
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Fetch one active challenge by id or slug.
pub async fn get_published_challenge(
    pool: &PgPool,
    challenge_id_or_slug: &str,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id AS challenge_id, slug, title, summary, bundle_path, statement_path, spec_json
        FROM challenges
        WHERE status = 'active'
          AND spec_json IS NOT NULL
          AND (id = $1 OR slug = $1)
        LIMIT 1
        "#,
    )
    .bind(challenge_id_or_slug)
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

/// Fetch one public challenge detail by id or slug, including archived records
/// that are hidden from default browsing.
pub async fn get_public_challenge(
    pool: &PgPool,
    challenge_id_or_slug: &str,
) -> Result<Option<ChallengeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id AS challenge_id, slug, title, summary, bundle_path, statement_path, spec_json
        FROM challenges
        WHERE status IN ('active', 'archived')
          AND spec_json IS NOT NULL
          AND (id = $1 OR slug = $1)
        LIMIT 1
        "#,
    )
    .bind(challenge_id_or_slug)
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_record).transpose()
}

fn row_to_challenge_record(r: sqlx::postgres::PgRow) -> Result<ChallengeRecord> {
    Ok(ChallengeRecord {
        challenge_id: r.try_get("challenge_id")?,
        slug: r.try_get("slug")?,
        title: r.try_get("title")?,
        summary: r.try_get("summary")?,
        bundle_path: r.try_get("bundle_path")?,
        statement_path: r.try_get("statement_path")?,
        spec_json: r.try_get("spec_json")?,
    })
}
