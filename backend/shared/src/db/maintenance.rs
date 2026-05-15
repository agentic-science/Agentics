//! Maintenance queries used by server startup and worker liveness.

use std::path::PathBuf;

use sqlx::PgPool;

use super::ids::{solution_submission_id_from_row, uuid_string_from_row};
use crate::error::{AppError, Result};
use crate::models::ids::{EvaluationJobId, SolutionSubmissionId};
use crate::models::request::AdminServiceHeartbeatDto;

/// JSON payload stored with each service heartbeat.
///
/// Optional fields are omitted to keep the admin-facing heartbeat document
/// compact and compatible with the relaxed JSON shape used elsewhere.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HeartbeatPayload {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<EvaluationJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution_submission_id: Option<SolutionSubmissionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_completed_job_id: Option<EvaluationJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failed_job_id: Option<EvaluationJobId>,
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
/// Each immediate child directory may contain one or more bundle directories.
/// Directories without `spec.json` are ignored so local notes or partial bundles
/// do not block startup.
pub async fn ensure_challenges_seeded_from_root(
    pool: &PgPool,
    challenges_root: &str,
) -> Result<usize> {
    tokio::fs::create_dir_all(challenges_root).await?;
    let mut entries = tokio::fs::read_dir(challenges_root).await?;
    let mut challenge_dirs = Vec::new();
    let mut synced = 0usize;

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            challenge_dirs.push(entry.path());
        }
    }
    challenge_dirs.sort();

    for challenge_root in challenge_dirs {
        let mut bundles = tokio::fs::read_dir(&challenge_root).await?;
        let mut bundle_dirs: Vec<PathBuf> = Vec::new();

        while let Some(bundle_entry) = bundles.next_entry().await? {
            if !bundle_entry.file_type().await?.is_dir() {
                continue;
            }
            let bundle_dir = bundle_entry.path();
            if tokio::fs::try_exists(bundle_dir.join("spec.json")).await? {
                bundle_dirs.push(bundle_dir);
            }
        }
        bundle_dirs.sort();

        for bundle_dir in bundle_dirs {
            crate::challenge_bundle::validate_challenge_bundle(&bundle_dir).await?;
            let spec = crate::challenge_bundle::read_challenge_bundle_spec(&bundle_dir).await?;
            let statement_path = bundle_dir.join("statement.md");
            let managed_bundle_path =
                crate::models::paths::ManagedBundlePath::from_existing_dir(&bundle_dir)?;
            let managed_statement_path =
                crate::models::paths::ManagedStatementPath::from_existing_file(&statement_path)?;
            let challenge_name = &spec.challenge_name;

            if crate::db::publish_challenge(
                pool,
                challenge_name,
                &managed_bundle_path,
                &managed_statement_path,
                &spec,
                &spec.challenge_title,
                &spec.challenge_summary,
            )
            .await
            .is_err()
            {
                sqlx::query(
                    r#"
                    UPDATE challenges
                    SET title = $2,
                        summary = $3,
                        bundle_path = $4,
                        statement_path = $5,
                        spec_json = $6,
                        status = 'active',
                        updated_at = NOW()
                    WHERE name = $1
                    "#,
                )
                .bind(challenge_name.as_str())
                .bind(&spec.challenge_title)
                .bind(&spec.challenge_summary)
                .bind(managed_bundle_path.as_str()?)
                .bind(managed_statement_path.as_str()?)
                .bind(serde_json::to_value(&spec).map_err(|e| AppError::Internal(e.to_string()))?)
                .execute(pool)
                .await?;
            }

            synced = synced
                .checked_add(1)
                .ok_or_else(|| AppError::Internal("challenge sync count overflow".to_string()))?;
        }
    }

    Ok(synced)
}

/// Summary of stale job recovery work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaleJobReapResult {
    pub requeued: u64,
    pub failed: u64,
}

/// Recover running jobs whose worker lease has expired.
///
/// Jobs with attempts remaining return to the queue. Jobs that have exhausted
/// their retry budget move to `failed` together with their associated
/// evaluation and solution submission.
pub async fn reap_stuck_jobs(pool: &PgPool, timeout_minutes: i32) -> Result<StaleJobReapResult> {
    let mut tx = pool.begin().await?;

    let requeued = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'queued', worker_id = NULL, claimed_at = NULL
        WHERE status = 'running'
          AND claimed_at < NOW() - INTERVAL '1 minute' * $1
          AND attempt_count < max_attempts
        "#,
    )
    .bind(timeout_minutes)
    .execute(&mut *tx)
    .await?;

    let failed_jobs = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'failed',
            finished_at = NOW(),
            last_error = 'worker lease expired after max attempts',
            worker_id = NULL,
            claimed_at = NULL
        WHERE status = 'running'
          AND claimed_at < NOW() - INTERVAL '1 minute' * $1
          AND attempt_count >= max_attempts
        RETURNING id, solution_submission_id
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &failed_jobs {
        let job_id = uuid_string_from_row(row, "id")?;
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
        sqlx::query(
            r#"
            UPDATE evaluations
            SET status = 'failed',
                finished_at = NOW()
            WHERE job_id = $1::uuid
              AND status = 'running'
            "#,
        )
        .bind(&job_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE solution_submissions
            SET status = 'failed',
                visible_after_eval = FALSE,
                updated_at = NOW()
            WHERE id = $1::uuid
        "#,
        )
        .bind(solution_submission_id.as_str())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(StaleJobReapResult {
        requeued: requeued.rows_affected(),
        failed: failed_jobs.len() as u64,
    })
}
