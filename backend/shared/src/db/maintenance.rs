//! Maintenance queries used by server startup and worker liveness.

use sqlx::PgPool;

use crate::error::{AppError, Result};

/// JSON payload stored with each service heartbeat.
///
/// Optional fields are omitted to keep the admin-facing heartbeat document
/// compact and compatible with the relaxed JSON shape used elsewhere.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HeartbeatPayload {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submission_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_completed_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failed_job_id: Option<String>,
}

/// Insert or refresh the latest heartbeat for a named service instance.
pub async fn upsert_service_heartbeat(
    pool: &PgPool,
    service_name: &str,
    payload: &HeartbeatPayload,
) -> Result<()> {
    let payload_json =
        serde_json::to_value(payload).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO service_heartbeats (service_name, last_seen_at, payload)
        VALUES ($1, NOW(), $2)
        ON CONFLICT (service_name)
        DO UPDATE SET last_seen_at = EXCLUDED.last_seen_at, payload = EXCLUDED.payload
        "#,
    )
    .bind(service_name)
    .bind(&payload_json)
    .execute(pool)
    .await?;

    Ok(())
}

/// Seed or refresh published problems by scanning a bundle root.
///
/// Each immediate child directory may contain one or more version directories.
/// Directories without `spec.json` are ignored so local notes or partial bundles
/// do not block startup.
pub async fn ensure_problems_seeded_from_root(pool: &PgPool, problems_root: &str) -> Result<usize> {
    tokio::fs::create_dir_all(problems_root).await?;
    let mut entries = tokio::fs::read_dir(problems_root).await?;
    let mut synced = 0usize;

    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let slug_root = entry.path();
        let mut versions = tokio::fs::read_dir(&slug_root).await?;

        while let Some(v_entry) = versions.next_entry().await? {
            if !v_entry.file_type().await?.is_dir() {
                continue;
            }
            let bundle_dir = v_entry.path();
            let spec_path = bundle_dir.join("spec.json");
            if !spec_path.exists() {
                continue;
            }

            let spec = crate::problem_bundle::read_problem_bundle_spec(&bundle_dir).await?;
            let statement_path = bundle_dir.join("statement.md");
            let description =
                crate::problem_bundle::extract_problem_description(&statement_path).await?;
            let problem_id = &spec.problem_id;
            let version_id = format!("{}:{}", problem_id, spec.problem_version);

            sqlx::query(
                r#"
                INSERT INTO problems (id, slug, title, description, status)
                VALUES ($1, $2, $3, $4, 'active')
                ON CONFLICT (id) DO UPDATE
                SET slug = EXCLUDED.slug,
                    title = EXCLUDED.title,
                    description = CASE WHEN problems.description = '' THEN EXCLUDED.description ELSE problems.description END,
                    status = 'active',
                    updated_at = NOW()
                "#
            )
            .bind(problem_id)
            .bind(problem_id)
            .bind(&spec.problem_title)
            .bind(&description)
            .execute(pool)
            .await?;

            let spec_json =
                serde_json::to_value(&spec).map_err(|e| AppError::Internal(e.to_string()))?;

            sqlx::query(
                r#"
                INSERT INTO problem_versions (id, problem_id, version, bundle_path, statement_path, spec_json, status)
                VALUES ($1, $2, $3, $4, $5, $6, 'published')
                ON CONFLICT (problem_id, version) DO UPDATE
                SET bundle_path = EXCLUDED.bundle_path,
                    statement_path = EXCLUDED.statement_path,
                    spec_json = EXCLUDED.spec_json,
                    status = 'published'
                "#
            )
            .bind(&version_id)
            .bind(problem_id)
            .bind(&spec.problem_version)
            .bind(bundle_dir.to_string_lossy().as_ref())
            .bind(statement_path.to_string_lossy().as_ref())
            .bind(&spec_json)
            .execute(pool)
            .await?;

            synced += 1;
        }
    }

    Ok(synced)
}

/// Return stale running jobs to the queue so another worker can claim them.
pub async fn reap_stuck_jobs(pool: &PgPool, timeout_minutes: i32) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'queued', worker_id = NULL, claimed_at = NULL
        WHERE status = 'running'
          AND claimed_at < NOW() - INTERVAL '1 minute' * $1
        "#,
    )
    .bind(timeout_minutes)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
