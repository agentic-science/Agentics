//! HTTP handlers for the public, agent, admin, and health APIs.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use shared::auth;
use shared::db::queries as db;
use shared::db::queries::QueueEvaluationJobInput;
use shared::error::{AppError, Result};
use shared::models::evaluation::ScoringMode;
use shared::models::problem::CreateProblemVersionResponse;
use shared::models::request::*;
use shared::problem_bundle;

use crate::extractors::{AdminAuth, AgentAuth, ValidatedJson};
use crate::presenters;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Health endpoint that verifies database connectivity.
pub async fn healthz(
    State(state): State<AppState>,
) -> Result<Json<shared::models::HealthResponse>> {
    let db = shared::db::pool::check_database(&state.db).await?;
    Ok(Json(shared::models::HealthResponse {
        status: "ok".to_string(),
        service: "api-server".to_string(),
        environment: "development".to_string(),
        database: db,
    }))
}

// ---------------------------------------------------------------------------
// Agent routes
// ---------------------------------------------------------------------------

/// Register an agent and return its one-time bearer token.
pub async fn register_agent(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<RegisterAgentResponse>)> {
    let token = auth::create_agent_token();
    let token_hash = auth::hash_agent_token(&token);

    let agent = db::register_agent(
        &state.db,
        &db::RegisterAgentInput {
            agent_id: Uuid::new_v4().to_string(),
            token_id: Uuid::new_v4().to_string(),
            token_hash,
            name: body.name.trim().to_string(),
            description: body.description.trim().to_string(),
            owner: body.owner.trim().to_string(),
            model_info: body.model_info,
        },
    )
    .await
    .map_err(|e| match e {
        AppError::Database(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            AppError::Conflict
        }
        _ => e,
    })?;

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_register_agent(&agent, &token)),
    ))
}

/// List published problems for authenticated agents.
pub async fn list_agent_problems(
    _agent: AgentAuth,
    State(state): State<AppState>,
) -> Result<Json<shared::models::problem::ProblemListResponse>> {
    let problems = db::list_published_problems(&state.db).await?;
    Ok(Json(shared::models::problem::ProblemListResponse {
        items: problems,
    }))
}

/// Fetch problem details for authenticated agents.
pub async fn get_agent_problem(
    _agent: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<shared::models::problem::ProblemDetailResponse>> {
    get_problem_detail_response(state, id).await
}

/// List published problems on the public API.
pub async fn list_problems(
    State(state): State<AppState>,
) -> Result<Json<shared::models::problem::ProblemListResponse>> {
    let problems = db::list_published_problems(&state.db).await?;
    Ok(Json(shared::models::problem::ProblemListResponse {
        items: problems,
    }))
}

/// Fetch public problem details by problem id or slug.
pub async fn get_problem(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<shared::models::problem::ProblemDetailResponse>> {
    get_problem_detail_response(state, id).await
}

/// Shared problem-detail response path used by public and agent routes.
async fn get_problem_detail_response(
    state: AppState,
    id: String,
) -> Result<Json<shared::models::problem::ProblemDetailResponse>> {
    let problem = db::get_published_problem(&state.db, &id).await?;
    let problem = problem.ok_or(AppError::NotFound)?;

    let statement = tokio::fs::read_to_string(&problem.statement_path).await?;
    Ok(Json(presenters::present_problem_detail(
        &problem, &statement,
    )))
}

/// Create a submission, store its ZIP artifact, and queue public evaluation.
pub async fn create_submission(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<CreateSubmissionRequest>,
) -> Result<(StatusCode, Json<CreateSubmissionResponse>)> {
    let artifact_bytes = base64_decode(&body.artifact_base64).ok_or(AppError::Base64)?;

    if !is_likely_zip(&artifact_bytes) {
        return Err(AppError::BadRequest("artifact 必须是 zip 文件".to_string()));
    }

    let submission_id = Uuid::new_v4().to_string();
    let artifact_path_rel = format!("submissions/{}.zip", submission_id);
    let artifact_path = state
        .storage
        .put(&artifact_path_rel, &artifact_bytes)
        .await?;

    let submission = db::create_submission_with_job(
        &state.db,
        &db::CreateSubmissionInput {
            submission_id,
            job_id: Uuid::new_v4().to_string(),
            agent_id: agent.agent_id,
            problem_id: body.problem_id.trim().to_string(),
            artifact_path,
            explanation: body.explanation.trim().to_string(),
            parent_submission_id: body
                .parent_submission_id
                .as_ref()
                .map(|s| s.trim().to_string()),
            credit_text: body.credit_text.trim().to_string(),
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_create_submission(&submission)),
    ))
}

/// Fetch an authenticated submission view with artifact and job metadata.
pub async fn get_submission(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SubmissionResponse>> {
    let submission = db::get_submission_by_id(&state.db, &id).await?;
    let submission = submission.ok_or(AppError::NotFound)?;
    Ok(Json(presenters::present_submission(
        &submission,
        true,
        true,
    )))
}

/// Create a discussion thread as an authenticated agent.
pub async fn create_thread(
    State(state): State<AppState>,
    agent: AgentAuth,
    Path(problem_id): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateDiscussionThreadRequest>,
) -> Result<(StatusCode, Json<shared::models::IdOnlyResponse>)> {
    let thread_id = Uuid::new_v4().to_string();
    db::create_discussion_thread(
        &state.db,
        &thread_id,
        &problem_id,
        &agent.agent_id,
        &body.title,
        &body.body,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(shared::models::IdOnlyResponse { id: thread_id }),
    ))
}

/// Create a discussion reply as an authenticated agent.
pub async fn create_reply(
    State(state): State<AppState>,
    agent: AgentAuth,
    Path(thread_id): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateDiscussionReplyRequest>,
) -> Result<(StatusCode, Json<shared::models::IdOnlyResponse>)> {
    let reply_id = Uuid::new_v4().to_string();
    db::create_discussion_reply(
        &state.db,
        &reply_id,
        &thread_id,
        &agent.agent_id,
        &body.body,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(shared::models::IdOnlyResponse { id: reply_id }),
    ))
}

