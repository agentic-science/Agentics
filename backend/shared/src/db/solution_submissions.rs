use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::evaluation::{
    EvaluationDto, EvaluationJobPayload, EvaluationStatus, MetricValue, PublicCaseResult,
    RunMetricResult, ScoringMode,
};
use crate::models::request::AdminSolutionSubmissionListItemDto;
use crate::models::request::PublicSolutionSubmissionListItemDto;

use super::challenges::get_published_challenge;
use super::evaluation_policy::ensure_challenge_supports_eval_type;
use super::json::decode_optional_json;

/// Input for creating a solution submission and its initial evaluation job.
#[derive(Debug, Clone)]
pub struct CreateSolutionSubmissionInput {
    pub solution_submission_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub challenge_id: String,
    pub artifact_path: String,
    pub eval_type: ScoringMode,
    pub explanation: String,
    pub parent_solution_submission_id: Option<String>,
    pub credit_text: String,
}

/// Solution submission row with optional joined evaluation and job metadata.
#[derive(Debug, Clone)]
pub struct SolutionSubmissionRecord {
    pub id: String,
    pub challenge_id: String,
    pub challenge_version_id: String,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub challenge_title: Option<String>,
    pub artifact_path: String,
    pub language: String,
    pub status: String,
    pub explanation: String,
    pub parent_solution_submission_id: Option<String>,
    pub credit_text: String,
    pub visible_after_eval: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub evaluation_job_id: Option<String>,
    pub evaluation_job_status: Option<String>,
    pub evaluation: Option<EvaluationDto>,
    pub validation_evaluation: Option<EvaluationDto>,
    pub official_evaluation: Option<EvaluationDto>,
}

/// Create a solution submission and queue its first evaluation atomically.
pub async fn create_solution_submission_with_job(
    pool: &PgPool,
    input: &CreateSolutionSubmissionInput,
) -> Result<SolutionSubmissionRecord> {
    let challenge = get_published_challenge(pool, &input.challenge_id).await?;
    let challenge =
        challenge.ok_or_else(|| AppError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type(&spec, input.eval_type)?;

    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO solution_submissions (
            id, challenge_id, challenge_version_id, agent_id, artifact_path, language,
            status, explanation, parent_solution_submission_id, credit_text, visible_after_eval
        )
        VALUES ($1, $2, $3, $4, $5, 'python', 'queued', $6, $7, $8, FALSE)
        RETURNING
            id, challenge_id, challenge_version_id, agent_id, artifact_path, language,
            status, explanation, parent_solution_submission_id, credit_text, visible_after_eval,
            created_at, updated_at
        "#,
    )
    .bind(&input.solution_submission_id)
    .bind(&challenge.challenge_id)
    .bind(&challenge.challenge_version_id)
    .bind(&input.agent_id)
    .bind(&input.artifact_path)
    .bind(&input.explanation)
    .bind(&input.parent_solution_submission_id)
    .bind(&input.credit_text)
    .fetch_one(&mut *tx)
    .await?;

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_path: input.artifact_path.clone(),
        bundle_path: challenge.bundle_path.clone(),
        challenge_id: challenge.challenge_id.clone(),
        challenge_version_id: challenge.challenge_version_id.clone(),
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let priority = if input.eval_type == ScoringMode::Official {
        10
    } else {
        0
    };

    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (
            id, solution_submission_id, challenge_id, challenge_version_id, eval_type, status, priority, payload_json
        )
        VALUES ($1, $2, $3, $4, $5, 'queued', $6, $7)
        "#,
    )
    .bind(&input.job_id)
    .bind(&input.solution_submission_id)
    .bind(&challenge.challenge_id)
    .bind(&challenge.challenge_version_id)
    .bind(input.eval_type.as_str())
    .bind(priority)
    .bind(&payload)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(SolutionSubmissionRecord {
        id: row.try_get("id")?,
        challenge_id: row.try_get("challenge_id")?,
        challenge_version_id: row.try_get("challenge_version_id")?,
        agent_id: row.try_get("agent_id")?,
        agent_name: None,
        challenge_title: None,
        artifact_path: row.try_get("artifact_path")?,
        language: row.try_get("language")?,
        status: row.try_get("status")?,
        explanation: row.try_get("explanation")?,
        parent_solution_submission_id: row
            .try_get::<Option<String>, _>("parent_solution_submission_id")?,
        credit_text: row.try_get("credit_text")?,
        visible_after_eval: row.try_get("visible_after_eval")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        evaluation_job_id: Some(input.job_id.clone()),
        evaluation_job_status: Some("queued".to_string()),
        evaluation: None,
        validation_evaluation: None,
        official_evaluation: None,
    })
}

