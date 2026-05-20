use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::evaluation::{EvaluationJobPayload, ScoringMode};
use crate::models::ids::{EvaluationJobId, SolutionSubmissionId};
use crate::models::names::{ChallengeName, TargetName};
use crate::models::paths::ManagedBundlePath;

use super::evaluation_policy::ensure_challenge_supports_eval_type_tx;
use super::ids::{
    agent_id_from_row, challenge_name_from_row, evaluation_job_id_from_row,
    solution_submission_id_from_row, target_from_row,
};
use super::leaderboard::repair_leaderboard_entry_for_solution_submission_tx;

/// Claimed or queued evaluation job with parsed runner payload.
#[derive(Debug, Clone)]
pub struct EvaluationJobRecord {
    pub id: EvaluationJobId,
    pub solution_submission_id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub eval_type: ScoringMode,
    pub status: String,
    pub attempt_count: i32,
    pub payload: EvaluationJobPayload,
}

/// Claim the next queued job using `FOR UPDATE SKIP LOCKED`.
///
/// Claimed jobs move their solution submission into `running` so public visibility can be
/// controlled consistently by the completion path for each evaluation mode.
pub async fn claim_next_evaluation_job(
    pool: &PgPool,
    worker_id: &str,
) -> Result<Option<EvaluationJobRecord>> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        WITH next_job AS (
            SELECT id
            FROM evaluation_jobs
            WHERE status = 'queued'
              AND scheduled_at <= NOW()
              AND attempt_count < max_attempts
            ORDER BY priority DESC, scheduled_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE evaluation_jobs j
        SET status = 'running', claimed_at = NOW(), worker_id = $1, attempt_count = j.attempt_count + 1
        FROM next_job
        WHERE j.id = next_job.id
        RETURNING j.id, j.solution_submission_id, j.challenge_name, j.target, j.eval_type, j.status, j.attempt_count, j.payload_json
        "#
    )
    .bind(worker_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(r) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    let eval_type_raw: String = r.try_get("eval_type")?;
    let eval_type = ScoringMode::from_storage_value(&eval_type_raw).ok_or_else(|| {
        AppError::Internal(format!("unexpected evaluation job type `{eval_type_raw}`"))
    })?;
    let solution_submission_id = solution_submission_id_from_row(&r, "solution_submission_id")?;

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = 'running', updated_at = NOW()
        WHERE id = $1::uuid
          AND visible_after_eval = FALSE
        "#,
    )
    .bind(solution_submission_id.as_str())
    .execute(&mut *tx)
    .await?;

    let payload: EvaluationJobPayload = serde_json::from_value(r.try_get("payload_json")?)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tx.commit().await?;

    Ok(Some(EvaluationJobRecord {
        id: evaluation_job_id_from_row(&r, "id")?,
        solution_submission_id,
        challenge_name: challenge_name_from_row(&r, "challenge_name")?,
        target: target_from_row(&r, "target")?,
        eval_type,
        status: r.try_get("status")?,
        attempt_count: r.try_get("attempt_count")?,
        payload,
    }))
}

/// Refresh a running job lease owned by one worker.
pub async fn refresh_evaluation_job_claim(
    pool: &PgPool,
    job_id: &EvaluationJobId,
    worker_id: &str,
    attempt_count: i32,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET claimed_at = NOW()
        WHERE id = $1::uuid
          AND worker_id = $2
          AND attempt_count = $3
          AND status = 'running'
        "#,
    )
    .bind(job_id.as_str())
    .bind(worker_id)
    .bind(attempt_count)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Requeue a running job when platform capacity is temporarily unavailable.
