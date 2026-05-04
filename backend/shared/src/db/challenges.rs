//! Challenge shell and published version queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::challenge::{
    AdminChallengeListItemDto, ChallengeBundleSpec, ChallengeListItemDto,
    CreateChallengeVersionResponse,
};

/// Latest published challenge version joined with challenge metadata.
#[derive(Debug, Clone)]
pub struct ChallengeVersionRecord {
    pub challenge_id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub challenge_version_id: String,
    pub version: String,
    pub bundle_path: String,
    pub statement_path: String,
    pub spec_json: Value,
}

/// Create or update the challenge shell that versions attach to.
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
        VALUES ($1, $2, $3, $4, 'active')
        ON CONFLICT (id) DO UPDATE
        SET slug = EXCLUDED.slug,
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            status = 'active',
            updated_at = NOW()
        RETURNING id, slug, title, summary, status, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(slug)
    .bind(title)
    .bind(summary)
    .fetch_one(pool)
    .await?;

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

/// List all challenge shells for admin review, including drafts without versions.
pub async fn list_admin_challenges(pool: &PgPool) -> Result<Vec<AdminChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            p.id,
            p.slug,
            p.title,
            p.summary,
            p.status,
            p.created_at,
            p.updated_at,
            pv.id AS version_id,
            pv.version,
            pv.spec_json
        FROM challenges p
        LEFT JOIN challenge_versions pv ON pv.id = p.current_version_id
        ORDER BY p.updated_at DESC, p.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let version_id: Option<String> = r.try_get("version_id")?;
            let version: Option<String> = r.try_get("version")?;
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
                current_version: version_id
                    .zip(version)
                    .map(|(id, version)| crate::models::CurrentVersionDto { id, version }),
                current_benchmark_targets: spec.as_ref().map(|spec| spec.benchmark_targets.clone()),
                private_benchmark_enabled: spec
                    .as_ref()
                    .map(|spec| spec.datasets.private_benchmark_enabled),
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Publish a validated bundle as the current challenge version.
///
/// Publishing a different version preserves older records and marks the
/// previous current version `superseded` so historical leaderboards and
/// solution submissions stay attached to the exact version they evaluated.
pub async fn publish_challenge_version(
    pool: &PgPool,
    challenge_id: &str,
    bundle_path: &str,
    statement_path: &str,
    spec: &ChallengeBundleSpec,
    title: &str,
    summary: &str,
) -> Result<CreateChallengeVersionResponse> {
    let version_id = format!("{}:{}", challenge_id, spec.challenge_version);
    let spec_json = serde_json::to_value(spec).map_err(|e| AppError::Internal(e.to_string()))?;

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE challenge_versions pv
        SET status = 'superseded'
        FROM challenges p
        WHERE p.id = $1
          AND p.current_version_id = pv.id
          AND pv.id <> $2
          AND pv.status = 'published'
        "#,
    )
    .bind(challenge_id)
    .bind(&version_id)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query(
        r#"
        WITH upserted_version AS (
            INSERT INTO challenge_versions (
                id, challenge_id, version, bundle_path, statement_path, spec_json, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'published')
            ON CONFLICT (challenge_id, version) DO UPDATE
            SET bundle_path = EXCLUDED.bundle_path,
                statement_path = EXCLUDED.statement_path,
                spec_json = EXCLUDED.spec_json,
                status = 'published'
            RETURNING id, challenge_id, version, bundle_path, statement_path
        )
            UPDATE challenges p
            SET title = $7,
                summary = $8,
                status = 'active',
                current_version_id = v.id,
                updated_at = NOW()
            FROM upserted_version v
        WHERE p.id = v.challenge_id
        RETURNING
            p.id AS challenge_id,
            p.slug,
            p.title,
            v.id AS version_id,
            v.version,
            v.bundle_path,
            v.statement_path
        "#,
    )
    .bind(&version_id)
    .bind(challenge_id)
    .bind(&spec.challenge_version)
    .bind(bundle_path)
    .bind(statement_path)
    .bind(&spec_json)
    .bind(title)
    .bind(summary)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(CreateChallengeVersionResponse {
        challenge_id: row.try_get("challenge_id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        version_id: row.try_get("version_id")?,
        version: row.try_get("version")?,
        bundle_path: row.try_get("bundle_path")?,
        statement_path: row.try_get("statement_path")?,
    })
}

