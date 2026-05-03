//! Maintenance queries used by server startup and worker liveness.

use sqlx::PgPool;

use crate::error::{AppError, Result};
use crate::models::request::AdminServiceHeartbeatDto;

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
    pub solution_submission_id: Option<String>,
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

/// List latest service heartbeats for the admin operations console.
pub async fn list_service_heartbeats(pool: &PgPool) -> Result<Vec<AdminServiceHeartbeatDto>> {
    let rows = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>, serde_json::Value)>(
        r#"
        SELECT service_name, last_seen_at, payload
        FROM service_heartbeats
        ORDER BY last_seen_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(service_name, last_seen_at, payload)| AdminServiceHeartbeatDto {
                service_name,
                last_seen_at: last_seen_at.to_rfc3339(),
                payload,
            },
        )
        .collect())
}

/// Seed or refresh published challenges by scanning a bundle root.
///
/// Each immediate child directory may contain one or more version directories.
/// Directories without `spec.json` are ignored so local notes or partial bundles
/// do not block startup.
pub async fn ensure_challenges_seeded_from_root(
    pool: &PgPool,
    challenges_root: &str,
) -> Result<usize> {
    tokio::fs::create_dir_all(challenges_root).await?;
    let mut entries = tokio::fs::read_dir(challenges_root).await?;
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

            crate::challenge_bundle::validate_challenge_bundle(&bundle_dir).await?;
            let spec = crate::challenge_bundle::read_challenge_bundle_spec(&bundle_dir).await?;
            let statement_path = bundle_dir.join("statement.md");
            let description =
                crate::challenge_bundle::extract_challenge_description(&statement_path).await?;
            let challenge_id = &spec.challenge_id;
            let version_id = format!("{}:{}", challenge_id, spec.challenge_version);

            sqlx::query(
                r#"
                INSERT INTO challenges (id, slug, title, description, status)
                VALUES ($1, $2, $3, $4, 'active')
                ON CONFLICT (id) DO UPDATE
                SET slug = EXCLUDED.slug,
                    title = EXCLUDED.title,
                    description = CASE WHEN challenges.description = '' THEN EXCLUDED.description ELSE challenges.description END,
                    status = 'active',
                    updated_at = NOW()
                "#
            )
            .bind(challenge_id)
            .bind(challenge_id)
            .bind(&spec.challenge_title)
            .bind(&description)
            .execute(pool)
            .await?;

            let spec_json =
                serde_json::to_value(&spec).map_err(|e| AppError::Internal(e.to_string()))?;

            sqlx::query(
                r#"
                INSERT INTO challenge_versions (id, challenge_id, version, bundle_path, statement_path, spec_json, status)
                VALUES ($1, $2, $3, $4, $5, $6, 'published')
                ON CONFLICT (challenge_id, version) DO UPDATE
                SET bundle_path = EXCLUDED.bundle_path,
                    statement_path = EXCLUDED.statement_path,
                    spec_json = EXCLUDED.spec_json,
                    status = 'published'
                "#
            )
            .bind(&version_id)
            .bind(challenge_id)
            .bind(&spec.challenge_version)
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
