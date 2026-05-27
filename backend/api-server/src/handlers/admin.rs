//! Admin and pioneer-code HTTP handlers.

use chrono::{DateTime, Utc};

use super::{
    AdminAuth, AdminCapacityResponse, AdminServiceHeartbeatListResponse,
    AdminSolutionSubmissionListResponse, AgentId, AgentPioneerCodeId, AgentStatus, AppState,
    ChallengeName, CreatePioneerCodeRequest, DisableAgentResponse, EvaluationJobResponse,
    EvaluationJobStatus, Json, Path, PioneerCode, PioneerCodeDetailResponse,
    PioneerCodeListResponse, PioneerCodeStatus, QueueEvaluationJobRequest, Result,
    RevokePioneerCodeResponse, ScoringMode, ServiceError, SolutionSubmissionPath, State,
    StatusCode, ValidatedJson, auth, challenge_metadata, evaluation_lifecycle, parse_request_value,
    presenters,
};
use agentics_domain::models::request::{
    ChallengeMoltbookDiscussionResponse, SetChallengeMoltbookDiscussionRequest,
};
use agentics_persistence::{CreatePioneerCodeInput, Repositories};
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
    let CreatePioneerCodeRequest {
        label,
        code,
        note,
        max_uses,
        expires_at,
    } = body;

    if max_uses == 0 || max_uses < -1 {
        return Err(ServiceError::BadRequest(
            "max_uses must be a positive integer or -1 for local testing".to_string(),
        )
        .into());
    }
    if max_uses == -1 && !state.config.allows_local_registration_testing_knobs() {
        return Err(ServiceError::BadRequest(
            "unlimited pioneer codes are only allowed for loopback local testing".to_string(),
        )
        .into());
    }

    let (code_display, code_hash, label) = {
        let code = resolve_pioneer_code_request(code, label.as_deref())?;
        (
            code.expose_secret().to_string(),
            auth::hash_opaque_token(code.expose_secret()),
            code.label().map(ToOwned::to_owned),
        )
    };
    let expires_at = parse_optional_rfc3339(expires_at.as_deref(), "expires_at")?;
    let note = note.unwrap_or_default();
    let record = Repositories::new(&state.db)
        .pioneer_codes()
        .create(&CreatePioneerCodeInput {
            id: AgentPioneerCodeId::generate(),
            code_hash,
            code_display,
            label,
            note,
            max_uses,
            expires_at,
            created_by_admin_username: admin.username,
        })
        .await
        .map_err(ServiceError::unique_violation_as_conflict)?;

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_pioneer_code_detail(&record, &[])?),
    ))
}

/// List pioneer codes and their usage counts for admins.
pub async fn list_pioneer_codes(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<PioneerCodeListResponse>> {
    let codes = Repositories::new(&state.db).pioneer_codes().list().await?;
    Ok(Json(presenters::present_pioneer_code_list(&codes)?))
}

/// Fetch one pioneer code with the agents created through it.
pub async fn get_pioneer_code(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PioneerCodeDetailResponse>> {
    let id =
        AgentPioneerCodeId::try_new(id).map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    let (code, uses) = Repositories::new(&state.db)
        .pioneer_codes()
        .detail(&id)
        .await?;
    Ok(Json(presenters::present_pioneer_code_detail(&code, &uses)?))
}

/// Revoke a pioneer code and disable all agents created through it.
pub async fn revoke_pioneer_code(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RevokePioneerCodeResponse>> {
    let id =
        AgentPioneerCodeId::try_new(id).map_err(|e| ServiceError::BadRequest(e.to_string()))?;
    let outcome = Repositories::new(&state.db)
        .pioneer_codes()
        .revoke(&id)
        .await?;
    Ok(Json(RevokePioneerCodeResponse {
        id,
        status: PioneerCodeStatus::Revoked,
        revoked_agent_count: outcome.revoked_agent_count,
        revoked_token_count: outcome.revoked_token_count,
    }))
}

/// Resolve admin-supplied or generated pioneer code text.
fn resolve_pioneer_code_request(
    code: Option<PioneerCode>,
    label: Option<&str>,
) -> Result<PioneerCode> {
    if let Some(code) = code {
        if let Some(label) = label
            && code.label() != Some(label)
        {
            return Err(ServiceError::BadRequest(
                "label must match the pioneer code prefix when code is supplied".to_string(),
            )
            .into());
        }
        return Ok(code);
    }

    Ok(PioneerCode::generate(label).map_err(|e| ServiceError::BadRequest(e.to_string()))?)
}

/// Parse an optional RFC3339 timestamp from an API request field.
fn parse_optional_rfc3339(raw: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    Ok(raw
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|e| ServiceError::BadRequest(format!("{field} must be RFC3339: {e}")))
        })
        .transpose()?)
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
    Repositories::new(&state.db)
        .agents()
        .disable(id.as_str())
        .await?;
    Ok(Json(DisableAgentResponse {
        id,
        status: AgentStatus::Disabled,
    }))
}
