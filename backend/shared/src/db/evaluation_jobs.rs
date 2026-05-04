use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::evaluation::{EvaluationJobPayload, ScoringMode};

use super::evaluation_policy::ensure_challenge_supports_eval_type;

/// Claimed or queued evaluation job with parsed runner payload.
#[derive(Debug, Clone)]
pub struct EvaluationJobRecord {
    pub id: String,
    pub solution_submission_id: String,
    pub challenge_id: String,
    pub challenge_version_id: String,
    pub benchmark_target_id: String,
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
        RETURNING j.id, j.solution_submission_id, j.challenge_id, j.challenge_version_id, j.benchmark_target_id, j.eval_type, j.status, j.attempt_count, j.payload_json
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
    let solution_submission_id: String = r.try_get("solution_submission_id")?;

    sqlx::query(
        "UPDATE solution_submissions SET status = 'running', updated_at = NOW() WHERE id = $1",
    )
    .bind(&solution_submission_id)
    .execute(&mut *tx)
    .await?;

    let payload: EvaluationJobPayload = serde_json::from_value(r.try_get("payload_json")?)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tx.commit().await?;

    Ok(Some(EvaluationJobRecord {
        id: r.try_get("id")?,
        solution_submission_id,
        challenge_id: r.try_get("challenge_id")?,
        challenge_version_id: r.try_get("challenge_version_id")?,
        benchmark_target_id: r.try_get("benchmark_target_id")?,
        eval_type,
        status: r.try_get("status")?,
        attempt_count: r.try_get("attempt_count")?,
        payload,
    }))
}

/// Refresh a running job lease owned by one worker.
pub async fn refresh_evaluation_job_claim(
    pool: &PgPool,
    job_id: &str,
    worker_id: &str,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET claimed_at = NOW()
        WHERE id = $1
          AND worker_id = $2
          AND status = 'running'
        "#,
    )
    .bind(job_id)
    .bind(worker_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Input for queueing a validation or official re-run.
#[derive(Debug, Clone)]
pub struct QueueEvaluationJobInput {
    pub job_id: String,
    pub solution_submission_id: String,
    pub eval_type: ScoringMode,
}

/// Queue an evaluation job for an existing solution submission.
///
/// Official jobs are rejected when the challenge version does not enable private benchmark data.
/// Any queued re-run hides the solution submission until its completion path decides
/// whether the result should become public.
pub async fn queue_evaluation_job(
    pool: &PgPool,
    input: &QueueEvaluationJobInput,
) -> Result<EvaluationJobRecord> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        SELECT s.id, s.challenge_id, s.challenge_version_id, s.benchmark_target_id, s.agent_id, s.artifact_path, s.visible_after_eval,
               pv.bundle_path, pv.spec_json
        FROM solution_submissions s
        JOIN challenge_versions pv ON pv.id = s.challenge_version_id
        WHERE s.id = $1
        LIMIT 1
        "#
    )
    .bind(&input.solution_submission_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AppError::NotFound)?;

    let spec_json: Value = row.try_get("spec_json")?;
    let spec: ChallengeBundleSpec =
        serde_json::from_value(spec_json).map_err(|e| AppError::Internal(e.to_string()))?;

    let benchmark_target_id: String = row.try_get("benchmark_target_id")?;
    ensure_challenge_supports_eval_type(&spec, &benchmark_target_id, input.eval_type)?;

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_path: row.try_get("artifact_path")?,
        bundle_path: row.try_get("bundle_path")?,
        challenge_id: row.try_get("challenge_id")?,
        challenge_version_id: row.try_get("challenge_version_id")?,
        benchmark_target_id: benchmark_target_id.clone(),
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let eval_type_str = input.eval_type.as_str();
    let priority = if input.eval_type == ScoringMode::Official {
        10
    } else {
        0
    };

    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (id, solution_submission_id, challenge_id, challenge_version_id, benchmark_target_id, eval_type, status, priority, payload_json)
        VALUES ($1, $2, $3, $4, $5, $6, 'queued', $7, $8)
        "#
    )
    .bind(&input.job_id)
    .bind(row.try_get::<String, _>("id")?)
    .bind(row.try_get::<String, _>("challenge_id")?)
    .bind(row.try_get::<String, _>("challenge_version_id")?)
    .bind(&benchmark_target_id)
    .bind(eval_type_str)
    .bind(priority)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(map_active_job_conflict)?;

    sqlx::query(
        "UPDATE solution_submissions SET status = 'queued', visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1"
    )
    .bind(row.try_get::<String, _>("id")?)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(EvaluationJobRecord {
        id: input.job_id.clone(),
        solution_submission_id: row.try_get("id")?,
        challenge_id: row.try_get("challenge_id")?,
        challenge_version_id: row.try_get("challenge_version_id")?,
        benchmark_target_id,
        eval_type: input.eval_type,
        status: "queued".to_string(),
        attempt_count: 0,
        payload: serde_json::from_value(payload).map_err(|e| AppError::Internal(e.to_string()))?,
    })
}

fn map_active_job_conflict(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::Database(db_err)
            if db_err.constraint().is_some_and(|constraint| {
                constraint == "idx_evaluation_jobs_one_active_per_submission_mode"
            }) =>
        {
            AppError::Conflict
        }
        other => AppError::Database(other),
    }
}

/// Count queued or running jobs for one evaluation type.
pub async fn count_active_evaluation_jobs(pool: &PgPool, eval_type: ScoringMode) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM evaluation_jobs
        WHERE eval_type = $1
          AND status IN ('queued', 'running')
        "#,
    )
    .bind(eval_type.as_str())
    .fetch_one(pool)
    .await?;

    Ok(count)
}
