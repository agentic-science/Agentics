//! Submission, leaderboard, and evaluation query helpers.
//!
//! The API server and worker both depend on this module, so public functions
//! describe transactional side effects such as queueing jobs, changing
//! submission visibility, and updating leaderboard rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::error::{AppError, Result};
use crate::leaderboard::should_replace_leaderboard_entry;
use crate::models::evaluation::{
    EvaluationDto, EvaluationJobPayload, EvaluationStatus, ScoreSummary, ScoringMode,
    ShownCaseResult,
};
use crate::models::problem::ProblemBundleSpec;
use crate::models::request::{LeaderboardEntryDto, PublicSubmissionListItemDto};

pub use super::agents::{
    AgentRecord, AuthenticatedAgent, RegisterAgentInput, authenticate_agent_token, disable_agent,
    register_agent,
};
pub use super::discussions::{
    create_discussion_reply, create_discussion_thread, list_discussion_threads,
};
pub use super::maintenance::{
    HeartbeatPayload, ensure_problems_seeded_from_root, reap_stuck_jobs, upsert_service_heartbeat,
};
pub use super::problems::{
    ProblemVersionRecord, create_or_update_problem, get_published_problem, list_published_problems,
    publish_problem_version,
};

// ---------------------------------------------------------------------------
// Submission
// ---------------------------------------------------------------------------

/// Input for creating a submission and its initial public evaluation job.
#[derive(Debug, Clone)]
pub struct CreateSubmissionInput {
    pub submission_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub problem_id: String,
    pub artifact_path: String,
    pub explanation: String,
    pub parent_submission_id: Option<String>,
    pub credit_text: String,
}

/// Submission row with optional joined evaluation and job metadata.
#[derive(Debug, Clone)]
pub struct SubmissionRecord {
    pub id: String,
    pub problem_id: String,
    pub problem_version_id: String,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub problem_title: Option<String>,
    pub artifact_path: String,
    pub language: String,
    pub status: String,
    pub explanation: String,
    pub parent_submission_id: Option<String>,
    pub credit_text: String,
    pub visible_after_eval: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub evaluation_job_id: Option<String>,
    pub evaluation_job_status: Option<String>,
    pub evaluation: Option<EvaluationDto>,
    pub public_evaluation: Option<EvaluationDto>,
    pub official_evaluation: Option<EvaluationDto>,
}

/// Parse an evaluation DTO from a row using a prefix such as `public_eval`.
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
    let shown_json: Option<Value> = row.try_get(format!("{}_shown_results", prefix).as_str())?;
    let hidden_json: Option<Value> = row.try_get(format!("{}_hidden_summary", prefix).as_str())?;
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
    let eval_type = match eval_type_str.as_str() {
        "public" => ScoringMode::Public,
        "official" => ScoringMode::Official,
        other => {
            return Err(AppError::Internal(format!(
                "unexpected evaluation type `{other}`"
            )));
        }
    };
    let shown_results: Vec<ShownCaseResult> =
        decode_optional_json(shown_json, &format!("{prefix} shown results"))?.unwrap_or_default();
    let hidden_summary = decode_optional_json(hidden_json, &format!("{prefix} hidden summary"))?;
    let official_summary =
        decode_optional_json(official_json, &format!("{prefix} official summary"))?;

    Ok(Some(EvaluationDto {
        id,
        status,
        eval_type,
        primary_score,
        shown_results,
        hidden_summary,
        official_summary,
        log_path,
        started_at: started_at.map(|d| d.to_rfc3339()),
        finished_at: finished_at.map(|d| d.to_rfc3339()),
    }))
}

fn decode_optional_json<T>(value: Option<Value>, context: &str) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    match value {
        Some(Value::Null) | None => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| AppError::Internal(format!("invalid {context}: {e}"))),
    }
}

