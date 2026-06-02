//! Admin and pioneer-code HTTP handlers.

use super::{
    AdminAuth, AdminCapacityResponse, AdminServiceHeartbeatListResponse,
    AdminSolutionSubmissionListResponse, AppState, ChallengeName, CreatePioneerCodeRequest,
    DisableAgentResponse, EvaluationJobResponse, EvaluationJobStatus, Json, Path,
    PioneerCodeDetailResponse, PioneerCodeListResponse, QueueEvaluationJobRequest, Result,
    RevokePioneerCodeResponse, ScoringMode, ServiceError, SolutionSubmissionPath, State,
    StatusCode, ValidatedJson, challenge_metadata, evaluation_lifecycle, parse_request_value,
};
use agentics_domain::models::ids::{AgentId, AgentPioneerCodeId};
use agentics_domain::models::request::{
    ChallengeMoltbookDiscussionResponse, SetChallengeMoltbookDiscussionRequest,
};
use agentics_services::admin as admin_service;

// ---------------------------------------------------------------------------
// Admin routes
// ---------------------------------------------------------------------------

/// Create a pioneer code for MVP-gated agent registration.
pub async fn create_pioneer_code(
    admin: AdminAuth,
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<CreatePioneerCodeRequest>,
) -> Result<(StatusCode, Json<PioneerCodeDetailResponse>)> {
    Ok((
        StatusCode::CREATED,
        Json(
            admin_service::create_pioneer_code(&state.db, &state.config, admin.username, body)
                .await?,
        ),
    ))
}

/// List pioneer codes and their usage counts for admins.
pub async fn list_pioneer_codes(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<PioneerCodeListResponse>> {
    Ok(Json(admin_service::list_pioneer_codes(&state.db).await?))
}

/// Fetch one pioneer code with the agents created through it.
pub async fn get_pioneer_code(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PioneerCodeDetailResponse>> {
    let id =
        AgentPioneerCodeId::try_new(id).map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    Ok(Json(admin_service::get_pioneer_code(&state.db, &id).await?))
}

/// Revoke a pioneer code and disable all agents created through it.
pub async fn revoke_pioneer_code(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RevokePioneerCodeResponse>> {
    let id =
        AgentPioneerCodeId::try_new(id).map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    Ok(Json(
        admin_service::revoke_pioneer_code(&state.db, id).await?,
    ))
}

/// List challenge shells and published benchmark contracts for admins.
pub async fn list_admin_challenges(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<agentics_domain::models::challenge::AdminChallengeListResponse>> {
    Ok(Json(admin_service::list_admin_challenges(&state.db).await?))
}

/// Attach a Moltbook discussion post to a published challenge.
pub async fn set_challenge_moltbook_discussion(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
    ValidatedJson(body): ValidatedJson<SetChallengeMoltbookDiscussionRequest>,
) -> Result<Json<ChallengeMoltbookDiscussionResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    Ok(Json(
        challenge_metadata::set_challenge_moltbook_discussion(
            &state.db,
            &state.config,
            &challenge_name,
            &body.discussion_url,
        )
        .await?,
    ))
}

/// Clear the Moltbook discussion post from a published challenge.
pub async fn clear_challenge_moltbook_discussion(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
) -> Result<Json<ChallengeMoltbookDiscussionResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    Ok(Json(
        challenge_metadata::clear_challenge_moltbook_discussion(
            &state.db,
            &state.config,
            &challenge_name,
        )
        .await?,
    ))
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminSolutionSubmissionListResponse>> {
    Ok(Json(
        admin_service::list_admin_solution_submissions(&state.db).await?,
    ))
}

/// List latest service heartbeats for admin operations.
pub async fn list_admin_service_heartbeats(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminServiceHeartbeatListResponse>> {
    Ok(Json(
        admin_service::list_admin_service_heartbeats(&state.db).await?,
    ))
}

/// Show configured quota limits and current queue usage for admin capacity review.
pub async fn get_admin_capacity(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminCapacityResponse>> {
    Ok(Json(
        admin_service::get_admin_capacity(&state.db, &state.config).await?,
    ))
}

/// Queue an official rejudge for an existing solution submission.
pub async fn rejudge(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = evaluation_lifecycle::queue_solution_evaluation_job(
        &state.db,
        QueueEvaluationJobRequest {
            solution_submission_id: id,
            eval_type: ScoringMode::Official,
            max_active_official_jobs: Some(i64::from(state.config.quotas.max_active_official_jobs)),
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            solution_submission_id: job.solution_submission_id,
            target: job.target,
            eval_type: ScoringMode::Official,
            status: EvaluationJobStatus::from_storage_value(&job.status).ok_or_else(|| {
                ServiceError::Internal(format!(
                    "stored invalid evaluation job status `{}`",
                    job.status
                ))
            })?,
        }),
    ))
}

/// Queue an official private benchmark run for an existing solution submission.
pub async fn official_run(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = evaluation_lifecycle::queue_solution_evaluation_job(
        &state.db,
        QueueEvaluationJobRequest {
            solution_submission_id: id,
            eval_type: ScoringMode::Official,
            max_active_official_jobs: Some(i64::from(state.config.quotas.max_active_official_jobs)),
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            solution_submission_id: job.solution_submission_id,
            target: job.target,
            eval_type: ScoringMode::Official,
            status: EvaluationJobStatus::from_storage_value(&job.status).ok_or_else(|| {
                ServiceError::Internal(format!(
                    "stored invalid evaluation job status `{}`",
                    job.status
                ))
            })?,
        }),
    ))
}

/// Disable an agent and revoke its tokens.
pub async fn disable_agent(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DisableAgentResponse>> {
    let id = AgentId::try_new(id).map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    Ok(Json(admin_service::disable_agent(&state.db, id).await?))
}
