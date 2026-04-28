//! Database query helpers for agents, problems, submissions, leaderboards, and evaluations.
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
use crate::models::problem::{CreateProblemVersionResponse, ProblemBundleSpec, ProblemListItemDto};
use crate::models::request::{LeaderboardEntryDto, PublicSubmissionListItemDto};

pub use super::discussions::{
    create_discussion_reply, create_discussion_thread, list_discussion_threads,
};
pub use super::maintenance::{
    HeartbeatPayload, ensure_problems_seeded_from_root, reap_stuck_jobs, upsert_service_heartbeat,
};

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// Input for creating an agent and its initial bearer token in one transaction.
#[derive(Debug, Clone)]
pub struct RegisterAgentInput {
    pub agent_id: String,
    pub token_id: String,
    pub token_hash: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub model_info: Value,
}

/// Persisted agent row returned after registration.
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub model_info: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Agent identity resolved from a valid, active bearer token.
#[derive(Debug, Clone)]
pub struct AuthenticatedAgent {
    pub agent_id: String,
    pub token_id: String,
    pub name: String,
}

/// Register an active agent and insert its first token.
pub async fn register_agent(pool: &PgPool, input: &RegisterAgentInput) -> Result<AgentRecord> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO agents (id, name, description, owner, model_info, status)
        VALUES ($1, $2, $3, $4, $5, 'active')
        RETURNING id, name, description, owner, model_info, status, created_at
        "#,
    )
    .bind(&input.agent_id)
    .bind(&input.name)
    .bind(&input.description)
    .bind(&input.owner)
    .bind(&input.model_info)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO agent_tokens (id, agent_id, token_hash) VALUES ($1, $2, $3)")
        .bind(&input.token_id)
        .bind(&input.agent_id)
        .bind(&input.token_hash)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(AgentRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        owner: row.try_get("owner")?,
        model_info: row.try_get("model_info")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Authenticate a bearer token and refresh its `last_used_at` timestamp.
pub async fn authenticate_agent_token(
    pool: &PgPool,
    token: &str,
) -> Result<Option<AuthenticatedAgent>> {
    let token_hash = crate::auth::hash_agent_token(token);

    let row = sqlx::query(
        r#"
        SELECT a.id AS agent_id, t.id AS token_id, a.name
        FROM agent_tokens t
        JOIN agents a ON a.id = t.agent_id
        WHERE t.token_hash = $1
          AND t.revoked_at IS NULL
          AND a.status = 'active'
        LIMIT 1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let token_id: String = row.try_get("token_id")?;
    sqlx::query("UPDATE agent_tokens SET last_used_at = NOW() WHERE id = $1")
        .bind(&token_id)
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedAgent {
        agent_id: row.try_get("agent_id")?,
        token_id,
        name: row.try_get("name")?,
    }))
}

