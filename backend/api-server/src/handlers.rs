//! HTTP handlers for the public, agent, admin, and health APIs.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use shared::auth;
use shared::challenge_bundle;
use shared::db::{self, QueueEvaluationJobInput};
use shared::error::{AppError, Result};
use shared::models::challenge::CreateChallengeVersionResponse;
use shared::models::evaluation::ScoringMode;
use shared::models::request::*;

use crate::extractors::{AdminAuth, AgentAuth, ValidatedJson};
use crate::presenters;
use crate::state::AppState;

const MAX_ARTIFACT_BYTES: u64 = 20 * 1024 * 1024;
const MAX_ARTIFACT_FILE_COUNT: usize = 256;
const MAX_ARTIFACT_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024;
const MAX_INLINE_TEXT_BYTES: u64 = 200_000;
const MAX_TOTAL_INLINE_TEXT_BYTES: u64 = 1_000_000;
const VALIDATION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

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

/// List published challenges for authenticated agents.
pub async fn list_agent_challenges(
    _agent: AgentAuth,
    State(state): State<AppState>,
) -> Result<Json<shared::models::challenge::ChallengeListResponse>> {
    let challenges = db::list_published_challenges(&state.db).await?;
    Ok(Json(shared::models::challenge::ChallengeListResponse {
        items: challenges,
    }))
}

/// Fetch challenge details for authenticated agents.
pub async fn get_agent_challenge(
    _agent: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, id).await
}

/// List published challenges on the public API.
pub async fn list_challenges(
    State(state): State<AppState>,
) -> Result<Json<shared::models::challenge::ChallengeListResponse>> {
    let challenges = db::list_published_challenges(&state.db).await?;
    Ok(Json(shared::models::challenge::ChallengeListResponse {
        items: challenges,
    }))
}

/// Fetch public challenge details by challenge id or slug.
pub async fn get_challenge(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, id).await
}

/// Shared challenge-detail response path used by public and agent routes.
async fn get_challenge_detail_response(
    state: AppState,
    id: String,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    let challenge = db::get_published_challenge(&state.db, &id).await?;
    let challenge = challenge.ok_or(AppError::NotFound)?;

    let statement = tokio::fs::read_to_string(&challenge.statement_path).await?;
    Ok(Json(presenters::present_challenge_detail(
        &challenge, &statement,
    )))
}

/// Create a ranking-visible solution submission, store its ZIP artifact, and queue official evaluation.
pub async fn create_solution_submission(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<CreateSolutionSubmissionRequest>,
) -> Result<(StatusCode, Json<CreateSolutionSubmissionResponse>)> {
    create_solution_submission_for_mode(state, agent, body, ScoringMode::Official).await
}

async fn create_solution_submission_for_mode(
    state: AppState,
    agent: AgentAuth,
    body: CreateSolutionSubmissionRequest,
    eval_type: ScoringMode,
) -> Result<(StatusCode, Json<CreateSolutionSubmissionResponse>)> {
    let challenge_id = body.challenge_id.trim().to_string();
    db::ensure_published_challenge_supports_eval_type(&state.db, &challenge_id, eval_type).await?;
    ensure_validation_quota_available(&state, &agent.agent_id, &challenge_id, eval_type).await?;

    let artifact_bytes = base64_decode(&body.artifact_base64).ok_or(AppError::Base64)?;
    if artifact_bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(AppError::BadRequest(format!(
            "artifact zip must be at most {} bytes",
            MAX_ARTIFACT_BYTES
        )));
    }

    if !is_likely_zip(&artifact_bytes) {
        return Err(AppError::BadRequest("artifact 必须是 zip 文件".to_string()));
    }

    let solution_submission_id = Uuid::new_v4().to_string();
    let artifact_path_rel = format!("solution-submissions/{}.zip", solution_submission_id);
    let artifact_path = state
        .storage
        .put(&artifact_path_rel, &artifact_bytes)
        .await?;

    let solution_submission = db::create_solution_submission_with_job(
        &state.db,
        &db::CreateSolutionSubmissionInput {
            solution_submission_id,
            job_id: Uuid::new_v4().to_string(),
            agent_id: agent.agent_id,
            challenge_id,
            artifact_path,
            eval_type,
            explanation: body.explanation.trim().to_string(),
            parent_solution_submission_id: body
                .parent_solution_submission_id
                .as_ref()
                .map(|s| s.trim().to_string()),
            credit_text: body.credit_text.trim().to_string(),
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_create_solution_submission(
            &solution_submission,
        )),
    ))
}