/// Fetch one solution submission with latest job state and validation/official evaluations.
pub async fn get_solution_submission_by_id(
    pool: &PgPool,
    solution_submission_id: &str,
) -> Result<Option<SolutionSubmissionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id, s.challenge_id, s.challenge_version_id, s.agent_id,
            p.title AS challenge_title, a.name AS agent_name,
            s.artifact_path, s.language, s.status, s.explanation,
            s.parent_solution_submission_id, s.credit_text, s.visible_after_eval,
            s.created_at, s.updated_at,
            j.id AS latest_job_id, j.status AS latest_job_status,
            pe.id AS validation_eval_id,
            pe.status AS validation_eval_status,
            pe.eval_type AS validation_eval_eval_type,
            pe.primary_score AS validation_eval_primary_score,
            pe.rank_score AS validation_eval_rank_score,
            pe.aggregate_metrics_json AS validation_eval_aggregate_metrics,
            pe.run_metrics_json AS validation_eval_run_metrics,
            pe.public_results_json AS validation_eval_public_results,
            pe.validation_summary_json AS validation_eval_validation_summary,
            pe.official_summary_json AS validation_eval_official_summary,
            pe.log_path AS validation_eval_log_path,
            pe.started_at AS validation_eval_started_at,
            pe.finished_at AS validation_eval_finished_at,
            oe.id AS official_eval_id,
            oe.status AS official_eval_status,
            oe.eval_type AS official_eval_eval_type,
            oe.primary_score AS official_eval_primary_score,
            oe.rank_score AS official_eval_rank_score,
            oe.aggregate_metrics_json AS official_eval_aggregate_metrics,
            oe.run_metrics_json AS official_eval_run_metrics,
            oe.public_results_json AS official_eval_public_results,
            oe.validation_summary_json AS official_eval_validation_summary,
            oe.official_summary_json AS official_eval_official_summary,
            oe.log_path AS official_eval_log_path,
            oe.started_at AS official_eval_started_at,
            oe.finished_at AS official_eval_finished_at
        FROM solution_submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN challenges p ON p.id = s.challenge_id
        LEFT JOIN LATERAL (
            SELECT id, status FROM evaluation_jobs WHERE solution_submission_id = s.id ORDER BY created_at DESC LIMIT 1
        ) j ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, status, eval_type, primary_score, rank_score, aggregate_metrics_json, run_metrics_json, public_results_json, validation_summary_json, official_summary_json, log_path, started_at, finished_at
            FROM evaluations WHERE solution_submission_id = s.id AND eval_type = 'validation' ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, status, eval_type, primary_score, rank_score, aggregate_metrics_json, run_metrics_json, public_results_json, validation_summary_json, official_summary_json, log_path, started_at, finished_at
            FROM evaluations WHERE solution_submission_id = s.id AND eval_type = 'official' ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE s.id = $1
        LIMIT 1
        "#
    )
    .bind(solution_submission_id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };

    let validation_eval = parse_eval_from_row(&r, "validation_eval")?;
    let official_eval = parse_eval_from_row(&r, "official_eval")?;

    Ok(Some(SolutionSubmissionRecord {
        id: r.try_get("id")?,
        challenge_id: r.try_get("challenge_id")?,
        challenge_version_id: r.try_get("challenge_version_id")?,
        agent_id: r.try_get("agent_id")?,
        agent_name: r.try_get::<Option<String>, _>("agent_name")?,
        challenge_title: r.try_get::<Option<String>, _>("challenge_title")?,
        artifact_path: r.try_get("artifact_path")?,
        language: r.try_get("language")?,
        status: r.try_get("status")?,
        explanation: r.try_get("explanation")?,
        parent_solution_submission_id: r
            .try_get::<Option<String>, _>("parent_solution_submission_id")?,
        credit_text: r.try_get("credit_text")?,
        visible_after_eval: r.try_get("visible_after_eval")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
        evaluation_job_id: r.try_get::<Option<String>, _>("latest_job_id")?,
        evaluation_job_status: r.try_get::<Option<String>, _>("latest_job_status")?,
        evaluation: validation_eval.clone().or_else(|| official_eval.clone()),
        validation_evaluation: validation_eval,
        official_evaluation: official_eval,
    }))
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<AdminSolutionSubmissionListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id,
            s.challenge_id,
            p.title AS challenge_title,
            s.agent_id,
            a.name AS agent_name,
            s.status,
            s.visible_after_eval,
            s.created_at,
            s.updated_at,
            j.id AS latest_job_id,
            j.status AS latest_job_status,
            j.eval_type AS latest_job_eval_type,
            ve.status AS validation_status,
            oe.status AS official_status,
            oe.rank_score AS official_rank_score
        FROM solution_submissions s
        JOIN challenges p ON p.id = s.challenge_id
        JOIN agents a ON a.id = s.agent_id
        LEFT JOIN LATERAL (
            SELECT id, status, eval_type
            FROM evaluation_jobs
            WHERE solution_submission_id = s.id
            ORDER BY created_at DESC
            LIMIT 1
        ) j ON TRUE
        LEFT JOIN LATERAL (
            SELECT status
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'validation'
            ORDER BY created_at DESC
            LIMIT 1
        ) ve ON TRUE
        LEFT JOIN LATERAL (
            SELECT status, rank_score
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'official'
            ORDER BY created_at DESC
            LIMIT 1
        ) oe ON TRUE
        ORDER BY s.updated_at DESC, s.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(AdminSolutionSubmissionListItemDto {
                id: r.try_get("id")?,
                challenge_id: r.try_get("challenge_id")?,
                challenge_title: r.try_get("challenge_title")?,
                agent_id: r.try_get("agent_id")?,
                agent_name: r.try_get("agent_name")?,
                status: r.try_get("status")?,
                visible_after_eval: r.try_get("visible_after_eval")?,
                latest_job_id: r.try_get("latest_job_id")?,
                latest_job_status: r.try_get("latest_job_status")?,
                latest_job_eval_type: r.try_get("latest_job_eval_type")?,
                validation_status: r.try_get("validation_status")?,
                official_status: r.try_get("official_status")?,
                rank_score: r.try_get("official_rank_score")?,
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// List solution submissions for a challenge after an official evaluation makes them visible.
pub async fn list_public_solution_submissions_for_challenge(
    pool: &PgPool,
    challenge_id_or_slug: &str,
) -> Result<Vec<PublicSolutionSubmissionListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id, s.challenge_id, s.challenge_version_id, p.title AS challenge_title,
            s.agent_id, a.name AS agent_name, s.status, s.explanation,
            s.parent_solution_submission_id, s.credit_text, s.created_at, s.updated_at,
            COALESCE(pe.primary_score, (pe.validation_summary_json->>'score')::double precision) AS validation_score,
            COALESCE(oe.rank_score, (oe.official_summary_json->>'score')::double precision) AS official_score,
            COALESCE(pe.rank_score, oe.rank_score, (pe.validation_summary_json->>'score')::double precision, (oe.official_summary_json->>'score')::double precision) AS rank_score,
            COALESCE(pe.aggregate_metrics_json, oe.aggregate_metrics_json, '[]'::jsonb) AS aggregate_metrics,
            COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS official_metrics
        FROM solution_submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN challenges p ON p.id = s.challenge_id
        LEFT JOIN LATERAL (
            SELECT primary_score, rank_score, aggregate_metrics_json, validation_summary_json
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'validation' AND status = 'completed'
            ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT primary_score, rank_score, aggregate_metrics_json, official_summary_json
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'official' AND status = 'completed'
            ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE (p.id = $1 OR p.slug = $1)
          AND s.visible_after_eval = TRUE
        ORDER BY s.created_at DESC
        "#,
    )
    .bind(challenge_id_or_slug)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let aggregate_metrics = decode_optional_json(
                r.try_get::<Option<Value>, _>("aggregate_metrics")?,
                "solution submission aggregate metrics",
            )?
            .unwrap_or_default();
            let official_metrics = decode_optional_json(
                r.try_get::<Option<Value>, _>("official_metrics")?,
                "solution submission official metrics",
            )?
            .unwrap_or_default();

            Ok(PublicSolutionSubmissionListItemDto {
                id: r.try_get("id")?,
                challenge_id: r.try_get("challenge_id")?,
                challenge_version_id: r.try_get("challenge_version_id")?,
                challenge_title: r.try_get("challenge_title")?,
                agent_id: r.try_get("agent_id")?,
                agent_name: r.try_get("agent_name")?,
                status: r.try_get("status")?,
                explanation: r.try_get("explanation")?,
                parent_solution_submission_id: r
                    .try_get::<Option<String>, _>("parent_solution_submission_id")?,
                credit_text: r.try_get("credit_text")?,
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
                validation_score: r.try_get::<Option<f64>, _>("validation_score")?,
                official_score: r.try_get::<Option<f64>, _>("official_score")?,
                rank_score: r.try_get::<Option<f64>, _>("rank_score")?,
                aggregate_metrics,
                official_metrics,
            })
        })
        .collect::<Result<Vec<_>>>()
}

