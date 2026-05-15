use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::evaluation::{
    EvaluationDto, EvaluationJobPayload, EvaluationStatus, MetricValue, PublicCaseResult,
    RunMetricResult, ScoringMode,
};
use crate::models::ids::{AgentId, EvaluationId, EvaluationJobId, SolutionSubmissionId};
use crate::models::names::{ChallengeName, TargetName};
use crate::models::request::AdminSolutionSubmissionListItemDto;
use crate::models::request::PublicSolutionSubmissionListItemDto;
use crate::storage::StorageKey;

use super::challenges::get_published_challenge;
use super::evaluation_policy::ensure_challenge_supports_eval_type;
use super::ids::{
    agent_id_from_row, challenge_name_from_row, optional_solution_submission_id_from_row,
    optional_uuid_string_from_row, solution_submission_id_from_row, target_from_row,
};
use super::json::decode_optional_json;

/// Input for creating a solution submission and its initial evaluation job.
#[derive(Debug, Clone)]
pub struct CreateSolutionSubmissionInput {
    pub solution_submission_id: SolutionSubmissionId,
    pub job_id: EvaluationJobId,
    pub agent_id: AgentId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub artifact_key: StorageKey,
    pub language: String,
    pub eval_type: ScoringMode,
    pub explanation: String,
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    pub initial_job_delay_seconds: Option<i64>,
    pub quota_admission: SolutionSubmissionQuotaAdmission,
}

/// Authoritative quota limits applied inside the submission/job transaction.
#[derive(Debug, Clone, Copy)]
pub struct SolutionSubmissionQuotaAdmission {
    pub window_seconds: i64,
    pub per_agent_challenge_limit: i64,
    pub challenge_lifetime_limit: Option<i64>,
    pub max_active_official_jobs: Option<i64>,
}

/// Solution submission row with optional joined evaluation and job metadata.
#[derive(Debug, Clone)]
pub struct SolutionSubmissionRecord {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub agent_id: AgentId,
    pub agent_display_name: Option<String>,
    pub challenge_title: Option<String>,
    pub artifact_key: StorageKey,
    pub language: String,
    pub status: String,
    pub explanation: String,
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    pub visible_after_eval: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub evaluation_job_id: Option<EvaluationJobId>,
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
    let challenge = get_published_challenge(pool, &input.challenge_name).await?;
    let challenge =
        challenge.ok_or_else(|| AppError::BadRequest("challenge not found".to_string()))?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    ensure_challenge_supports_eval_type(
        pool,
        &challenge.challenge_name,
        &spec,
        &input.target,
        input.eval_type,
        &input.agent_id,
    )
    .await?;