/// Archive a challenge shell while preserving versions, private assets, and
/// historical solution submissions.
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

/// List active challenges with their latest published version.
pub async fn list_published_challenges(pool: &PgPool) -> Result<Vec<ChallengeListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            p.id AS challenge_id,
            p.slug,
            p.title,
            p.summary,
            pv.id AS version_id,
            pv.version
        FROM challenges p
        JOIN challenge_versions pv ON pv.id = p.current_version_id
        WHERE p.status = 'active'
          AND pv.status = 'published'
        ORDER BY p.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(ChallengeListItemDto {
                id: r.try_get("challenge_id")?,
                slug: r.try_get("slug")?,
                title: r.try_get("title")?,
                summary: r.try_get("summary")?,
                current_version: crate::models::CurrentVersionDto {
                    id: r.try_get("version_id")?,
                    version: r.try_get("version")?,
                },
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Fetch one active challenge by id or slug with its latest published version.
pub async fn get_published_challenge(
    pool: &PgPool,
    challenge_id_or_slug: &str,
) -> Result<Option<ChallengeVersionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS challenge_id,
            p.slug,
            p.title,
            p.summary,
            pv.id AS version_id,
            pv.version,
            pv.bundle_path,
            pv.statement_path,
            pv.spec_json
        FROM challenges p
        JOIN challenge_versions pv ON pv.id = p.current_version_id
        WHERE p.status = 'active'
          AND pv.status = 'published'
          AND (p.id = $1 OR p.slug = $1)
        LIMIT 1
        "#,
    )
    .bind(challenge_id_or_slug)
    .fetch_optional(pool)
    .await?;

    row.map(|r| {
        Ok(ChallengeVersionRecord {
            challenge_id: r.try_get("challenge_id")?,
            slug: r.try_get("slug")?,
            title: r.try_get("title")?,
            summary: r.try_get("summary")?,
            challenge_version_id: r.try_get("version_id")?,
            version: r.try_get("version")?,
            bundle_path: r.try_get("bundle_path")?,
            statement_path: r.try_get("statement_path")?,
            spec_json: r.try_get("spec_json")?,
        })
    })
    .transpose()
}

/// Fetch one public challenge detail by id or slug, including archived records
/// that are hidden from default browsing.
pub async fn get_public_challenge(
    pool: &PgPool,
    challenge_id_or_slug: &str,
) -> Result<Option<ChallengeVersionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS challenge_id,
            p.slug,
            p.title,
            p.summary,
            pv.id AS version_id,
            pv.version,
            pv.bundle_path,
            pv.statement_path,
            pv.spec_json
        FROM challenges p
        JOIN challenge_versions pv ON pv.id = p.current_version_id
        WHERE p.status IN ('active', 'archived')
          AND pv.status IN ('published', 'archived')
          AND (p.id = $1 OR p.slug = $1)
        LIMIT 1
        "#,
    )
    .bind(challenge_id_or_slug)
    .fetch_optional(pool)
    .await?;

    row.map(row_to_challenge_version_record).transpose()
}

fn row_to_challenge_version_record(r: sqlx::postgres::PgRow) -> Result<ChallengeVersionRecord> {
    Ok(ChallengeVersionRecord {
        challenge_id: r.try_get("challenge_id")?,
        slug: r.try_get("slug")?,
        title: r.try_get("title")?,
        summary: r.try_get("summary")?,
        challenge_version_id: r.try_get("version_id")?,
        version: r.try_get("version")?,
        bundle_path: r.try_get("bundle_path")?,
        statement_path: r.try_get("statement_path")?,
        spec_json: r.try_get("spec_json")?,
    })
}