fn parse_eval_from_row(row: &sqlx::postgres::PgRow, prefix: &str) -> Result<Option<EvaluationDto>> {
    let id_col = format!("{}_id", prefix);
    let id: Option<String> = row.try_get(id_col.as_str())?;
    let id = match id {
        Some(i) if !i.is_empty() => i,
        _ => return Ok(None),
    };
    let status_str: String = row.try_get(format!("{}_status", prefix).as_str())?;
    let eval_type_str: String = row.try_get(format!("{}_eval_type", prefix).as_str())?;
    let primary_score: Option<f64> = row.try_get(format!("{}_primary_score", prefix).as_str())?;
    let rank_score: Option<f64> = row.try_get(format!("{}_rank_score", prefix).as_str())?;
    let aggregate_json: Option<Value> =
        row.try_get(format!("{}_aggregate_metrics", prefix).as_str())?;
    let run_metrics_json: Option<Value> =
        row.try_get(format!("{}_run_metrics", prefix).as_str())?;
    let public_results_json: Option<Value> =
        row.try_get(format!("{}_public_results", prefix).as_str())?;
    let validation_summary_json: Option<Value> =
        row.try_get(format!("{}_validation_summary", prefix).as_str())?;
    let official_json: Option<Value> =
        row.try_get(format!("{}_official_summary", prefix).as_str())?;
    let log_path: Option<String> = row.try_get(format!("{}_log_path", prefix).as_str())?;
    let started_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_started_at", prefix).as_str())?;
    let finished_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_finished_at", prefix).as_str())?;

    let status = match status_str.as_str() {
        "queued" => EvaluationStatus::Queued,
        "running" => EvaluationStatus::Running,
        "completed" => EvaluationStatus::Completed,
        "failed" => EvaluationStatus::Failed,
        other => {
            return Err(AppError::Internal(format!(
                "unexpected evaluation status `{other}`"
            )));
        }
    };
    let eval_type = ScoringMode::from_storage_value(&eval_type_str).ok_or_else(|| {
        AppError::Internal(format!("unexpected evaluation type `{eval_type_str}`"))
    })?;
    let public_results: Vec<PublicCaseResult> =
        decode_optional_json(public_results_json, &format!("{prefix} public results"))?
            .unwrap_or_default();
    let aggregate_metrics: Vec<MetricValue> =
        decode_optional_json(aggregate_json, &format!("{prefix} aggregate metrics"))?
            .unwrap_or_default();
    let run_metrics: Vec<RunMetricResult> =
        decode_optional_json(run_metrics_json, &format!("{prefix} run metrics"))?
            .unwrap_or_default();
    let validation_summary = decode_optional_json(
        validation_summary_json,
        &format!("{prefix} validation summary"),
    )?;
    let official_summary =
        decode_optional_json(official_json, &format!("{prefix} official summary"))?;

    Ok(Some(EvaluationDto {
        id,
        status,
        eval_type,
        primary_score,
        rank_score,
        aggregate_metrics,
        run_metrics,
        public_results,
        validation_summary,
        official_summary,
        log_path,
        started_at: started_at.map(|d| d.to_rfc3339()),
        finished_at: finished_at.map(|d| d.to_rfc3339()),
    }))
}
