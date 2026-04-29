//! Problem shell and published version queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::problem::{CreateProblemVersionResponse, ProblemBundleSpec, ProblemListItemDto};

/// Latest published problem version joined with problem metadata.
#[derive(Debug, Clone)]
pub struct ProblemVersionRecord {
    pub problem_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub problem_version_id: String,
    pub version: String,
    pub bundle_path: String,
    pub statement_path: String,
    pub spec_json: Value,
}

/// Create or update the problem shell that versions attach to.
pub async fn create_or_update_problem(
    pool: &PgPool,
    id: &str,
    slug: &str,
    title: &str,
    description: &str,
) -> Result<crate::models::problem::ProblemAdminResponse> {
    let row = sqlx::query(
        r#"
        INSERT INTO problems (id, slug, title, description, status)
        VALUES ($1, $2, $3, $4, 'active')
        ON CONFLICT (id) DO UPDATE
        SET slug = EXCLUDED.slug,
            title = EXCLUDED.title,
            description = EXCLUDED.description,
            status = 'active',
            updated_at = NOW()
        RETURNING id, slug, title, description, status, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(slug)
    .bind(title)
    .bind(description)
    .fetch_one(pool)
    .await?;

    Ok(crate::models::problem::ProblemAdminResponse {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

/// Publish a validated bundle as the current problem version.
pub async fn publish_problem_version(
    pool: &PgPool,
    problem_id: &str,
    bundle_path: &str,
    statement_path: &str,
    spec: &ProblemBundleSpec,
    title: &str,
    description: &str,
) -> Result<CreateProblemVersionResponse> {
    let version_id = format!("{}:{}", problem_id, spec.problem_version);
    let spec_json = serde_json::to_value(spec).map_err(|e| AppError::Internal(e.to_string()))?;

    let row = sqlx::query(
        r#"
        WITH upserted_version AS (
            INSERT INTO problem_versions (
                id, problem_id, version, bundle_path, statement_path, spec_json, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'published')
            ON CONFLICT (problem_id, version) DO UPDATE
            SET bundle_path = EXCLUDED.bundle_path,
                statement_path = EXCLUDED.statement_path,
                spec_json = EXCLUDED.spec_json,
                status = 'published'
            RETURNING id, problem_id, version, bundle_path, statement_path
        )
        UPDATE problems p
        SET title = $7,
            description = CASE WHEN p.description = '' THEN $8 ELSE p.description END,
            status = 'active',
            updated_at = NOW()
        FROM upserted_version v
        WHERE p.id = v.problem_id
        RETURNING
            p.id AS problem_id,
            p.slug,
            p.title,
            v.id AS version_id,
            v.version,
            v.bundle_path,
            v.statement_path
        "#,
    )
    .bind(&version_id)
    .bind(problem_id)
    .bind(&spec.problem_version)
    .bind(bundle_path)
    .bind(statement_path)
    .bind(&spec_json)
    .bind(title)
    .bind(description)
    .fetch_one(pool)
    .await?;

    Ok(CreateProblemVersionResponse {
        problem_id: row.try_get("problem_id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        version_id: row.try_get("version_id")?,
        version: row.try_get("version")?,
        bundle_path: row.try_get("bundle_path")?,
        statement_path: row.try_get("statement_path")?,
    })
}

/// List active problems with their latest published version.
pub async fn list_published_problems(pool: &PgPool) -> Result<Vec<ProblemListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            p.id AS problem_id,
            p.slug,
            p.title,
            p.description,
            pv.id AS version_id,
            pv.version
        FROM problems p
        JOIN LATERAL (
            SELECT id, version
            FROM problem_versions
            WHERE problem_id = p.id
              AND status = 'published'
            ORDER BY created_at DESC
            LIMIT 1
        ) pv ON TRUE
        WHERE p.status = 'active'
        ORDER BY p.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(ProblemListItemDto {
                id: r.try_get("problem_id")?,
                slug: r.try_get("slug")?,
                title: r.try_get("title")?,
                description: r.try_get("description")?,
                current_version: crate::models::CurrentVersionDto {
                    id: r.try_get("version_id")?,
                    version: r.try_get("version")?,
                },
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Fetch one active problem by id or slug with its latest published version.
pub async fn get_published_problem(
    pool: &PgPool,
    problem_id_or_slug: &str,
) -> Result<Option<ProblemVersionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS problem_id,
            p.slug,
            p.title,
            p.description,
            pv.id AS version_id,
            pv.version,
            pv.bundle_path,
            pv.statement_path,
            pv.spec_json
        FROM problems p
        JOIN LATERAL (
            SELECT id, version, bundle_path, statement_path, spec_json
            FROM problem_versions
            WHERE problem_id = p.id
              AND status = 'published'
            ORDER BY created_at DESC
            LIMIT 1
        ) pv ON TRUE
        WHERE p.status = 'active'
          AND (p.id = $1 OR p.slug = $1)
        LIMIT 1
        "#,
    )
    .bind(problem_id_or_slug)
    .fetch_optional(pool)
    .await?;

    row.map(|r| {
        Ok(ProblemVersionRecord {
            problem_id: r.try_get("problem_id")?,
            slug: r.try_get("slug")?,
            title: r.try_get("title")?,
            description: r.try_get("description")?,
            problem_version_id: r.try_get("version_id")?,
            version: r.try_get("version")?,
            bundle_path: r.try_get("bundle_path")?,
            statement_path: r.try_get("statement_path")?,
            spec_json: r.try_get("spec_json")?,
        })
    })
    .transpose()
}