    let mut tx = pool.begin().await?;
    enforce_quota_admission(&mut tx, input).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO solution_submissions (
            id, challenge_name, target, agent_id, artifact_key, language,
            status, explanation, parent_solution_submission_id, credit_text, visible_after_eval
        )
        VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6, 'queued', $7, $8::uuid, $9, FALSE)
        RETURNING
            id, challenge_name, target, agent_id, artifact_key, language,
            status, explanation, parent_solution_submission_id, credit_text, visible_after_eval,
            created_at, updated_at
        "#,
    )
    .bind(input.solution_submission_id.as_str())
    .bind(challenge.challenge_name.as_str())
    .bind(input.target.as_str())
    .bind(input.agent_id.as_str())
    .bind(input.artifact_key.as_str())
    .bind(&input.language)
    .bind(&input.explanation)
    .bind(
        input
            .parent_solution_submission_id
            .as_ref()
            .map(SolutionSubmissionId::as_str),
    )
    .bind(&input.credit_text)
    .fetch_one(&mut *tx)
    .await?;

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_key: input.artifact_key.clone(),
        bundle_path: challenge.bundle_path.clone(),
        challenge_name: challenge.challenge_name.clone(),
        target: input.target.clone(),
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
            id, solution_submission_id, challenge_name, target, eval_type, status, priority, payload_json, scheduled_at
        )
        VALUES (
            $1::uuid, $2::uuid, $3, $4, $5, 'queued', $6, $7,
            CASE
                WHEN $8::BIGINT IS NULL THEN NOW()
                ELSE NOW() + ($8::DOUBLE PRECISION * INTERVAL '1 second')
            END
        )
        "#,
    )
    .bind(input.job_id.as_str())
    .bind(input.solution_submission_id.as_str())
    .bind(challenge.challenge_name.as_str())
    .bind(input.target.as_str())
    .bind(input.eval_type.as_str())
    .bind(priority)
    .bind(&payload)
    .bind(input.initial_job_delay_seconds)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(SolutionSubmissionRecord {
        id: solution_submission_id_from_row(&row, "id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        target: target_from_row(&row, "target")?,
        agent_id: agent_id_from_row(&row, "agent_id")?,
        agent_display_name: None,
        challenge_title: None,
        artifact_key: storage_key_from_row(&row, "artifact_key")?,
        language: row.try_get("language")?,
        status: row.try_get("status")?,
        explanation: row.try_get("explanation")?,
        parent_solution_submission_id: optional_solution_submission_id_from_row(
            &row,
            "parent_solution_submission_id",
        )?,
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

/// Delete a solution submission and its dependent jobs/evaluations.
pub async fn delete_solution_submission(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<()> {
    sqlx::query("DELETE FROM solution_submissions WHERE id = $1::uuid")
        .bind(solution_submission_id.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

/// Handles enforce quota admission for this module.
async fn enforce_quota_admission(
    tx: &mut Transaction<'_, Postgres>,
    input: &CreateSolutionSubmissionInput,
) -> Result<()> {
    let mut scopes = vec![format!(
        "agent:{}:challenge:{}:target:{}:mode:{}:daily",
        input.agent_id,
        input.challenge_name,
        input.target,
        input.eval_type.as_str()
    )];
    if input.eval_type == ScoringMode::Official {
        scopes.push("global:official-active".to_string());
    }
    if input.quota_admission.challenge_lifetime_limit.is_some() {
        scopes.push(format!(
            "agent:{}:challenge:{}:target:{}:mode:{}:lifetime",
            input.agent_id,
            input.challenge_name,
            input.target,
            input.eval_type.as_str()
        ));
    }
    scopes.sort();

    for scope in scopes {
        lock_quota_scope(tx, &scope).await?;
    }

    let used = count_recent_runs_for_agent_challenge_tx(
        tx,
        &input.agent_id,
        &input.challenge_name,
        &input.target,
        input.eval_type,
        input.quota_admission.window_seconds,
    )
    .await?;
    let limit = input.quota_admission.per_agent_challenge_limit;
    if used >= limit {
        return Err(AppError::TooManyRequests(format!(
            "{} quota exceeded for challenge `{}`: {} of {} runs used in the last 24 hours",
            input.eval_type.as_str(),
            input.challenge_name,
            used,
            limit
        )));
    }

    if let Some(limit) = input.quota_admission.challenge_lifetime_limit {
        let used = count_lifetime_runs_for_agent_challenge_tx(
            tx,
            &input.agent_id,
            &input.challenge_name,
            &input.target,
            input.eval_type,
        )
        .await?;
        if used >= limit {
            return Err(AppError::TooManyRequests(format!(
                "{} challenge limit exceeded for challenge `{}`: {} of {} lifetime runs used",
                input.eval_type.as_str(),
                input.challenge_name,
                used,
                limit
            )));
        }
    }

    if let Some(max_active) = input.quota_admission.max_active_official_jobs {
        let active = count_active_evaluation_jobs_tx(tx, ScoringMode::Official).await?;
        if active >= max_active {
            return Err(AppError::TooManyRequests(format!(
                "official evaluation queue is full: {active} of {max_active} official jobs are queued or running"
            )));
        }
    }

    Ok(())
}

/// Handles lock quota scope for this module.
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

/// Handles count recent runs for agent challenge tx for this module.
async fn count_recent_runs_for_agent_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM solution_submissions s
        JOIN evaluation_jobs j ON j.solution_submission_id = s.id
        WHERE s.agent_id = $1::uuid
          AND s.challenge_name = $2
          AND s.target = $3
          AND j.eval_type = $4
          AND s.created_at >= NOW() - ($5::DOUBLE PRECISION * INTERVAL '1 second')
        "#,
    )
    .bind(agent_id.as_str())
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(eval_type.as_str())
    .bind(window_seconds)
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Handles count lifetime runs for agent challenge tx for this module.
async fn count_lifetime_runs_for_agent_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM solution_submissions s
        JOIN evaluation_jobs j ON j.solution_submission_id = s.id
        WHERE s.agent_id = $1::uuid
          AND s.challenge_name = $2
          AND s.target = $3
          AND j.eval_type = $4
        "#,
    )
    .bind(agent_id.as_str())
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(eval_type.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Handles count active evaluation jobs tx for this module.
async fn count_active_evaluation_jobs_tx(
    tx: &mut Transaction<'_, Postgres>,
    eval_type: ScoringMode,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM evaluation_jobs
        WHERE eval_type = $1
          AND status IN ('queued', 'running')
        "#,
    )
    .bind(eval_type.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Fetch one solution submission with latest job state and validation/official evaluations.
pub async fn get_solution_submission_by_id(
    pool: &PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<Option<SolutionSubmissionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id, s.challenge_name, s.target, s.agent_id,
            p.title AS challenge_title, a.display_name AS agent_display_name,
            s.artifact_key, s.language, s.status, s.explanation,
            s.parent_solution_submission_id, s.credit_text, s.visible_after_eval,
            s.created_at, s.updated_at,
            j.id AS latest_job_id, j.status AS latest_job_status,
            pe.id AS validation_eval_id,
            pe.target AS validation_eval_target,
            pe.status AS validation_eval_status,
            pe.eval_type AS validation_eval_eval_type,
            pe.primary_score AS validation_eval_primary_score,
            pe.rank_score AS validation_eval_rank_score,
            pe.aggregate_metrics_json AS validation_eval_aggregate_metrics,
            pe.run_metrics_json AS validation_eval_run_metrics,
            pe.public_results_json AS validation_eval_public_results,
            pe.validation_summary_json AS validation_eval_validation_summary,
            pe.official_summary_json AS validation_eval_official_summary,
            pe.log_key AS validation_eval_log_key,
            pe.started_at AS validation_eval_started_at,
            pe.finished_at AS validation_eval_finished_at,
            oe.id AS official_eval_id,
            oe.target AS official_eval_target,
            oe.status AS official_eval_status,
            oe.eval_type AS official_eval_eval_type,
            oe.primary_score AS official_eval_primary_score,
            oe.rank_score AS official_eval_rank_score,
            oe.aggregate_metrics_json AS official_eval_aggregate_metrics,
            oe.run_metrics_json AS official_eval_run_metrics,
            oe.public_results_json AS official_eval_public_results,
            oe.validation_summary_json AS official_eval_validation_summary,
            oe.official_summary_json AS official_eval_official_summary,
            oe.log_key AS official_eval_log_key,
            oe.started_at AS official_eval_started_at,
            oe.finished_at AS official_eval_finished_at
        FROM solution_submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN challenges p ON p.name = s.challenge_name
        LEFT JOIN LATERAL (
            SELECT id, status FROM evaluation_jobs WHERE solution_submission_id = s.id ORDER BY created_at DESC LIMIT 1
        ) j ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, target, status, eval_type, primary_score, rank_score, aggregate_metrics_json, run_metrics_json, public_results_json, validation_summary_json, official_summary_json, log_key, started_at, finished_at
            FROM evaluations WHERE solution_submission_id = s.id AND eval_type = 'validation' AND target = s.target ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, target, status, eval_type, primary_score, rank_score, aggregate_metrics_json, run_metrics_json, public_results_json, validation_summary_json, official_summary_json, log_key, started_at, finished_at
            FROM evaluations WHERE solution_submission_id = s.id AND eval_type = 'official' AND target = s.target ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE s.id = $1::uuid
        LIMIT 1
        "#
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };

    let validation_eval = parse_eval_from_row(&r, "validation_eval")?;
    let official_eval = parse_eval_from_row(&r, "official_eval")?;

    Ok(Some(SolutionSubmissionRecord {
        id: solution_submission_id_from_row(&r, "id")?,
        challenge_name: challenge_name_from_row(&r, "challenge_name")?,
        target: target_from_row(&r, "target")?,
        agent_id: agent_id_from_row(&r, "agent_id")?,
        agent_display_name: r.try_get::<Option<String>, _>("agent_display_name")?,
        challenge_title: r.try_get::<Option<String>, _>("challenge_title")?,
        artifact_key: storage_key_from_row(&r, "artifact_key")?,
        language: r.try_get("language")?,
        status: r.try_get("status")?,
        explanation: r.try_get("explanation")?,
        parent_solution_submission_id: optional_solution_submission_id_from_row(
            &r,
            "parent_solution_submission_id",
        )?,
        credit_text: r.try_get("credit_text")?,
        visible_after_eval: r.try_get("visible_after_eval")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
        evaluation_job_id: optional_evaluation_job_id_from_row(&r, "latest_job_id")?,
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
            s.challenge_name,
            s.target,
            p.title AS challenge_title,
            s.agent_id,
            a.display_name AS agent_display_name,
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
        JOIN challenges p ON p.name = s.challenge_name
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
                id: solution_submission_id_from_row(&r, "id")?,
                challenge_name: challenge_name_from_row(&r, "challenge_name")?,
                challenge_title: r.try_get("challenge_title")?,
                target: target_from_row(&r, "target")?,
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
                status: r.try_get("status")?,
                visible_after_eval: r.try_get("visible_after_eval")?,
                latest_job_id: optional_evaluation_job_id_from_row(&r, "latest_job_id")?,
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
    challenge_name: &ChallengeName,
    limit: i64,
) -> Result<Vec<PublicSolutionSubmissionListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id, s.challenge_name, s.target, p.title AS challenge_title,
            s.agent_id, a.display_name AS agent_display_name, s.status, s.explanation,
            s.parent_solution_submission_id, s.credit_text, s.created_at, s.updated_at,
            COALESCE(pe.primary_score, (pe.validation_summary_json->>'score')::double precision) AS validation_score,
            COALESCE(oe.rank_score, (oe.official_summary_json->>'score')::double precision) AS official_score,
            COALESCE(pe.rank_score, oe.rank_score, (pe.validation_summary_json->>'score')::double precision, (oe.official_summary_json->>'score')::double precision) AS rank_score,
            COALESCE(pe.aggregate_metrics_json, oe.aggregate_metrics_json, '[]'::jsonb) AS aggregate_metrics,
            COALESCE(oe.aggregate_metrics_json, '[]'::jsonb) AS official_metrics
        FROM solution_submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN challenges p ON p.name = s.challenge_name
        LEFT JOIN LATERAL (
            SELECT primary_score, rank_score, aggregate_metrics_json, validation_summary_json
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'validation' AND status = 'completed' AND target = s.target
            ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT primary_score, rank_score, aggregate_metrics_json, official_summary_json
            FROM evaluations
            WHERE solution_submission_id = s.id AND eval_type = 'official' AND status = 'completed' AND target = s.target
            ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE p.name = $1
          AND s.visible_after_eval = TRUE
        ORDER BY s.created_at DESC
        LIMIT $2
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(limit)
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
                id: solution_submission_id_from_row(&r, "id")?,
                challenge_name: challenge_name_from_row(&r, "challenge_name")?,
                target: target_from_row(&r, "target")?,
                challenge_title: r.try_get("challenge_title")?,
                agent_id: agent_id_from_row(&r, "agent_id")?,
                agent_display_name: r.try_get("agent_display_name")?,
                status: r.try_get("status")?,
                explanation: r.try_get("explanation")?,
                parent_solution_submission_id: optional_solution_submission_id_from_row(
                    &r,
                    "parent_solution_submission_id",
                )?,
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

/// Reads parse eval from a database row and validates its domain shape.
fn parse_eval_from_row(row: &sqlx::postgres::PgRow, prefix: &str) -> Result<Option<EvaluationDto>> {
    let id_col = format!("{}_id", prefix);
    let id = optional_evaluation_id_from_row(row, id_col.as_str())?;
    let id = match id {
        Some(i) => i,
        _ => return Ok(None),
    };
    let status_str: String = row.try_get(format!("{}_status", prefix).as_str())?;
    let target_col = format!("{}_target", prefix);
    let target = target_from_row(row, target_col.as_str())?;
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
    let log_key = optional_storage_key_from_row(row, format!("{prefix}_log_key").as_str())?;
    let started_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_started_at", prefix).as_str())?;
    let finished_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_finished_at", prefix).as_str())?;

    let status = EvaluationStatus::from_storage_value(&status_str).ok_or_else(|| {
        AppError::Internal(format!("unexpected evaluation status `{status_str}`"))
    })?;
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
        target,
        status,
        eval_type,
        primary_score,
        rank_score,
        aggregate_metrics,
        run_metrics,
        public_results,
        validation_summary,
        official_summary,
        log_key,
        started_at: started_at.map(|d| d.to_rfc3339()),
        finished_at: finished_at.map(|d| d.to_rfc3339()),
    }))
}

/// Reads storage key from a database row and validates its domain shape.
fn storage_key_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value)
        .map_err(|e| AppError::Internal(format!("stored invalid storage key in `{column}`: {e}")))
}

/// Reads optional storage key from a database row and validates its domain shape.
fn optional_storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<StorageKey>> {
    row.try_get::<Option<String>, _>(column)?
        .map(StorageKey::try_new)
        .transpose()
        .map_err(|e| AppError::Internal(format!("stored invalid storage key in `{column}`: {e}")))
}

/// Reads optional evaluation job id from a database row and validates its domain shape.
fn optional_evaluation_job_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<EvaluationJobId>> {
    optional_uuid_string_from_row(row, column)?
        .map(EvaluationJobId::try_new)
        .transpose()
        .map_err(|e| {
            AppError::Internal(format!(
                "stored invalid evaluation job id in column `{column}`: {e}"
            ))
        })
}

/// Reads optional evaluation id from a database row and validates its domain shape.
fn optional_evaluation_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<EvaluationId>> {
    optional_uuid_string_from_row(row, column)?
        .map(EvaluationId::try_new)
        .transpose()
        .map_err(|e| {
            AppError::Internal(format!(
                "stored invalid evaluation id in column `{column}`: {e}"
            ))
        })
}