///
/// Capacity requeues do not consume an evaluation attempt because participant
/// code did not run to completion.
pub async fn requeue_running_evaluation_job_for_capacity(
    pool: &PgPool,
    job_id: &EvaluationJobId,
    worker_id: &str,
    attempt_count: i32,
    last_error: &str,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'queued',
            worker_id = NULL,
            claimed_at = NULL,
            scheduled_at = NOW() + INTERVAL '5 seconds',
            attempt_count = GREATEST(attempt_count - 1, 0),
            last_error = $4
        WHERE id = $1::uuid
          AND status = 'running'
          AND worker_id = $2
          AND attempt_count = $3
        RETURNING solution_submission_id
        "#,
    )
    .bind(job_id.as_str())
    .bind(worker_id)
    .bind(attempt_count)
    .bind(last_error)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(false);
    };

    sqlx::query("DELETE FROM evaluations WHERE job_id = $1::uuid AND status = 'running'")
        .bind(job_id.as_str())
        .execute(&mut *tx)
        .await?;

    let solution_submission_id = solution_submission_id_from_row(&row, "solution_submission_id")?;
    let visible_after_eval = sqlx::query_scalar::<_, bool>(
        "SELECT visible_after_eval FROM solution_submissions WHERE id = $1::uuid FOR UPDATE",
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !visible_after_eval {
        sqlx::query(
            "UPDATE solution_submissions SET status = 'queued', visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1::uuid"
        )
        .bind(solution_submission_id.as_str())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(true)
}

/// Make a staged queued job eligible for worker claiming after its artifact is durable.
pub async fn mark_evaluation_job_ready(pool: &PgPool, job_id: &EvaluationJobId) -> Result<()> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'queued', scheduled_at = NOW()
        WHERE id = $1::uuid
          AND status = 'staged'
        RETURNING solution_submission_id
        "#,
    )
    .bind(job_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        return Err(AppError::Internal(format!(
            "staged evaluation job `{job_id}` is not staged"
        )));
    };
    let solution_submission_id = solution_submission_id_from_row(&row, "solution_submission_id")?;

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = 'queued', updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'pending'
        "#,
    )
    .bind(solution_submission_id.as_str())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Input for queueing a validation or official re-run.
#[derive(Debug, Clone)]
pub struct QueueEvaluationJobInput {
    pub job_id: EvaluationJobId,
    pub solution_submission_id: SolutionSubmissionId,
    pub eval_type: ScoringMode,
    pub max_active_official_jobs: Option<i64>,
}

/// Queue an evaluation job for an existing solution submission.
///
/// Official jobs are rejected when the challenge does not enable private benchmark data.
/// Queued official re-runs preserve an already visible official result until a newer
/// official run succeeds.
pub async fn queue_evaluation_job(
    pool: &PgPool,
    input: &QueueEvaluationJobInput,
) -> Result<EvaluationJobRecord> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        SELECT s.id, s.challenge_name, s.target, s.agent_id::text AS agent_id, s.artifact_key, s.visible_after_eval,
               p.bundle_path, p.spec_json
        FROM solution_submissions s
        JOIN challenges p ON p.name = s.challenge_name
        WHERE s.id = $1::uuid
          AND p.status = 'active'
          AND p.spec_json IS NOT NULL
        LIMIT 1
        FOR UPDATE OF s, p
        "#,
    )
    .bind(input.solution_submission_id.as_str())
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AppError::NotFound)?;
    let was_visible: bool = row.try_get("visible_after_eval")?;

    let spec_json: Value = row.try_get("spec_json")?;
    let spec: ChallengeBundleSpec =
        serde_json::from_value(spec_json).map_err(|e| AppError::Internal(e.to_string()))?;

    let target = target_from_row(&row, "target")?;
    let challenge_name = challenge_name_from_row(&row, "challenge_name")?;
    ensure_challenge_supports_eval_type_tx(
        &mut tx,
        &challenge_name,
        &spec,
        &target,
        input.eval_type,
        &agent_id_from_row(&row, "agent_id")?,
    )
    .await?;
    ensure_no_active_job_for_submission_tx(&mut tx, &input.solution_submission_id).await?;

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_key: storage_key_from_row(&row, "artifact_key")?,
        bundle_path: managed_bundle_path_from_row(&row, "bundle_path")?,
        challenge_name: challenge_name.clone(),
        target: target.clone(),
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let eval_type_str = input.eval_type.as_str();
    let priority = if input.eval_type == ScoringMode::Official {
        if let Some(max_active) = input.max_active_official_jobs {
            lock_quota_scope(&mut tx, "global:official-active").await?;
            let active = count_active_evaluation_jobs_tx(&mut tx, ScoringMode::Official).await?;
            if active >= max_active {
                return Err(AppError::TooManyRequests(format!(
                    "official evaluation queue is full: {active} of {max_active} official jobs are staged, queued, or running"
                )));
            }
        }
        10
    } else {
        0
    };

    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (id, solution_submission_id, challenge_name, target, eval_type, status, priority, payload_json)
        VALUES ($1::uuid, $2::uuid, $3, $4, $5, 'queued', $6, $7)
        "#
    )
    .bind(input.job_id.as_str())
    .bind(input.solution_submission_id.as_str())
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(eval_type_str)
    .bind(priority)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(map_active_job_conflict)?;

    if input.eval_type == ScoringMode::Official && was_visible {
        sqlx::query("UPDATE solution_submissions SET updated_at = NOW() WHERE id = $1::uuid")
            .bind(input.solution_submission_id.as_str())
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            "UPDATE solution_submissions SET status = 'queued', visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1::uuid"
        )
        .bind(input.solution_submission_id.as_str())
        .execute(&mut *tx)
        .await?;
        repair_leaderboard_entry_for_solution_submission_tx(&mut tx, &input.solution_submission_id)
            .await?;
    }

    tx.commit().await?;

    Ok(EvaluationJobRecord {
        id: input.job_id.clone(),
        solution_submission_id: solution_submission_id_from_row(&row, "id")?,
        challenge_name,
        target,
        eval_type: input.eval_type,
        status: "queued".to_string(),
        attempt_count: 0,
        payload: serde_json::from_value(payload).map_err(|e| AppError::Internal(e.to_string()))?,
    })
}

