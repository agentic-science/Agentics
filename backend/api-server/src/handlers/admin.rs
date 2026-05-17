//! Admin and pioneer-code HTTP handlers.

use super::{
    AdminAuth, AdminBundlePath, AdminCapacityResponse, AdminCapacityUsageDto,
    AdminQuotaSettingsDto, AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    AgentId, AgentPioneerCodeId, AgentStatus, AppError, AppState, ChallengeEligibilityType,
    ChallengeName, Config, CreateChallengeRequest, CreatePioneerCodeRequest, DateTime,
    DisableAgentResponse, EvaluationJobId, EvaluationJobResponse, EvaluationJobStatus, FsPath,
    HideSolutionSubmissionResponse, Json, Path, PathBuf, PioneerCode, PioneerCodeDetailResponse,
    PioneerCodeListResponse, PioneerCodeStatus, PublishChallengeRequest, PublishChallengeResponse,
    QueueEvaluationJobInput, Result, RevokePioneerCodeResponse, SUBMISSION_QUOTA_WINDOW_SECONDS,
    ScoringMode, SolutionSubmissionPath, State, StatusCode, Utc, Uuid, ValidatedJson, auth,
    challenge_bundle, db, parse_request_value, presenters,
};

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
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
    ValidatedJson(body): ValidatedJson<PublishChallengeRequest>,
) -> Result<(StatusCode, Json<PublishChallengeResponse>)> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    let bundle_path = if FsPath::new(&body.bundle_path).is_absolute() {
        PathBuf::from(&body.bundle_path)
    } else {
        FsPath::new(&state.config.challenges_root).join(&body.bundle_path)
    };
    let bundle_path = AdminBundlePath::from_existing_dir(&bundle_path)?;

    challenge_bundle::validate_challenge_bundle(bundle_path.as_path()).await?;
    let spec = challenge_bundle::read_challenge_bundle_spec(bundle_path.as_path()).await?;
    if state.config.require_digest_pinned_images {
        challenge_bundle::validate_digest_pinned_images(&spec)?;
    }

    if spec.challenge_name != challenge_name {
        return Err(AppError::BadRequest(format!(
            "challenge bundle id mismatch: expected {}, got {}",
            challenge_name, spec.challenge_name
        )));
    }
    if spec.eligibility.eligibility_type == ChallengeEligibilityType::PrivateShortlist {
        return Err(AppError::BadRequest(
            "private_shortlist challenges must be published through the creator draft flow so an owner can manage the shortlist"
                .to_string(),
        ));
    }

    let managed_bundle_path =
        copy_admin_bundle_to_managed_storage(&state.config, bundle_path.as_path()).await?;
    let statement_path = managed_bundle_path.join("statement.md");
    let managed_bundle_path =
        shared::models::paths::ManagedBundlePath::from_existing_dir(&managed_bundle_path)?;
    let statement_path =
        shared::models::paths::ManagedStatementPath::from_existing_file(&statement_path)?;

    let challenge = db::publish_challenge(
        &state.db,
        &challenge_name,
        &managed_bundle_path,
        &statement_path,
        &spec,
        &spec.challenge_title,
        &spec.challenge_summary,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(challenge)))
}

/// Copies an admin-supplied bundle into content-addressed managed storage.
async fn copy_admin_bundle_to_managed_storage(
    config: &Config,
    source: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let source_digest = challenge_bundle::challenge_bundle_tree_sha256(source).await?;
    let target = std::path::Path::new(&config.storage_root)
        .join("challenge-bundles")
        .join("admin")
        .join(source_digest.to_string());
    if !tokio::fs::try_exists(&target).await? {
        let temp_target = target.with_extension(format!("tmp-{}", Uuid::new_v4()));
        if tokio::fs::try_exists(&temp_target).await? {
            tokio::fs::remove_dir_all(&temp_target).await?;
        }
        challenge_bundle::copy_challenge_bundle_dir(source, &temp_target, true).await?;
        let temp_digest = challenge_bundle::challenge_bundle_tree_sha256(&temp_target).await?;
        if temp_digest != source_digest {
            tokio::fs::remove_dir_all(&temp_target).await.ok();
            return Err(AppError::Validation(format!(
                "managed bundle temporary copy digest mismatch for {}",
                temp_target.display()
            )));
        }
        match tokio::fs::rename(&temp_target, &target).await {
            Ok(()) => {}
            Err(error) if tokio::fs::try_exists(&target).await? => {
                tokio::fs::remove_dir_all(&temp_target).await.ok();
                let target_digest = challenge_bundle::challenge_bundle_tree_sha256(&target).await?;
                if target_digest != source_digest {
                    return Err(AppError::Validation(format!(
                        "managed bundle target digest mismatch for {}",
                        target.display()
                    )));
                }
                tracing::debug!(
                    target = %target.display(),
                    error = %error,
                    "managed bundle target already exists after concurrent copy"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }

    challenge_bundle::validate_challenge_bundle(&target).await?;
    let managed_digest = challenge_bundle::challenge_bundle_tree_sha256(&target).await?;
    if managed_digest != source_digest {
        return Err(AppError::Validation(format!(
            "managed bundle copy digest mismatch for {}",
            target.display()
        )));
    }

    Ok(target)
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

/// Hide a solution submission from public views and repair leaderboard state.
pub async fn hide_solution_submission(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
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
    let id = AgentId::try_new(id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    db::disable_agent(&state.db, id.as_str()).await?;
    Ok(Json(DisableAgentResponse {
        id,
        status: AgentStatus::Disabled,
    }))
}