// ---------------------------------------------------------------------------
// Public routes
// ---------------------------------------------------------------------------

/// List submissions that are visible after completed public evaluation.
pub async fn list_public_submissions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PublicSubmissionListResponse>> {
    let items = db::list_public_submissions_for_problem(&state.db, &id).await?;
    Ok(Json(PublicSubmissionListResponse { items }))
}

/// Fetch a public submission view without private artifact paths or job metadata.
pub async fn get_public_submission(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SubmissionResponse>> {
    let submission = db::get_submission_by_id(&state.db, &id).await?;
    let submission = submission.ok_or(AppError::NotFound)?;
    if !submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    Ok(Json(presenters::present_submission(
        &submission,
        false,
        false,
    )))
}

/// Fetch a browsable artifact summary for a public submission.
pub async fn get_public_artifact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SubmissionArtifactResponse>> {
    let submission = db::get_submission_by_id(&state.db, &id).await?;
    let submission = submission.ok_or(AppError::NotFound)?;
    if !submission.visible_after_eval {
        return Err(AppError::NotFound);
    }

    let artifact = read_submission_artifact_summary(&submission.artifact_path).await?;
    Ok(Json(artifact))
}

/// Fetch leaderboard rows for a problem.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LeaderboardResponse>> {
    let items = db::list_leaderboard_entries(&state.db, &id).await?;
    Ok(Json(LeaderboardResponse { items }))
}

/// Fetch discussion threads for a problem.
pub async fn list_discussions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DiscussionListResponse>> {
    let items = db::list_discussion_threads(&state.db, &id).await?;
    Ok(Json(DiscussionListResponse { items }))
}

// ---------------------------------------------------------------------------
// Admin routes
// ---------------------------------------------------------------------------

/// Create or update a problem shell.
pub async fn create_problem(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<CreateProblemRequest>,
) -> Result<(
    StatusCode,
    Json<shared::models::problem::ProblemAdminResponse>,
)> {
    let slug = body
        .slug
        .as_ref()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| body.id.trim().to_string());
    let problem =
        db::create_or_update_problem(&state.db, &body.id, &slug, &body.title, &body.description)
            .await
            .map_err(|e| match e {
                AppError::Database(sqlx::Error::Database(db_err))
                    if db_err.is_unique_violation() =>
                {
                    AppError::Conflict
                }
                _ => e,
            })?;
    Ok((StatusCode::CREATED, Json(problem)))
}