/// Reads storage key from a database row and validates its domain shape.
fn storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<crate::storage::StorageKey> {
    let value: String = row.try_get(column)?;
    crate::storage::StorageKey::try_new(&value)
        .map_err(|e| AppError::Internal(format!("stored invalid storage key in `{column}`: {e}")))
}

/// Reads managed bundle path from a database row and validates its domain shape.
fn managed_bundle_path_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ManagedBundlePath> {
    let value: String = row.try_get(column)?;
    ManagedBundlePath::from_existing_dir(value)
        .map_err(|e| AppError::Internal(format!("stored invalid {column}: {e}")))
}

/// Handles map active job conflict for this module.
fn map_active_job_conflict(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::Database(db_err)
            if db_err.constraint().is_some_and(|constraint| {
                constraint == "idx_evaluation_jobs_one_active_per_submission"
                    || constraint == "idx_evaluation_jobs_one_active_per_submission_mode"
            }) =>
        {
            AppError::Conflict
        }
        other => AppError::Database(other),
    }
}

/// Serialize active official-capacity admission through a database lock row.
async fn lock_quota_scope(tx: &mut Transaction<'_, Postgres>, scope: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO quota_admission_locks (scope)
        VALUES ($1)
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .bind(scope)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        SELECT scope
        FROM quota_admission_locks
        WHERE scope = $1
        FOR UPDATE
        "#,
    )
    .bind(scope)
    .fetch_one(&mut **tx)
    .await?;

    Ok(())
}

/// Count active capacity reservations for one evaluation type inside a transaction.
async fn count_active_evaluation_jobs_tx(
    tx: &mut Transaction<'_, Postgres>,
    eval_type: ScoringMode,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM evaluation_jobs
        WHERE eval_type = $1
          AND status IN ('staged', 'queued', 'running')
        "#,
    )
    .bind(eval_type.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Reject queueing when any evaluation mode already reserves this submission.
async fn ensure_no_active_job_for_submission_tx(
    tx: &mut Transaction<'_, Postgres>,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    let active = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM evaluation_jobs
            WHERE solution_submission_id = $1::uuid
              AND status IN ('staged', 'queued', 'running')
        )
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(&mut **tx)
    .await?;
    if active {
        return Err(AppError::Conflict);
    }
    Ok(())
}

/// Count jobs that reserve active capacity for one evaluation type.
pub async fn count_active_evaluation_jobs(pool: &PgPool, eval_type: ScoringMode) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM evaluation_jobs
        WHERE eval_type = $1
          AND status IN ('staged', 'queued', 'running')
        "#,
    )
    .bind(eval_type.as_str())
    .fetch_one(pool)
    .await?;

    Ok(count)
}