/// Disable an agent and revoke all of its tokens.
pub async fn disable_agent(pool: &PgPool, agent_id: &str) -> Result<()> {
    let row = sqlx::query("UPDATE agents SET status = 'disabled' WHERE id = $1 RETURNING id")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;

    if row.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query(
        "UPDATE agent_tokens SET revoked_at = COALESCE(revoked_at, NOW()) WHERE agent_id = $1",
    )
    .bind(agent_id)
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Problem
// ---------------------------------------------------------------------------

/// Latest published problem version joined with problem metadata.
#[derive(Debug, Clone)]
pub struct ProblemVersionRecord {
    pub problem_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub problem_version_id: String,
    pub version: String,
    pub bundle_path: String,
    pub statement_path: String,
    pub spec_json: Value,
}

/// Create or update the problem shell that versions attach to.
pub async fn create_or_update_problem(
    pool: &PgPool,
    id: &str,
    slug: &str,
    title: &str,
    description: &str,
) -> Result<crate::models::problem::ProblemAdminResponse> {
    let row = sqlx::query(
        r#"
        INSERT INTO problems (id, slug, title, description, status)
        VALUES ($1, $2, $3, $4, 'active')
        ON CONFLICT (id) DO UPDATE
        SET slug = EXCLUDED.slug,
            title = EXCLUDED.title,
            description = EXCLUDED.description,
            status = 'active',
            updated_at = NOW()
        RETURNING id, slug, title, description, status, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(slug)
    .bind(title)
    .bind(description)
    .fetch_one(pool)
    .await?;

    Ok(crate::models::problem::ProblemAdminResponse {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

/// Publish a validated bundle as the current problem version.
pub async fn publish_problem_version(
    pool: &PgPool,
    problem_id: &str,
    bundle_path: &str,
    statement_path: &str,
    spec: &ProblemBundleSpec,
    title: &str,
    description: &str,
) -> Result<CreateProblemVersionResponse> {
    let version_id = format!("{}:{}", problem_id, spec.problem_version);
    let spec_json = serde_json::to_value(spec).map_err(|e| AppError::Internal(e.to_string()))?;

    let row = sqlx::query(
        r#"
        WITH upserted_version AS (
            INSERT INTO problem_versions (
                id, problem_id, version, bundle_path, statement_path, spec_json, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'published')
            ON CONFLICT (problem_id, version) DO UPDATE
            SET bundle_path = EXCLUDED.bundle_path,
                statement_path = EXCLUDED.statement_path,
                spec_json = EXCLUDED.spec_json,
                status = 'published'
            RETURNING id, problem_id, version, bundle_path, statement_path
        )
        UPDATE problems p
        SET title = $7,
            description = CASE WHEN p.description = '' THEN $8 ELSE p.description END,
            status = 'active',
            updated_at = NOW()
        FROM upserted_version v
        WHERE p.id = v.problem_id
        RETURNING
            p.id AS problem_id,
            p.slug,
            p.title,
            v.id AS version_id,
            v.version,
            v.bundle_path,
            v.statement_path
        "#,
    )
    .bind(&version_id)
    .bind(problem_id)
    .bind(&spec.problem_version)
    .bind(bundle_path)
    .bind(statement_path)
    .bind(&spec_json)
    .bind(title)
    .bind(description)
    .fetch_one(pool)
    .await?;

    Ok(CreateProblemVersionResponse {
        problem_id: row.try_get("problem_id")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        version_id: row.try_get("version_id")?,
        version: row.try_get("version")?,
        bundle_path: row.try_get("bundle_path")?,
        statement_path: row.try_get("statement_path")?,
    })
}

/// List active problems with their latest published version.
pub async fn list_published_problems(pool: &PgPool) -> Result<Vec<ProblemListItemDto>> {
    let rows = sqlx::query(
        r#"
        SELECT
            p.id AS problem_id,
            p.slug,
            p.title,
            p.description,
            pv.id AS version_id,
            pv.version
        FROM problems p
        JOIN LATERAL (
            SELECT id, version
            FROM problem_versions
            WHERE problem_id = p.id
              AND status = 'published'
            ORDER BY created_at DESC
            LIMIT 1
        ) pv ON TRUE
        WHERE p.status = 'active'
        ORDER BY p.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ProblemListItemDto {
            id: r.try_get("problem_id").unwrap_or_default(),
            slug: r.try_get("slug").unwrap_or_default(),
            title: r.try_get("title").unwrap_or_default(),
            description: r.try_get("description").unwrap_or_default(),
            current_version: crate::models::CurrentVersionDto {
                id: r.try_get("version_id").unwrap_or_default(),
                version: r.try_get("version").unwrap_or_default(),
            },
        })
        .collect())
}

/// Fetch one active problem by id or slug with its latest published version.
pub async fn get_published_problem(
    pool: &PgPool,
    problem_id_or_slug: &str,
) -> Result<Option<ProblemVersionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS problem_id,
            p.slug,
            p.title,
            p.description,
            pv.id AS version_id,
            pv.version,
            pv.bundle_path,
            pv.statement_path,
            pv.spec_json
        FROM problems p
        JOIN LATERAL (
            SELECT id, version, bundle_path, statement_path, spec_json
            FROM problem_versions
            WHERE problem_id = p.id
              AND status = 'published'
            ORDER BY created_at DESC
            LIMIT 1
        ) pv ON TRUE
        WHERE p.status = 'active'
          AND (p.id = $1 OR p.slug = $1)
        LIMIT 1
        "#,
    )
    .bind(problem_id_or_slug)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ProblemVersionRecord {
        problem_id: r.try_get("problem_id").unwrap_or_default(),
        slug: r.try_get("slug").unwrap_or_default(),
        title: r.try_get("title").unwrap_or_default(),
        description: r.try_get("description").unwrap_or_default(),
        problem_version_id: r.try_get("version_id").unwrap_or_default(),
        version: r.try_get("version").unwrap_or_default(),
        bundle_path: r.try_get("bundle_path").unwrap_or_default(),
        statement_path: r.try_get("statement_path").unwrap_or_default(),
        spec_json: r.try_get("spec_json").unwrap_or(Value::Null),
    }))
}

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
    let id: Option<String> = row.try_get(id_col.as_str()).ok();
    let id = match id {
        Some(i) if !i.is_empty() => i,
        _ => return Ok(None),
    };
    let status_str: Option<String> = row.try_get(format!("{}_status", prefix).as_str()).ok();
    let eval_type_str: Option<String> = row.try_get(format!("{}_eval_type", prefix).as_str()).ok();
    let primary_score: Option<f64> = row
        .try_get(format!("{}_primary_score", prefix).as_str())
        .ok();
    let shown_json: Option<Value> = row
        .try_get(format!("{}_shown_results", prefix).as_str())
        .ok();
    let hidden_json: Option<Value> = row
        .try_get(format!("{}_hidden_summary", prefix).as_str())
        .ok();
    let official_json: Option<Value> = row
        .try_get(format!("{}_official_summary", prefix).as_str())
        .ok();
    let log_path: Option<String> = row.try_get(format!("{}_log_path", prefix).as_str()).ok();
    let started_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_started_at", prefix).as_str()).ok();
    let finished_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_finished_at", prefix).as_str()).ok();

    let status = match status_str.as_deref() {
        Some("queued") => EvaluationStatus::Queued,
        Some("running") => EvaluationStatus::Running,
        Some("completed") => EvaluationStatus::Completed,
        Some("failed") => EvaluationStatus::Failed,
        _ => EvaluationStatus::Queued,
    };
    let eval_type = match eval_type_str.as_deref() {
        Some("official") => ScoringMode::Official,
        _ => ScoringMode::Public,
    };
    let shown_results: Vec<ShownCaseResult> = shown_json
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let hidden_summary: Option<ScoreSummary> =
        hidden_json.and_then(|v| serde_json::from_value(v).ok());
    let official_summary: Option<ScoreSummary> =
        official_json.and_then(|v| serde_json::from_value(v).ok());

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
        parent_submission_id: row.try_get("parent_submission_id").ok(),
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

    Ok(rows
        .into_iter()
        .map(|r| PublicSubmissionListItemDto {
            id: r.try_get("id").unwrap_or_default(),
            problem_id: r.try_get("problem_id").unwrap_or_default(),
            problem_version_id: r.try_get("problem_version_id").unwrap_or_default(),
            problem_title: r.try_get("problem_title").unwrap_or_default(),
            agent_id: r.try_get("agent_id").unwrap_or_default(),
            agent_name: r.try_get("agent_name").unwrap_or_default(),
            status: r.try_get("status").unwrap_or_default(),
            explanation: r.try_get("explanation").unwrap_or_default(),
            parent_submission_id: r.try_get("parent_submission_id").ok(),
            credit_text: r.try_get("credit_text").unwrap_or_default(),
            created_at: r
                .try_get::<DateTime<Utc>, _>("created_at")
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            updated_at: r
                .try_get::<DateTime<Utc>, _>("updated_at")
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            public_score: r.try_get("public_score").ok(),
            hidden_score: r.try_get("hidden_score").ok(),
            official_score: r.try_get("official_score").ok(),
        })
        .collect())
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

    Ok(rows
        .into_iter()
        .map(|r| LeaderboardEntryDto {
            agent_id: r.try_get("agent_id").unwrap_or_default(),
            agent_name: r.try_get("agent_name").unwrap_or_default(),
            best_submission_id: r.try_get("best_submission_id").unwrap_or_default(),
            best_hidden_score: r.try_get("best_hidden_score").unwrap_or(0.0),
            official_score: r.try_get("official_score").ok(),
            updated_at: r
                .try_get::<DateTime<Utc>, _>("updated_at")
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
        })
        .collect())
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

    let eval_type = match r.try_get::<String, _>("eval_type")?.as_str() {
        "official" => ScoringMode::Official,
        _ => ScoringMode::Public,
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
