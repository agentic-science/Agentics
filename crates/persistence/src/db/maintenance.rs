//! Maintenance queries used by server startup and worker liveness.

use sqlx::{PgPool, Row};

use super::ids::{solution_submission_id_from_row, uuid_string_from_row};
use super::leaderboard::repair_leaderboard_entry_for_solution_submission_tx;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::request::AdminServiceHeartbeatDto;
use agentics_error::{Result, ServiceError};

/// JSON payload stored with each service heartbeat.
///
/// Optional fields are omitted to keep the admin-facing heartbeat document
/// compact and compatible with the relaxed JSON shape used elsewhere.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HeartbeatPayload {
    pub status: String,
    pub accelerators: Vec<String>,
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
        serde_json::to_value(payload).map_err(|e| ServiceError::Internal(e.to_string()))?;

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

    let staged_jobs = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'failed',
            finished_at = NOW(),
            last_error = 'staged job was not promoted before timeout',
            worker_id = NULL,
            claimed_at = NULL
        WHERE status = 'staged'
          AND scheduled_at < NOW() - INTERVAL '1 minute' * $1
        RETURNING id, solution_submission_id, eval_type
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &staged_jobs {
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
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

    let requeued_jobs = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'queued', worker_id = NULL, claimed_at = NULL
        WHERE status = 'running'
          AND claimed_at < NOW() - INTERVAL '1 minute' * $1
          AND attempt_count < max_attempts
        RETURNING id, solution_submission_id, eval_type
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &requeued_jobs {
        let job_id = uuid_string_from_row(row, "id")?;
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
        let eval_type = eval_type_from_row(row, "eval_type")?;
        sqlx::query("DELETE FROM evaluations WHERE job_id = $1::uuid AND status = 'running'")
            .bind(&job_id)
            .execute(&mut *tx)
            .await?;
        if preserve_visible_official_result_tx(&mut tx, &solution_submission_id, &job_id, eval_type)
            .await?
        {
            continue;
        }
        let was_visible =
            hide_reaped_solution_submission_tx(&mut tx, &solution_submission_id, "queued").await?;
        if was_visible {
            repair_leaderboard_entry_for_solution_submission_tx(&mut tx, &solution_submission_id)
                .await?;
        }
    }

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
        RETURNING id, solution_submission_id, eval_type
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &failed_jobs {
        let job_id = uuid_string_from_row(row, "id")?;
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
        let eval_type = eval_type_from_row(row, "eval_type")?;
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

        if preserve_visible_official_result_tx(&mut tx, &solution_submission_id, &job_id, eval_type)
            .await?
        {
            continue;
        }
        let was_visible =
            hide_reaped_solution_submission_tx(&mut tx, &solution_submission_id, "failed").await?;
        if was_visible {
            repair_leaderboard_entry_for_solution_submission_tx(&mut tx, &solution_submission_id)
                .await?;
        }
    }

    tx.commit().await?;

    Ok(StaleJobReapResult {
        requeued: u64::try_from(requeued_jobs.len())
            .map_err(|_| ServiceError::Internal("requeued job count overflow".to_string()))?,
        failed: u64::try_from(failed_jobs.len().saturating_add(staged_jobs.len()))
            .map_err(|_| ServiceError::Internal("failed job count overflow".to_string()))?,
    })
}

/// Parse one persisted evaluation type from a maintenance query row.
fn eval_type_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<ScoringMode> {
    let value: String = row.try_get(column)?;
    ScoringMode::from_storage_value(&value)
        .ok_or_else(|| ServiceError::Internal(format!("unknown stored {column} `{value}`")))
}

/// Keep an older completed official result visible while a stale official rerun is repaired.
async fn preserve_visible_official_result_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    stale_job_id: &str,
    eval_type: ScoringMode,
) -> Result<bool> {
    if eval_type != ScoringMode::Official {
        return Ok(false);
    }

    let has_prior_completed_official = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM evaluations
            WHERE solution_submission_id = $1::uuid
              AND eval_type = 'official'
              AND status = 'completed'
              AND job_id <> $2::uuid
        )
        "#,
    )
    .bind(solution_submission_id.as_str())
    .bind(stale_job_id)
    .fetch_one(&mut **tx)
    .await?;
    if !has_prior_completed_official {
        return Ok(false);
    }

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = 'completed',
            visible_after_eval = TRUE,
            updated_at = NOW()
        WHERE id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .execute(&mut **tx)
    .await?;

    Ok(true)
}

/// Apply the original stale-job fallback and report whether public visibility changed.
async fn hide_reaped_solution_submission_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    next_status: &str,
) -> Result<bool> {
    let was_visible = sqlx::query_scalar::<_, bool>(
        "SELECT visible_after_eval FROM solution_submissions WHERE id = $1::uuid FOR UPDATE",
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?
    .unwrap_or(false);

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = $2,
            visible_after_eval = FALSE,
            updated_at = NOW()
        WHERE id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .bind(next_status)
    .execute(&mut **tx)
    .await?;

    Ok(was_visible)
}
