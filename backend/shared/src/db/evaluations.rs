use sqlx::PgPool;

use crate::error::{AppError, Result};
use crate::models::evaluation::{
    EvaluationStatus, MetricValue, PublicCaseResult, RunMetricResult, ScoreSummary, ScoringMode,
};
use crate::models::ids::{EvaluationId, EvaluationJobId, SolutionSubmissionId};
use crate::models::names::TargetName;
use crate::storage::StorageKey;

use super::leaderboard::{
    update_official_score_for_solution_submission_tx,
    upsert_leaderboard_entry_for_solution_submission_tx,
};

/// Input for creating the evaluation row associated with a claimed job.
#[derive(Debug, Clone)]
pub struct MarkEvaluationStartedInput {
    pub evaluation_id: EvaluationId,
    pub solution_submission_id: SolutionSubmissionId,
    pub job_id: EvaluationJobId,
    pub target: TargetName,
    pub eval_type: ScoringMode,
}

/// Mark a job's evaluation as running.
pub async fn mark_evaluation_started(
    pool: &PgPool,
    input: &MarkEvaluationStartedInput,
) -> Result<bool> {
    let eval_type_str = input.eval_type.as_str();

    let result = sqlx::query(
        r#"
        INSERT INTO evaluations (id, solution_submission_id, job_id, target, eval_type, status, started_at)
        VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 'running', NOW())
        ON CONFLICT (job_id) DO NOTHING
        "#,
    )
    .bind(input.evaluation_id.as_str())
    .bind(input.solution_submission_id.as_str())
    .bind(input.job_id.as_str())
    .bind(input.target.as_str())
    .bind(eval_type_str)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Validated runner result prepared for persistence.
#[derive(Debug, Clone)]
pub struct PersistedEvaluationResult {
    pub solution_submission_id: SolutionSubmissionId,
    pub job_id: EvaluationJobId,
    pub worker_id: String,
    pub claim_attempt_count: i32,
    pub target: TargetName,
    pub eval_type: ScoringMode,
    pub status: EvaluationStatus,
    pub primary_score: Option<f64>,
    pub rank_score: Option<f64>,
    pub aggregate_metrics: Vec<MetricValue>,
    pub run_metrics: Vec<RunMetricResult>,
    pub public_results: Vec<PublicCaseResult>,
    pub validation_summary: Option<ScoreSummary>,
    pub official_summary: Option<ScoreSummary>,
    pub log_key: Option<StorageKey>,
    pub last_error: Option<String>,
}

/// Persist a finished evaluation and update dependent solution submission and leaderboard state.
pub async fn mark_evaluation_finished(
    pool: &PgPool,
    result: &PersistedEvaluationResult,
) -> Result<bool> {
    let mut tx = pool.begin().await?;

    let public_results_json = serde_json::to_value(&result.public_results)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let validation_summary_json = serde_json::to_value(&result.validation_summary)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let official_json = serde_json::to_value(&result.official_summary)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let aggregate_metrics_json = serde_json::to_value(&result.aggregate_metrics)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let run_metrics_json =
        serde_json::to_value(&result.run_metrics).map_err(|e| AppError::Internal(e.to_string()))?;
    let status_str = match result.status {
        EvaluationStatus::Completed => "completed",
        _ => "failed",
    };

    let job_update = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = $2, finished_at = NOW(), last_error = $3
        WHERE id = $1::uuid
          AND status = 'running'
          AND worker_id = $4
          AND attempt_count = $5
        "#,
    )
    .bind(result.job_id.as_str())
    .bind(status_str)
    .bind(&result.last_error)
    .bind(&result.worker_id)
    .bind(result.claim_attempt_count)
    .execute(&mut *tx)
    .await?;

    if job_update.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(false);
    }

    let evaluation_update = sqlx::query(
        r#"
        UPDATE evaluations
        SET status = $2, primary_score = $3, rank_score = $4,
            aggregate_metrics_json = $5, run_metrics_json = $6,
            public_results_json = $7, validation_summary_json = $8,
            official_summary_json = $9, log_key = $10, finished_at = NOW()
        WHERE job_id = $1::uuid
          AND status = 'running'
        "#,
    )
    .bind(result.job_id.as_str())
    .bind(status_str)
    .bind(result.primary_score)
    .bind(result.rank_score)
    .bind(&aggregate_metrics_json)
    .bind(&run_metrics_json)
    .bind(&public_results_json)
    .bind(&validation_summary_json)
    .bind(&official_json)
    .bind(result.log_key.as_ref().map(StorageKey::as_str))
    .execute(&mut *tx)
    .await?;

    if evaluation_update.rows_affected() != 1 {
        return Err(AppError::Conflict);
    }

    match result.eval_type {
        ScoringMode::Validation => {
            let sub_status = if result.status == EvaluationStatus::Completed {
                "completed"
            } else {
                "failed"
            };
            sqlx::query(
                "UPDATE solution_submissions SET status = $2, visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1::uuid"
            )
            .bind(result.solution_submission_id.as_str())
            .bind(sub_status)
            .execute(&mut *tx)
            .await?;
        }
        ScoringMode::Official => {
            let visible = result.status == EvaluationStatus::Completed;
            let sub_status = if visible { "completed" } else { "failed" };
            sqlx::query(
                "UPDATE solution_submissions SET status = $2, visible_after_eval = $3, updated_at = NOW() WHERE id = $1::uuid"
            )
            .bind(result.solution_submission_id.as_str())
            .bind(sub_status)
            .bind(visible)
            .execute(&mut *tx)
            .await?;

            if result.status == EvaluationStatus::Completed
                && let Some(rank_score) = result.rank_score
            {
                let became_best = upsert_leaderboard_entry_for_solution_submission_tx(
                    &mut tx,
                    &result.solution_submission_id,
                    &result.target,
                    rank_score,
                    &result.public_results,
                    &result.aggregate_metrics,
                )
                .await?;
                if became_best {
                    update_official_score_for_solution_submission_tx(
                        &mut tx,
                        &result.solution_submission_id,
                        &result.target,
                        rank_score,
                        &result.aggregate_metrics,
                    )
                    .await?;
                }
            }
        }
    }

    tx.commit().await?;
    Ok(true)
}
