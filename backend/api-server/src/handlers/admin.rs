//! Admin and pioneer-code HTTP handlers.

use super::{
    AdminAuth, AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse, AgentId,
    AgentPioneerCodeId, AgentStatus, AppError, AppState, CreatePioneerCodeRequest, DateTime,
    DisableAgentResponse, EvaluationJobId, EvaluationJobResponse, EvaluationJobStatus, Json, Path,
    PioneerCode, PioneerCodeDetailResponse, PioneerCodeListResponse, PioneerCodeStatus,
    QueueEvaluationJobInput, Result, RevokePioneerCodeResponse, SUBMISSION_QUOTA_WINDOW_SECONDS,
    ScoringMode, SolutionSubmissionPath, State, StatusCode, Utc, ValidatedJson, auth, db,
    presenters,
};
use shared::models::challenge::PublishChallengeResponse;
use shared::models::request::{CreateChallengeRequest, PublishChallengeRequest};

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
        return Err(AppError::BadRequest(
            "max_uses must be a positive integer or -1 for local testing".to_string(),
        ));
    }
    if max_uses == -1 && !state.config.allows_local_registration_testing_knobs() {
        return Err(AppError::BadRequest(
            "unlimited pioneer codes are only allowed for loopback local testing".to_string(),
        ));
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
    let record = db::create_pioneer_code(
        &state.db,
        &db::CreatePioneerCodeInput {
            id: AgentPioneerCodeId::generate(),
            code_hash,
            code_display,
            label,
            note,
            max_uses,
            expires_at,
            created_by_admin_username: admin.username,
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
        Json(presenters::present_pioneer_code_detail(&record, &[])?),
    ))
}

/// List pioneer codes and their usage counts for admins.
pub async fn list_pioneer_codes(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<PioneerCodeListResponse>> {
    let codes = db::list_pioneer_codes(&state.db).await?;
    Ok(Json(presenters::present_pioneer_code_list(&codes)?))
}

/// Fetch one pioneer code with the agents created through it.
pub async fn get_pioneer_code(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PioneerCodeDetailResponse>> {
    let id = AgentPioneerCodeId::try_new(id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let (code, uses) = db::get_pioneer_code_detail(&state.db, &id).await?;
    Ok(Json(presenters::present_pioneer_code_detail(&code, &uses)?))
}

/// Revoke a pioneer code and disable all agents created through it.
pub async fn revoke_pioneer_code(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RevokePioneerCodeResponse>> {
    let id = AgentPioneerCodeId::try_new(id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let outcome = db::revoke_pioneer_code(&state.db, &id).await?;
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
            return Err(AppError::BadRequest(
                "label must match the pioneer code prefix when code is supplied".to_string(),
            ));
        }
        return Ok(code);
    }

    PioneerCode::generate(label).map_err(|e| AppError::BadRequest(e.to_string()))
}

/// Parse an optional RFC3339 timestamp from an API request field.
fn parse_optional_rfc3339(raw: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    raw.map(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|e| AppError::BadRequest(format!("{field} must be RFC3339: {e}")))
    })
    .transpose()
}

/// List challenge shells and published benchmark contracts for admins.
pub async fn list_admin_challenges(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<shared::models::challenge::AdminChallengeListResponse>> {
    let items = db::list_admin_challenges(&state.db).await?;
    Ok(Json(
        shared::models::challenge::AdminChallengeListResponse { items },
    ))
}

/// Create or update a challenge shell.
pub async fn create_challenge(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<CreateChallengeRequest>,
) -> Result<(
    StatusCode,
    Json<shared::models::challenge::ChallengeAdminResponse>,
)> {
    let challenge =
        db::create_or_update_challenge(&state.db, &body.name, &body.title, &body.summary)
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

/// Validate and publish a challenge bundle.
pub async fn publish_challenge(
    _admin: AdminAuth,
    State(_state): State<AppState>,
    Path(_challenge_name): Path<String>,
    ValidatedJson(_body): ValidatedJson<PublishChallengeRequest>,
) -> Result<(StatusCode, Json<PublishChallengeResponse>)> {
    Err(AppError::Forbidden(
        "direct admin bundle publishing is disabled for MVP; use the GitHub-backed challenge draft review flow"
            .to_string(),
    ))
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminSolutionSubmissionListResponse>> {
    let items = db::list_admin_solution_submissions(&state.db, 100).await?;
    Ok(Json(AdminSolutionSubmissionListResponse { items }))
}

/// List latest service heartbeats for admin operations.
pub async fn list_admin_service_heartbeats(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminServiceHeartbeatListResponse>> {
    let items = db::list_service_heartbeats(&state.db).await?;
    Ok(Json(AdminServiceHeartbeatListResponse { items }))
}

/// Show configured quota limits and current queue usage for admin capacity review.
pub async fn get_admin_capacity(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminCapacityResponse>> {
    let active_agents = db::count_active_agents(&state.db).await?;
    let active_validation_jobs =
        db::count_active_evaluation_jobs(&state.db, ScoringMode::Validation).await?;
    let active_official_jobs =
        db::count_active_evaluation_jobs(&state.db, ScoringMode::Official).await?;

    Ok(Json(AdminCapacityResponse {
        quota_window_seconds: SUBMISSION_QUOTA_WINDOW_SECONDS,
        quotas: AdminQuotaSettingsDto {
            validation_runs_per_agent_challenge_day: state
                .config
                .validation_runs_per_agent_challenge_day,
            official_runs_per_agent_challenge_day: state
                .config
                .official_runs_per_agent_challenge_day,
            max_active_official_jobs: state.config.max_active_official_jobs,
            max_active_agents: state.config.max_active_agents,
        },
        usage: AdminCapacityUsageDto {
            active_agents,
            active_validation_jobs,
            active_official_jobs,
        },
    }))
}

/// Queue an official rejudge for an existing solution submission.
pub async fn rejudge(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: EvaluationJobId::generate(),
            solution_submission_id: id,
            eval_type: ScoringMode::Official,
            max_active_official_jobs: Some(i64::from(state.config.max_active_official_jobs)),
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
                AppError::Internal(format!(
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
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: EvaluationJobId::generate(),
            solution_submission_id: id,
            eval_type: ScoringMode::Official,
            max_active_official_jobs: Some(i64::from(state.config.max_active_official_jobs)),
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
                AppError::Internal(format!(
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
    let id = AgentId::try_new(id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    db::disable_agent(&state.db, id.as_str()).await?;
    Ok(Json(DisableAgentResponse {
        id,
        status: AgentStatus::Disabled,
    }))
}