async fn ensure_validation_quota_available(
    state: &AppState,
    agent_id: &str,
    challenge_id: &str,
    eval_type: ScoringMode,
) -> Result<()> {
    if eval_type != ScoringMode::Validation {
        return Ok(());
    }

    let limit = i64::from(state.config.validation_runs_per_agent_challenge_day);
    let used = db::count_recent_validation_runs_for_agent_challenge(
        &state.db,
        agent_id,
        challenge_id,
        VALIDATION_QUOTA_WINDOW_SECONDS,
    )
    .await?;

    if used >= limit {
        return Err(AppError::TooManyRequests(format!(
            "validation quota exceeded for challenge `{challenge_id}`: {used} of {limit} validation runs used in the last 24 hours"
        )));
    }

    Ok(())
}

/// Create a private validation run, store its ZIP artifact, and queue validation evaluation.
pub async fn create_validation_run(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<CreateSolutionSubmissionRequest>,
) -> Result<(StatusCode, Json<CreateSolutionSubmissionResponse>)> {
    create_solution_submission_for_mode(state, agent, body, ScoringMode::Validation).await
}

/// Fetch an authenticated solution submission view with artifact and job metadata.
pub async fn get_solution_submission(
    State(state): State<AppState>,
    agent: AgentAuth,
    Path(id): Path<String>,
) -> Result<Json<SolutionSubmissionResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    Ok(Json(presenters::present_solution_submission(
        &solution_submission,
        true,
        true,
    )))
}

/// Fetch an authenticated validation run view owned by the caller.
pub async fn get_validation_run(
    State(state): State<AppState>,
    agent: AgentAuth,
    Path(id): Path<String>,
) -> Result<Json<SolutionSubmissionResponse>> {
    get_solution_submission(State(state), agent, Path(id)).await
}

/// Create a discussion thread as an authenticated agent.
pub async fn create_thread(
    State(state): State<AppState>,
    agent: AgentAuth,
    Path(challenge_id): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateDiscussionThreadRequest>,
) -> Result<(StatusCode, Json<shared::models::IdOnlyResponse>)> {
    let thread_id = Uuid::new_v4().to_string();
    db::create_discussion_thread(
        &state.db,
        &thread_id,
        &challenge_id,
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

/// List solution submissions that are visible after completed official evaluation.
pub async fn list_public_solution_submissions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PublicSolutionSubmissionListResponse>> {
    let items = db::list_public_solution_submissions_for_challenge(&state.db, &id).await?;
    Ok(Json(PublicSolutionSubmissionListResponse { items }))
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SolutionSubmissionResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    Ok(Json(presenters::present_solution_submission(
        &solution_submission,
        false,
        false,
    )))
}

/// Fetch a browsable artifact summary for a public solution submission.
pub async fn get_public_artifact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SolutionSubmissionArtifactResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }

    let artifact =
        read_solution_submission_artifact_summary(&solution_submission.artifact_path).await?;
    Ok(Json(artifact))
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LeaderboardResponse>> {
    let items = db::list_leaderboard_entries(&state.db, &id).await?;
    Ok(Json(LeaderboardResponse { items }))
}

/// Fetch discussion threads for a challenge.
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

/// Create or update a challenge shell.
pub async fn create_challenge(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<CreateChallengeRequest>,
) -> Result<(
    StatusCode,
    Json<shared::models::challenge::ChallengeAdminResponse>,
)> {
    let slug = body
        .slug
        .as_ref()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| body.id.trim().to_string());
    let challenge =
        db::create_or_update_challenge(&state.db, &body.id, &slug, &body.title, &body.description)
            .await
            .map_err(|e| match e {
                AppError::Database(sqlx::Error::Database(db_err))
                    if db_err.is_unique_violation() =>
                {
                    AppError::Conflict
                }
                _ => e,
            })?;
    Ok((StatusCode::CREATED, Json(challenge)))
}