/// Create a submission and queue its first public evaluation atomically.
pub async fn create_submission_with_job(
    pool: &PgPool,
    input: &CreateSubmissionInput,
) -> Result<SubmissionRecord> {
    let problem = get_published_problem(pool, &input.problem_id).await?;
    let problem = problem.ok_or_else(|| AppError::BadRequest("problem not found".to_string()))?;

    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO submissions (
            id, problem_id, problem_version_id, agent_id, artifact_path, language,
            status, explanation, parent_submission_id, credit_text, visible_after_eval
        )
        VALUES ($1, $2, $3, $4, $5, 'python', 'queued', $6, $7, $8, FALSE)
        RETURNING
            id, problem_id, problem_version_id, agent_id, artifact_path, language,
            status, explanation, parent_submission_id, credit_text, visible_after_eval,
            created_at, updated_at
        "#,
    )
    .bind(&input.submission_id)
    .bind(&problem.problem_id)
    .bind(&problem.problem_version_id)
    .bind(&input.agent_id)
    .bind(&input.artifact_path)
    .bind(&input.explanation)
    .bind(&input.parent_submission_id)
    .bind(&input.credit_text)
    .fetch_one(&mut *tx)
    .await?;

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_path: input.artifact_path.clone(),
        bundle_path: problem.bundle_path.clone(),
        problem_id: problem.problem_id.clone(),
        problem_version_id: problem.problem_version_id.clone(),
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (
            id, submission_id, problem_id, problem_version_id, eval_type, status, payload_json
        )
        VALUES ($1, $2, $3, $4, 'public', 'queued', $5)
        "#,
    )
    .bind(&input.job_id)
    .bind(&input.submission_id)
    .bind(&problem.problem_id)
    .bind(&problem.problem_version_id)
    .bind(&payload)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(SubmissionRecord {
        id: row.try_get("id")?,
        problem_id: row.try_get("problem_id")?,
        problem_version_id: row.try_get("problem_version_id")?,
        agent_id: row.try_get("agent_id")?,
        agent_name: None,
        problem_title: None,
        artifact_path: row.try_get("artifact_path")?,
        language: row.try_get("language")?,
        status: row.try_get("status")?,
        explanation: row.try_get("explanation")?,
        parent_submission_id: row.try_get::<Option<String>, _>("parent_submission_id")?,
        credit_text: row.try_get("credit_text")?,
        visible_after_eval: row.try_get("visible_after_eval")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        evaluation_job_id: Some(input.job_id.clone()),
        evaluation_job_status: Some("queued".to_string()),
        evaluation: None,
        public_evaluation: None,
        official_evaluation: None,
    })
}

/// Fetch one submission with latest job state and public/official evaluations.
pub async fn get_submission_by_id(
    pool: &PgPool,
    submission_id: &str,
) -> Result<Option<SubmissionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            s.id, s.problem_id, s.problem_version_id, s.agent_id,
            p.title AS problem_title, a.name AS agent_name,
            s.artifact_path, s.language, s.status, s.explanation,
            s.parent_submission_id, s.credit_text, s.visible_after_eval,
            s.created_at, s.updated_at,
            j.id AS latest_job_id, j.status AS latest_job_status,
            pe.id AS public_eval_id,
            pe.status AS public_eval_status,
            pe.eval_type AS public_eval_eval_type,
            pe.primary_score AS public_eval_primary_score,
            pe.shown_results_json AS public_eval_shown_results,
            pe.hidden_summary_json AS public_eval_hidden_summary,
            pe.official_summary_json AS public_eval_official_summary,
            pe.log_path AS public_eval_log_path,
            pe.started_at AS public_eval_started_at,
            pe.finished_at AS public_eval_finished_at,
            oe.id AS official_eval_id,
            oe.status AS official_eval_status,
            oe.eval_type AS official_eval_eval_type,
            oe.primary_score AS official_eval_primary_score,
            oe.shown_results_json AS official_eval_shown_results,
            oe.hidden_summary_json AS official_eval_hidden_summary,
            oe.official_summary_json AS official_eval_official_summary,
            oe.log_path AS official_eval_log_path,
            oe.started_at AS official_eval_started_at,
            oe.finished_at AS official_eval_finished_at
        FROM submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN problems p ON p.id = s.problem_id
        LEFT JOIN LATERAL (
            SELECT id, status FROM evaluation_jobs WHERE submission_id = s.id ORDER BY created_at DESC LIMIT 1
        ) j ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, status, eval_type, primary_score, shown_results_json, hidden_summary_json, official_summary_json, log_path, started_at, finished_at
            FROM evaluations WHERE submission_id = s.id AND eval_type = 'public' ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT id, status, eval_type, primary_score, shown_results_json, hidden_summary_json, official_summary_json, log_path, started_at, finished_at
            FROM evaluations WHERE submission_id = s.id AND eval_type = 'official' ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE s.id = $1
        LIMIT 1
        "#
    )
    .bind(submission_id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };

    let public_eval = parse_eval_from_row(&r, "public_eval")?;
    let official_eval = parse_eval_from_row(&r, "official_eval")?;

    Ok(Some(SubmissionRecord {
        id: r.try_get("id")?,
        problem_id: r.try_get("problem_id")?,
        problem_version_id: r.try_get("problem_version_id")?,
        agent_id: r.try_get("agent_id")?,
        agent_name: r.try_get::<Option<String>, _>("agent_name")?,
        problem_title: r.try_get::<Option<String>, _>("problem_title")?,
        artifact_path: r.try_get("artifact_path")?,
        language: r.try_get("language")?,
        status: r.try_get("status")?,
        explanation: r.try_get("explanation")?,
        parent_submission_id: r.try_get::<Option<String>, _>("parent_submission_id")?,
        credit_text: r.try_get("credit_text")?,
        visible_after_eval: r.try_get("visible_after_eval")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
        evaluation_job_id: r.try_get::<Option<String>, _>("latest_job_id")?,
        evaluation_job_status: r.try_get::<Option<String>, _>("latest_job_status")?,
        evaluation: public_eval.clone().or_else(|| official_eval.clone()),
        public_evaluation: public_eval,
        official_evaluation: official_eval,
    }))
}