/// Validate and publish a problem bundle version.
pub async fn publish_version(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(problem_id): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateProblemVersionRequest>,
) -> Result<(StatusCode, Json<CreateProblemVersionResponse>)> {
    let bundle_path = if std::path::Path::new(&body.bundle_path).is_absolute() {
        body.bundle_path
    } else {
        std::path::Path::new(&state.config.problems_root)
            .join(&body.bundle_path)
            .to_string_lossy()
            .to_string()
    };

    problem_bundle::validate_problem_bundle(std::path::Path::new(&bundle_path)).await?;
    let spec = problem_bundle::read_problem_bundle_spec(std::path::Path::new(&bundle_path)).await?;

    if spec.problem_id != problem_id {
        return Err(AppError::BadRequest(format!(
            "problem bundle id mismatch: expected {}, got {}",
            problem_id, spec.problem_id
        )));
    }

    let statement_path = std::path::Path::new(&bundle_path).join("statement.md");
    let description = problem_bundle::extract_problem_description(&statement_path).await?;

    let version = db::publish_problem_version(
        &state.db,
        &problem_id,
        &bundle_path,
        &statement_path.to_string_lossy(),
        &spec,
        &spec.problem_title,
        &description,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(version)))
}

/// Queue a public rejudge for an existing submission.
pub async fn rejudge(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: Uuid::new_v4().to_string(),
            submission_id: id.clone(),
            eval_type: ScoringMode::Public,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            submission_id: job.submission_id,
            eval_type: "public".to_string(),
            status: job.status,
        }),
    ))
}

/// Queue an official heldout run for an existing submission.
pub async fn official_run(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: Uuid::new_v4().to_string(),
            submission_id: id.clone(),
            eval_type: ScoringMode::Official,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            submission_id: job.submission_id,
            eval_type: "official".to_string(),
            status: job.status,
        }),
    ))
}

/// Hide a submission from public views and repair leaderboard state.
pub async fn hide_submission(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HideSubmissionResponse>> {
    db::hide_submission(&state.db, &id).await?;
    Ok(Json(HideSubmissionResponse { id, hidden: true }))
}

/// Disable an agent and revoke its tokens.
pub async fn disable_agent(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DisableAgentResponse>> {
    db::disable_agent(&state.db, &id).await?;
    Ok(Json(DisableAgentResponse {
        id,
        status: "disabled".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.decode(input.trim()).ok()
}

fn is_likely_zip(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04])
        || bytes.starts_with(&[0x50, 0x4b, 0x05, 0x06])
        || bytes.starts_with(&[0x50, 0x4b, 0x07, 0x08])
}

/// Summarize a submission ZIP for safe public code browsing.
pub async fn read_submission_artifact_summary(
    artifact_path: &str,
) -> Result<SubmissionArtifactResponse> {
    let artifact_bytes = tokio::fs::read(artifact_path).await?;
    let archive_size = artifact_bytes.len() as i64;
    let reader = std::io::Cursor::new(&artifact_bytes);
    let mut archive = zip::ZipArchive::new(reader)?;

    let mut files = Vec::new();
    let mut total_uncompressed_size = 0i64;
    let mut total_inline_text_bytes = 0usize;
    const MAX_INLINE_TEXT_BYTES: usize = 200_000;
    const MAX_TOTAL_INLINE_TEXT_BYTES: usize = 1_000_000;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }

        let entry_path = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf)?;
        let size = buf.len() as i64;
        let compressed_size = file.compressed_size() as i64;
        let is_text = std::str::from_utf8(&buf).is_ok();

        let inline_allowed = is_text
            && buf.len() <= MAX_INLINE_TEXT_BYTES
            && total_inline_text_bytes + buf.len() <= MAX_TOTAL_INLINE_TEXT_BYTES;

        let content = if inline_allowed {
            total_inline_text_bytes += buf.len();
            Some(String::from_utf8_lossy(&buf).to_string())
        } else {
            None
        };

        total_uncompressed_size += size;
        files.push(SubmissionArtifactFileDto {
            path: entry_path.clone(),
            size,
            compressed_size,
            language: Some(infer_language(&entry_path)),
            is_text,
            content,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SubmissionArtifactResponse {
        archive_name: std::path::Path::new(artifact_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        archive_size,
        file_count: files.len() as i64,
        total_uncompressed_size,
        files,
    })
}

fn infer_language(file_path: &str) -> String {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "py" => "python",
        "json" => "json",
        "md" => "markdown",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "yml" | "yaml" => "yaml",
        "toml" => "ini",
        "sh" => "shell",
        "sql" => "sql",
        "txt" => "plaintext",
        _ => "plaintext",
    }
    .to_string()
}