/// Validate and publish a challenge bundle version.
pub async fn publish_version(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(challenge_id): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateChallengeVersionRequest>,
) -> Result<(StatusCode, Json<CreateChallengeVersionResponse>)> {
    let bundle_path = if std::path::Path::new(&body.bundle_path).is_absolute() {
        body.bundle_path
    } else {
        std::path::Path::new(&state.config.challenges_root)
            .join(&body.bundle_path)
            .to_string_lossy()
            .to_string()
    };

    challenge_bundle::validate_challenge_bundle(std::path::Path::new(&bundle_path)).await?;
    let spec =
        challenge_bundle::read_challenge_bundle_spec(std::path::Path::new(&bundle_path)).await?;

    if spec.challenge_id != challenge_id {
        return Err(AppError::BadRequest(format!(
            "challenge bundle id mismatch: expected {}, got {}",
            challenge_id, spec.challenge_id
        )));
    }

    let statement_path = std::path::Path::new(&bundle_path).join("statement.md");
    let description = challenge_bundle::extract_challenge_description(&statement_path).await?;

    let version = db::publish_challenge_version(
        &state.db,
        &challenge_id,
        &bundle_path,
        &statement_path.to_string_lossy(),
        &spec,
        &spec.challenge_title,
        &description,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(version)))
}

/// Queue an official rejudge for an existing solution submission.
pub async fn rejudge(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: Uuid::new_v4().to_string(),
            solution_submission_id: id.clone(),
            eval_type: ScoringMode::Official,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            solution_submission_id: job.solution_submission_id,
            eval_type: ScoringMode::Official.as_str().to_string(),
            status: job.status,
        }),
    ))
}

/// Queue an official private benchmark run for an existing solution submission.
pub async fn official_run(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: Uuid::new_v4().to_string(),
            solution_submission_id: id.clone(),
            eval_type: ScoringMode::Official,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            solution_submission_id: job.solution_submission_id,
            eval_type: ScoringMode::Official.as_str().to_string(),
            status: job.status,
        }),
    ))
}