/// List public submissions for a problem after their public evaluation is visible.
pub async fn list_public_submissions_for_problem(
    pool: &PgPool,
    problem_id_or_slug: &str,
) -> Result<Vec<PublicSubmissionListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id, s.problem_id, s.problem_version_id, p.title AS problem_title,
            s.agent_id, a.name AS agent_name, s.status, s.explanation,
            s.parent_submission_id, s.credit_text, s.created_at, s.updated_at,
            pe.primary_score AS public_score,
            (pe.hidden_summary_json->>'score')::double precision AS hidden_score,
            (oe.official_summary_json->>'score')::double precision AS official_score
        FROM submissions s
        JOIN agents a ON a.id = s.agent_id
        JOIN problems p ON p.id = s.problem_id
        LEFT JOIN LATERAL (
            SELECT primary_score, hidden_summary_json
            FROM evaluations
            WHERE submission_id = s.id AND eval_type = 'public' AND status = 'completed'
            ORDER BY created_at DESC LIMIT 1
        ) pe ON TRUE
        LEFT JOIN LATERAL (
            SELECT official_summary_json
            FROM evaluations
            WHERE submission_id = s.id AND eval_type = 'official' AND status = 'completed'
            ORDER BY created_at DESC LIMIT 1
        ) oe ON TRUE
        WHERE (p.id = $1 OR p.slug = $1)
          AND s.visible_after_eval = TRUE
        ORDER BY s.created_at DESC
        "#,
    )
    .bind(problem_id_or_slug)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(PublicSubmissionListItemDto {
                id: r.try_get("id")?,
                problem_id: r.try_get("problem_id")?,
                problem_version_id: r.try_get("problem_version_id")?,
                problem_title: r.try_get("problem_title")?,
                agent_id: r.try_get("agent_id")?,
                agent_name: r.try_get("agent_name")?,
                status: r.try_get("status")?,
                explanation: r.try_get("explanation")?,
                parent_submission_id: r.try_get::<Option<String>, _>("parent_submission_id")?,
                credit_text: r.try_get("credit_text")?,
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
                public_score: r.try_get::<Option<f64>, _>("public_score")?,
                hidden_score: r.try_get::<Option<f64>, _>("hidden_score")?,
                official_score: r.try_get::<Option<f64>, _>("official_score")?,
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Hide a submission and repair or remove the affected leaderboard entry.
pub async fn hide_submission(pool: &PgPool, submission_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;

    let row: Option<(String, String)> = sqlx::query_as(
        "UPDATE submissions SET visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1 RETURNING problem_id, agent_id"
    )
    .bind(submission_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((problem_id, agent_id)) = row else {
        return Err(AppError::NotFound);
    };

    let leaderboard_entry: Option<(String,)> = sqlx::query_as(
        "SELECT best_submission_id FROM leaderboard_entries WHERE problem_id = $1 AND agent_id = $2 LIMIT 1"
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .fetch_optional(&mut *tx)
    .await?;

    if leaderboard_entry
        .map(|e| e.0 == submission_id)
        .unwrap_or(false)
    {
        let replacement: Option<(String, f64, Value)> = sqlx::query_as(
            r#"
            SELECT s.id, (e.hidden_summary_json->>'score')::double precision AS hidden_score, e.shown_results_json
            FROM submissions s
            JOIN LATERAL (
                SELECT hidden_summary_json, shown_results_json
                FROM evaluations
                WHERE submission_id = s.id AND eval_type = 'public' AND status = 'completed'
                ORDER BY created_at DESC LIMIT 1
            ) e ON TRUE
            WHERE s.problem_id = $1 AND s.agent_id = $2 AND s.id <> $3
              AND s.visible_after_eval = TRUE AND s.status = 'completed'
            ORDER BY hidden_score DESC, s.created_at ASC
            LIMIT 1
            "#
        )
        .bind(&problem_id)
        .bind(&agent_id)
        .bind(submission_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((best_id, best_score, shown_results)) = replacement {
            sqlx::query(
                r#"
                INSERT INTO leaderboard_entries (problem_id, agent_id, best_submission_id, best_hidden_score, shown_summary_json, updated_at)
                VALUES ($1, $2, $3, $4, $5, NOW())
                ON CONFLICT (problem_id, agent_id) DO UPDATE
                SET best_submission_id = EXCLUDED.best_submission_id,
                    best_hidden_score = EXCLUDED.best_hidden_score,
                    shown_summary_json = EXCLUDED.shown_summary_json,
                    updated_at = NOW()
                "#
            )
            .bind(&problem_id)
            .bind(&agent_id)
            .bind(&best_id)
            .bind(best_score)
            .bind(&shown_results)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("DELETE FROM leaderboard_entries WHERE problem_id = $1 AND agent_id = $2")
                .bind(&problem_id)
                .bind(&agent_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Leaderboard
// ---------------------------------------------------------------------------

/// List leaderboard entries for a problem id or slug.
pub async fn list_leaderboard_entries(
    pool: &PgPool,
    problem_id_or_slug: &str,
) -> Result<Vec<LeaderboardEntryDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            le.agent_id, a.name AS agent_name, le.best_submission_id,
            le.best_hidden_score, le.official_score, le.updated_at
        FROM leaderboard_entries le
        JOIN agents a ON a.id = le.agent_id
        JOIN problems p ON p.id = le.problem_id
        WHERE p.id = $1 OR p.slug = $1
        ORDER BY le.best_hidden_score DESC, le.updated_at ASC
        "#,
    )
    .bind(problem_id_or_slug)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(LeaderboardEntryDto {
                agent_id: r.try_get("agent_id")?,
                agent_name: r.try_get("agent_name")?,
                best_submission_id: r.try_get("best_submission_id")?,
                best_hidden_score: r.try_get("best_hidden_score")?,
                official_score: r.try_get::<Option<f64>, _>("official_score")?,
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>>>()
}

/// Upsert a leaderboard entry when a public run improves an agent's hidden score.
pub async fn upsert_leaderboard_entry_for_submission(
    pool: &PgPool,
    submission_id: &str,
    hidden_score: f64,
    shown_results: &[ShownCaseResult],
) -> Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT problem_id, agent_id FROM submissions WHERE id = $1 LIMIT 1")
            .bind(submission_id)
            .fetch_optional(pool)
            .await?;

    let Some((problem_id, agent_id)) = row else {
        return Ok(());
    };

    let current: Option<(f64,)> = sqlx::query_as(
        "SELECT best_hidden_score FROM leaderboard_entries WHERE problem_id = $1 AND agent_id = $2 LIMIT 1"
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .fetch_optional(pool)
    .await?;

    if !should_replace_leaderboard_entry(current.map(|c| c.0), hidden_score) {
        return Ok(());
    }

    let shown_json =
        serde_json::to_value(shown_results).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (problem_id, agent_id, best_submission_id, best_hidden_score, shown_summary_json, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (problem_id, agent_id) DO UPDATE
        SET best_submission_id = EXCLUDED.best_submission_id,
            best_hidden_score = EXCLUDED.best_hidden_score,
            shown_summary_json = EXCLUDED.shown_summary_json,
            updated_at = NOW()
        "#
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .bind(submission_id)
    .bind(hidden_score)
    .bind(&shown_json)
    .execute(pool)
    .await?;

    Ok(())
}

/// Attach an official score to the leaderboard row for a submission's agent/problem.
pub async fn update_official_score_for_submission(
    pool: &PgPool,
    submission_id: &str,
    official_score: f64,
) -> Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT problem_id, agent_id FROM submissions WHERE id = $1 LIMIT 1")
            .bind(submission_id)
            .fetch_optional(pool)
            .await?;

    let Some((problem_id, agent_id)) = row else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE leaderboard_entries SET official_score = $3, updated_at = NOW() WHERE problem_id = $1 AND agent_id = $2"
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .bind(official_score)
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Evaluation Jobs
// ---------------------------------------------------------------------------

/// Claimed or queued evaluation job with parsed runner payload.
#[derive(Debug, Clone)]
pub struct EvaluationJobRecord {
    pub id: String,
    pub submission_id: String,
    pub problem_id: String,
    pub problem_version_id: String,
    pub eval_type: ScoringMode,
    pub status: String,
    pub attempt_count: i32,
    pub payload: EvaluationJobPayload,
}

/// Claim the next queued job using `FOR UPDATE SKIP LOCKED`.
///
/// Public jobs also move their submission into `running`; official jobs leave
/// public submission visibility unchanged.
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
            WHERE status = 'queued' AND scheduled_at <= NOW()
            ORDER BY priority DESC, scheduled_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE evaluation_jobs j
        SET status = 'running', claimed_at = NOW(), worker_id = $1, attempt_count = j.attempt_count + 1
        FROM next_job
        WHERE j.id = next_job.id
        RETURNING j.id, j.submission_id, j.problem_id, j.problem_version_id, j.eval_type, j.status, j.attempt_count, j.payload_json
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
    let eval_type = match eval_type_raw.as_str() {
        "official" => ScoringMode::Official,
        "public" => ScoringMode::Public,
        other => {
            return Err(AppError::Internal(format!(
                "unexpected evaluation job type `{other}`"
            )));
        }
    };
    let submission_id: String = r.try_get("submission_id")?;

    if eval_type == ScoringMode::Public {
        sqlx::query("UPDATE submissions SET status = 'running', updated_at = NOW() WHERE id = $1")
            .bind(&submission_id)
            .execute(&mut *tx)
            .await?;
    }

    let payload: EvaluationJobPayload = serde_json::from_value(r.try_get("payload_json")?)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    tx.commit().await?;

    Ok(Some(EvaluationJobRecord {
        id: r.try_get("id")?,
        submission_id,
        problem_id: r.try_get("problem_id")?,
        problem_version_id: r.try_get("problem_version_id")?,
        eval_type,
        status: r.try_get("status")?,
        attempt_count: r.try_get("attempt_count")?,
        payload,
    }))
}

/// Input for queueing a public re-run or an official evaluation.
#[derive(Debug, Clone)]
pub struct QueueEvaluationJobInput {
    pub job_id: String,
    pub submission_id: String,
    pub eval_type: ScoringMode,
}

/// Queue an evaluation job for an existing submission.
///
/// Official jobs are rejected when the problem version does not enable heldout
/// data. Public jobs reset visibility until the new public result completes.
pub async fn queue_evaluation_job(
    pool: &PgPool,
    input: &QueueEvaluationJobInput,
) -> Result<EvaluationJobRecord> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        SELECT s.id, s.problem_id, s.problem_version_id, s.agent_id, s.artifact_path, s.visible_after_eval,
               pv.bundle_path, pv.spec_json
        FROM submissions s
        JOIN problem_versions pv ON pv.id = s.problem_version_id
        WHERE s.id = $1
        LIMIT 1
        "#
    )
    .bind(&input.submission_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AppError::NotFound)?;

    let spec_json: Value = row.try_get("spec_json")?;
    let spec: ProblemBundleSpec =
        serde_json::from_value(spec_json).map_err(|e| AppError::Internal(e.to_string()))?;

    if input.eval_type == ScoringMode::Official && !spec.datasets.heldout_enabled {
        return Err(AppError::BadRequest(
            "problem version does not support heldout official run".to_string(),
        ));
    }

    let payload = serde_json::to_value(EvaluationJobPayload {
        artifact_path: row.try_get("artifact_path")?,
        bundle_path: row.try_get("bundle_path")?,
        problem_id: row.try_get("problem_id")?,
        problem_version_id: row.try_get("problem_version_id")?,
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let eval_type_str = match input.eval_type {
        ScoringMode::Official => "official",
        ScoringMode::Public => "public",
    };
    let priority = if input.eval_type == ScoringMode::Official {
        10
    } else {
        0
    };

    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (id, submission_id, problem_id, problem_version_id, eval_type, status, priority, payload_json)
        VALUES ($1, $2, $3, $4, $5, 'queued', $6, $7)
        "#
    )
    .bind(&input.job_id)
    .bind(row.try_get::<String, _>("id")?)
    .bind(row.try_get::<String, _>("problem_id")?)
    .bind(row.try_get::<String, _>("problem_version_id")?)
    .bind(eval_type_str)
    .bind(priority)
    .bind(&payload)
    .execute(&mut *tx)
    .await?;

    if input.eval_type == ScoringMode::Public {
        sqlx::query(
            "UPDATE submissions SET status = 'queued', visible_after_eval = FALSE, updated_at = NOW() WHERE id = $1"
        )
        .bind(row.try_get::<String, _>("id")?)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(EvaluationJobRecord {
        id: input.job_id.clone(),
        submission_id: row.try_get("id")?,
        problem_id: row.try_get("problem_id")?,
        problem_version_id: row.try_get("problem_version_id")?,
        eval_type: input.eval_type,
        status: "queued".to_string(),
        attempt_count: 0,
        payload: serde_json::from_value(payload).map_err(|e| AppError::Internal(e.to_string()))?,
    })
}

/// Input for creating or resetting the evaluation row associated with a job.
#[derive(Debug, Clone)]
pub struct MarkEvaluationStartedInput {
    pub evaluation_id: String,
    pub submission_id: String,
    pub job_id: String,
    pub eval_type: ScoringMode,
}

/// Mark a job's evaluation as running.
pub async fn mark_evaluation_started(
    pool: &PgPool,
    input: &MarkEvaluationStartedInput,
) -> Result<()> {
    let eval_type_str = match input.eval_type {
        ScoringMode::Official => "official",
        ScoringMode::Public => "public",
    };

    sqlx::query(
        r#"
        INSERT INTO evaluations (id, submission_id, job_id, eval_type, status, started_at)
        VALUES ($1, $2, $3, $4, 'running', NOW())
        ON CONFLICT (job_id) DO UPDATE
        SET status = 'running', started_at = NOW(), finished_at = NULL
        "#,
    )
    .bind(&input.evaluation_id)
    .bind(&input.submission_id)
    .bind(&input.job_id)
    .bind(eval_type_str)
    .execute(pool)
    .await?;

    Ok(())
}

/// Validated runner result prepared for persistence.
#[derive(Debug, Clone)]
pub struct PersistedEvaluationResult {
    pub evaluation_id: String,
    pub submission_id: String,
    pub job_id: String,
    pub eval_type: ScoringMode,
    pub status: EvaluationStatus,
    pub primary_score: Option<f64>,
    pub shown_results: Vec<ShownCaseResult>,
    pub hidden_summary: Option<ScoreSummary>,
    pub official_summary: Option<ScoreSummary>,
    pub log_path: Option<String>,
    pub last_error: Option<String>,
}

/// Persist a finished evaluation and update dependent submission/leaderboard state.
pub async fn mark_evaluation_finished(
    pool: &PgPool,
    result: &PersistedEvaluationResult,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    let shown_json = serde_json::to_value(&result.shown_results)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let hidden_json = serde_json::to_value(&result.hidden_summary)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let official_json = serde_json::to_value(&result.official_summary)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let status_str = match result.status {
        EvaluationStatus::Completed => "completed",
        _ => "failed",
    };

    sqlx::query(
        r#"
        UPDATE evaluations
        SET status = $2, primary_score = $3, shown_results_json = $4,
            hidden_summary_json = $5, official_summary_json = $6, log_path = $7, finished_at = NOW()
        WHERE job_id = $1
        "#,
    )
    .bind(&result.job_id)
    .bind(status_str)
    .bind(result.primary_score)
    .bind(&shown_json)
    .bind(&hidden_json)
    .bind(&official_json)
    .bind(&result.log_path)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = $2, finished_at = NOW(), last_error = $3
        WHERE id = $1
        "#,
    )
    .bind(&result.job_id)
    .bind(status_str)
    .bind(&result.last_error)
    .execute(&mut *tx)
    .await?;

    if result.eval_type == ScoringMode::Public {
        let visible = result.status == EvaluationStatus::Completed;
        let sub_status = if visible { "completed" } else { "failed" };
        sqlx::query(
            "UPDATE submissions SET status = $2, visible_after_eval = $3, updated_at = NOW() WHERE id = $1"
        )
        .bind(&result.submission_id)
        .bind(sub_status)
        .bind(visible)
        .execute(&mut *tx)
        .await?;
    }

    if result.status == EvaluationStatus::Completed && result.eval_type == ScoringMode::Public {
        if let Some(ref hidden) = result.hidden_summary {
            upsert_leaderboard_entry_for_submission_tx(
                &mut tx,
                &result.submission_id,
                hidden.score,
                &result.shown_results,
            )
            .await?;
        }
    } else if result.status == EvaluationStatus::Completed
        && result.eval_type == ScoringMode::Official
        && let Some(ref official) = result.official_summary
    {
        update_official_score_for_submission_tx(&mut tx, &result.submission_id, official.score)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

async fn upsert_leaderboard_entry_for_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    submission_id: &str,
    hidden_score: f64,
    shown_results: &[ShownCaseResult],
) -> Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT problem_id, agent_id FROM submissions WHERE id = $1 LIMIT 1")
            .bind(submission_id)
            .fetch_optional(&mut **tx)
            .await?;

    let Some((problem_id, agent_id)) = row else {
        return Ok(());
    };

    let current: Option<(f64,)> = sqlx::query_as(
        "SELECT best_hidden_score FROM leaderboard_entries WHERE problem_id = $1 AND agent_id = $2 LIMIT 1"
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .fetch_optional(&mut **tx)
    .await?;

    if !should_replace_leaderboard_entry(current.map(|c| c.0), hidden_score) {
        return Ok(());
    }

    let shown_json =
        serde_json::to_value(shown_results).map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (problem_id, agent_id, best_submission_id, best_hidden_score, shown_summary_json, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (problem_id, agent_id) DO UPDATE
        SET best_submission_id = EXCLUDED.best_submission_id,
            best_hidden_score = EXCLUDED.best_hidden_score,
            shown_summary_json = EXCLUDED.shown_summary_json,
            updated_at = NOW()
        "#
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .bind(submission_id)
    .bind(hidden_score)
    .bind(&shown_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn update_official_score_for_submission_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    submission_id: &str,
    official_score: f64,
) -> Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT problem_id, agent_id FROM submissions WHERE id = $1 LIMIT 1")
            .bind(submission_id)
            .fetch_optional(&mut **tx)
            .await?;

    let Some((problem_id, agent_id)) = row else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE leaderboard_entries SET official_score = $3, updated_at = NOW() WHERE problem_id = $1 AND agent_id = $2"
    )
    .bind(&problem_id)
    .bind(&agent_id)
    .bind(official_score)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