/// Hide a solution submission from public views and repair leaderboard state.
pub async fn hide_solution_submission(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HideSolutionSubmissionResponse>> {
    db::hide_solution_submission(&state.db, &id).await?;
    Ok(Json(HideSolutionSubmissionResponse { id, hidden: true }))
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

/// Summarize a solution submission ZIP for safe public code browsing.
pub async fn read_solution_submission_artifact_summary(
    artifact_path: &str,
) -> Result<SolutionSubmissionArtifactResponse> {
    let archive_size = tokio::fs::metadata(artifact_path).await?.len();
    if archive_size > MAX_ARTIFACT_BYTES {
        return Err(AppError::BadRequest(format!(
            "artifact zip must be at most {} bytes",
            MAX_ARTIFACT_BYTES
        )));
    }

    let artifact_path = artifact_path.to_string();
    tokio::task::spawn_blocking(move || {
        read_solution_submission_artifact_summary_blocking(&artifact_path)
    })
    .await
    .map_err(|e| AppError::Internal(format!("artifact summary task failed: {e}")))?
}

fn read_solution_submission_artifact_summary_blocking(
    artifact_path: &str,
) -> Result<SolutionSubmissionArtifactResponse> {
    let archive_size = std::fs::metadata(artifact_path)?.len();
    let reader = std::fs::File::open(artifact_path)?;
    let mut archive = zip::ZipArchive::new(reader)?;

    if archive.len() > MAX_ARTIFACT_FILE_COUNT {
        return Err(AppError::BadRequest(format!(
            "artifact zip must contain at most {} entries",
            MAX_ARTIFACT_FILE_COUNT
        )));
    }

    let mut files = Vec::new();
    let mut total_uncompressed_size = 0u64;
    let mut total_inline_text_bytes = 0u64;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }

        let entry_path = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string());
        let Some(entry_path) = entry_path else {
            continue;
        };

        let size = file.size();
        total_uncompressed_size = total_uncompressed_size
            .checked_add(size)
            .ok_or_else(|| AppError::BadRequest("artifact zip is too large".to_string()))?;
        if total_uncompressed_size > MAX_ARTIFACT_UNCOMPRESSED_BYTES {
            return Err(AppError::BadRequest(format!(
                "artifact zip must expand to at most {} bytes",
                MAX_ARTIFACT_UNCOMPRESSED_BYTES
            )));
        }

        let mut buf = Vec::new();
        let compressed_size = file.compressed_size() as i64;
        let should_try_inline = size <= MAX_INLINE_TEXT_BYTES
            && total_inline_text_bytes + size <= MAX_TOTAL_INLINE_TEXT_BYTES;
        if should_try_inline {
            std::io::Read::read_to_end(&mut file, &mut buf)?;
        }

        let inline_text = if should_try_inline {
            std::str::from_utf8(&buf).ok()
        } else {
            None
        };
        let is_text = inline_text.is_some() || is_text_like_path(&entry_path);

        let content = if let Some(text) = inline_text {
            total_inline_text_bytes += buf.len() as u64;
            Some(text.to_string())
        } else {
            None
        };

        files.push(SolutionSubmissionArtifactFileDto {
            path: entry_path.clone(),
            size: size as i64,
            compressed_size,
            language: Some(infer_language(&entry_path)),
            is_text,
            content,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SolutionSubmissionArtifactResponse {
        archive_name: std::path::Path::new(artifact_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        archive_size: archive_size as i64,
        file_count: files.len() as i64,
        total_uncompressed_size: total_uncompressed_size as i64,
        files,
    })
}

fn is_text_like_path(file_path: &str) -> bool {
    !matches!(infer_language(file_path).as_str(), "plaintext")
        || matches!(
            std::path::Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("txt")
        )
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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use super::*;

    fn temp_zip_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-{name}-{}.zip", Uuid::new_v4()))
    }

    fn write_zip(path: &PathBuf, entries: Vec<(String, Vec<u8>)>) {
        let file = std::fs::File::create(path).expect("failed to create test zip");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            archive
                .start_file(name, options)
                .expect("failed to start zip entry");
            archive
                .write_all(&bytes)
                .expect("failed to write zip entry");
        }

        archive.finish().expect("failed to finish test zip");
    }

    #[tokio::test]
    async fn artifact_summary_skips_unsafe_entry_names() {
        let path = temp_zip_path("unsafe-entry");
        write_zip(
            &path,
            vec![
                ("../escape.py".to_string(), b"print('bad')\n".to_vec()),
                ("main.py".to_string(), b"print('ok')\n".to_vec()),
            ],
        );

        let summary = read_solution_submission_artifact_summary(&path.to_string_lossy())
            .await
            .expect("summary should succeed");
        let _ = std::fs::remove_file(path);

        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files[0].path, "main.py");
    }

    #[tokio::test]
    async fn artifact_summary_rejects_too_many_entries() {
        let path = temp_zip_path("too-many");
        let entries = (0..=MAX_ARTIFACT_FILE_COUNT)
            .map(|i| (format!("file-{i}.txt"), Vec::new()))
            .collect();
        write_zip(&path, entries);

        let result = read_solution_submission_artifact_summary(&path.to_string_lossy()).await;
        let _ = std::fs::remove_file(path);

        assert!(
            matches!(result, Err(AppError::BadRequest(message)) if message.contains("at most"))
        );
    }

    #[tokio::test]
    async fn artifact_summary_does_not_inline_large_text_entries() {
        let path = temp_zip_path("large-text");
        write_zip(
            &path,
            vec![(
                "main.py".to_string(),
                vec![b'a'; (MAX_INLINE_TEXT_BYTES + 1) as usize],
            )],
        );

        let summary = read_solution_submission_artifact_summary(&path.to_string_lossy())
            .await
            .expect("summary should succeed");
        let _ = std::fs::remove_file(path);

        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files[0].path, "main.py");
        assert!(summary.files[0].is_text);
        assert!(summary.files[0].content.is_none());
    }
}
