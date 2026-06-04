//! Admin read and capacity workflows.

use agentics_config::Config;
use agentics_domain::models::auth::{
    AdminHumanDto, AdminHumanListResponse, AdminHumanRoleResponse,
    AdminServiceTokenCreatedResponse, AdminServiceTokenDto, AdminServiceTokenListResponse,
    CreateAdminServiceTokenRequest, HumanStatus, RevokeAdminServiceTokenResponse,
};
use agentics_domain::models::challenge::{
    AdminChallengeListItemDto, AdminChallengeListResponse, ChallengeBundleSpec,
};
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AdminServiceTokenId, HumanId, PioneerCodeId};
use agentics_domain::models::pioneer_codes::{PioneerCode, PioneerCodeStatus, PioneerCodeUseKind};
use agentics_domain::models::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto, AdminServiceHeartbeatDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListItemDto,
    AdminSolutionSubmissionListResponse, AgentStatus, CreatePioneerCodeRequest,
    DisableAgentResponse, PioneerCodeDetailResponse, PioneerCodeDto, PioneerCodeListResponse,
    PioneerCodeUseDto, RevokePioneerCodeResponse,
};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{
    AdminChallengeListItemRecord, AdminServiceTokenRecord, AdminSolutionSubmissionListItemRecord,
    CreateAdminServiceTokenInput, CreatePioneerCodeInput, HumanRecord, PioneerCodeRecord,
    PioneerCodeUseRecord, Repositories,
};

use crate::auth;

const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// Admin actor metadata needed by service-level audit and creation records.
#[derive(Debug, Clone)]
pub enum AdminActorInput {
    Human {
        human_id: HumanId,
        display: String,
    },
    ServiceToken {
        token_id: AdminServiceTokenId,
        display: String,
    },
}

impl AdminActorInput {
    fn human_id(&self) -> Option<HumanId> {
        match self {
            Self::Human { human_id, .. } => Some(human_id.clone()),
            Self::ServiceToken { .. } => None,
        }
    }

    fn service_token_id(&self) -> Option<AdminServiceTokenId> {
        match self {
            Self::Human { .. } => None,
            Self::ServiceToken { token_id, .. } => Some(token_id.clone()),
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Human { display, .. } | Self::ServiceToken { display, .. } => display.clone(),
        }
    }
}

/// Create a pioneer code for MVP-gated agent registration.
pub async fn create_pioneer_code(
    pool: &sqlx::PgPool,
    config: &Config,
    actor: AdminActorInput,
    body: CreatePioneerCodeRequest,
) -> Result<PioneerCodeDetailResponse> {
    let CreatePioneerCodeRequest {
        label,
        note,
        max_uses,
        expires_at,
    } = body;

    if max_uses == 0 || max_uses < -1 {
        return Err(ServiceError::BadRequest(
            "max_uses must be a positive integer or -1 for local testing".to_string(),
        ));
    }
    if max_uses == -1 && !config.allows_local_registration_testing_knobs() {
        return Err(ServiceError::BadRequest(
            "unlimited pioneer codes are only allowed for loopback local testing".to_string(),
        ));
    }

    let code = generate_pioneer_code(label.as_deref())?;
    let record = Repositories::new(pool)
        .pioneer_codes()
        .create(&CreatePioneerCodeInput {
            id: PioneerCodeId::generate(),
            code_hash: auth::hash_opaque_token(code.expose_secret()),
            code_display: code.expose_secret().to_string(),
            label: code.label().map(ToOwned::to_owned),
            note: note.unwrap_or_default(),
            max_uses,
            expires_at: parse_optional_rfc3339(expires_at.as_deref(), "expires_at")?,
            created_by_human_id: actor.human_id(),
            created_by_admin_service_token_id: actor.service_token_id(),
            created_by_display: actor.display(),
        })
        .await
        .map_err(ServiceError::unique_violation_as_conflict)?;

    present_pioneer_code_detail(&record, &[])
}

/// List pioneer codes and usage counts for admins.
pub async fn list_pioneer_codes(pool: &sqlx::PgPool) -> Result<PioneerCodeListResponse> {
    let codes = Repositories::new(pool).pioneer_codes().list().await?;
    present_pioneer_code_list(&codes)
}

/// Fetch one pioneer code with the agents created through it.
pub async fn get_pioneer_code(
    pool: &sqlx::PgPool,
    id: &PioneerCodeId,
) -> Result<PioneerCodeDetailResponse> {
    let (code, uses) = Repositories::new(pool).pioneer_codes().detail(id).await?;
    present_pioneer_code_detail(&code, &uses)
}

/// Revoke a pioneer code and disable all agents created through it.
pub async fn revoke_pioneer_code(
    pool: &sqlx::PgPool,
    id: PioneerCodeId,
) -> Result<RevokePioneerCodeResponse> {
    let outcome = Repositories::new(pool).pioneer_codes().revoke(&id).await?;
    Ok(RevokePioneerCodeResponse {
        id,
        status: PioneerCodeStatus::Revoked,
        revoked_human_count: outcome.revoked_human_count,
        revoked_human_session_count: outcome.revoked_human_session_count,
        revoked_admin_service_token_count: outcome.revoked_admin_service_token_count,
        revoked_agent_count: outcome.revoked_agent_count,
        revoked_token_count: outcome.revoked_token_count,
    })
}

/// List human accounts and roles for admin identity management.
pub async fn list_humans(pool: &sqlx::PgPool) -> Result<AdminHumanListResponse> {
    let items = Repositories::new(pool)
        .sessions()
        .list_humans()
        .await?
        .into_iter()
        .map(present_human)
        .collect::<Result<Vec<_>>>()?;
    Ok(AdminHumanListResponse { items })
}

/// Grant a human account the admin role.
pub async fn grant_human_admin_role(
    pool: &sqlx::PgPool,
    target_human_id: &HumanId,
    granted_by_human_id: &HumanId,
) -> Result<AdminHumanRoleResponse> {
    let human = Repositories::new(pool)
        .sessions()
        .grant_admin_role(target_human_id, granted_by_human_id)
        .await?;
    Ok(AdminHumanRoleResponse {
        human: present_human(human)?,
    })
}

/// Revoke a human account's admin role.
pub async fn revoke_human_admin_role(
    pool: &sqlx::PgPool,
    target_human_id: &HumanId,
    revoked_by_human_id: &HumanId,
) -> Result<AdminHumanRoleResponse> {
    let human = Repositories::new(pool)
        .sessions()
        .revoke_admin_role(target_human_id, revoked_by_human_id)
        .await?;
    Ok(AdminHumanRoleResponse {
        human: present_human(human)?,
    })
}

/// Create an admin service token for non-browser automation.
pub async fn create_admin_service_token(
    pool: &sqlx::PgPool,
    human_id: &HumanId,
    body: CreateAdminServiceTokenRequest,
) -> Result<AdminServiceTokenCreatedResponse> {
    let expires_at = parse_optional_rfc3339(body.expires_at.as_deref(), "expires_at")?;
    let token = auth::create_admin_service_token();
    let record = Repositories::new(pool)
        .sessions()
        .create_admin_service_token(&CreateAdminServiceTokenInput {
            id: AdminServiceTokenId::generate(),
            token_hash: auth::hash_opaque_token(&token),
            label: body.label.trim().to_string(),
            created_by_human_id: human_id.clone(),
            expires_at,
        })
        .await?;
    Ok(AdminServiceTokenCreatedResponse {
        token,
        token_record: present_admin_service_token(record),
    })
}

/// List admin service tokens.
pub async fn list_admin_service_tokens(
    pool: &sqlx::PgPool,
) -> Result<AdminServiceTokenListResponse> {
    let items = Repositories::new(pool)
        .sessions()
        .list_admin_service_tokens()
        .await?
        .into_iter()
        .map(present_admin_service_token)
        .collect();
    Ok(AdminServiceTokenListResponse { items })
}

/// Revoke one admin service token.
pub async fn revoke_admin_service_token(
    pool: &sqlx::PgPool,
    id: &AdminServiceTokenId,
    revoked_by_human_id: &HumanId,
) -> Result<RevokeAdminServiceTokenResponse> {
    let record = Repositories::new(pool)
        .sessions()
        .revoke_admin_service_token(id, revoked_by_human_id)
        .await?;
    Ok(RevokeAdminServiceTokenResponse {
        token_record: present_admin_service_token(record),
    })
}

/// Disable an agent account and revoke its active tokens.
pub async fn disable_agent(
    pool: &sqlx::PgPool,
    id: agentics_domain::models::ids::AgentId,
) -> Result<DisableAgentResponse> {
    Repositories::new(pool)
        .agents()
        .disable(id.as_str())
        .await?;
    Ok(DisableAgentResponse {
        id,
        status: AgentStatus::Disabled,
    })
}

/// List challenge shells and published benchmark contracts for admins.
pub async fn list_admin_challenges(pool: &sqlx::PgPool) -> Result<AdminChallengeListResponse> {
    let items = Repositories::new(pool)
        .challenges()
        .list_admin()
        .await?
        .into_iter()
        .map(admin_challenge_list_item_from_record)
        .collect::<Result<Vec<_>>>()?;
    Ok(AdminChallengeListResponse { items })
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    pool: &sqlx::PgPool,
) -> Result<AdminSolutionSubmissionListResponse> {
    let items = Repositories::new(pool)
        .solution_submissions()
        .list_admin(100)
        .await?
        .into_iter()
        .map(admin_solution_submission_list_item_from_record)
        .collect();
    Ok(AdminSolutionSubmissionListResponse { items })
}

fn admin_solution_submission_list_item_from_record(
    record: AdminSolutionSubmissionListItemRecord,
) -> AdminSolutionSubmissionListItemDto {
    AdminSolutionSubmissionListItemDto {
        id: record.id,
        challenge_name: record.challenge_name,
        challenge_title: record.challenge_title,
        target: record.target,
        agent_id: record.agent_id,
        agent_display_name: record.agent_display_name,
        status: record.status,
        note: record.note,
        visible_after_eval: record.visible_after_eval,
        latest_job_id: record.latest_job_id,
        latest_job_status: record.latest_job_status,
        latest_job_eval_type: record.latest_job_eval_type,
        validation_status: record.validation_status,
        official_status: record.official_status,
        rank_score: record.rank_score,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn admin_challenge_list_item_from_record(
    record: AdminChallengeListItemRecord,
) -> Result<AdminChallengeListItemDto> {
    let spec = record
        .spec_json
        .map(serde_json::from_value::<ChallengeBundleSpec>)
        .transpose()
        .map_err(|error| ServiceError::Internal(error.to_string()))?;

    Ok(AdminChallengeListItemDto {
        challenge_name: record.challenge_name,
        title: record.title,
        summary: record.summary,
        keywords: spec
            .as_ref()
            .map(|challenge_spec| challenge_spec.keywords.clone())
            .unwrap_or_default(),
        status: record.status,
        targets: spec.as_ref().map(|spec| spec.targets.clone()),
        starts_at: spec.as_ref().map(|spec| spec.starts_at.clone()),
        closes_at: spec.as_ref().and_then(|spec| spec.closes_at.clone()),
        eligibility: spec.as_ref().map(|spec| spec.eligibility.clone()),
        visibility: spec.as_ref().map(|spec| spec.visibility.clone()),
        solution_publication: spec.as_ref().map(|spec| spec.solution_publication),
        private_benchmark_enabled: spec
            .as_ref()
            .map(|spec| spec.datasets.private_benchmark_enabled),
        moltbook_discussion_url: record.moltbook_discussion_url,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    })
}

/// List latest service heartbeats for admin operations.
pub async fn list_admin_service_heartbeats(
    pool: &sqlx::PgPool,
) -> Result<AdminServiceHeartbeatListResponse> {
    let items = Repositories::new(pool)
        .maintenance()
        .list_service_heartbeats()
        .await?
        .into_iter()
        .map(|record| AdminServiceHeartbeatDto {
            service_name: record.service_name,
            last_seen_at: record.last_seen_at.to_rfc3339(),
            payload: record.payload,
        })
        .collect();
    Ok(AdminServiceHeartbeatListResponse { items })
}

/// Show configured quota limits and current queue usage for admin capacity review.
pub async fn get_admin_capacity(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<AdminCapacityResponse> {
    let repos = Repositories::new(pool);
    let active_agents = repos.agents().count_active().await?;
    let active_validation_jobs = repos
        .evaluation_jobs()
        .count_active(ScoringMode::Validation)
        .await?;
    let active_official_jobs = repos
        .evaluation_jobs()
        .count_active(ScoringMode::Official)
        .await?;

    Ok(AdminCapacityResponse {
        quota_window_seconds: SUBMISSION_QUOTA_WINDOW_SECONDS,
        quotas: AdminQuotaSettingsDto {
            validation_runs_per_agent_challenge_day: config
                .quotas
                .validation_runs_per_agent_challenge_day,
            official_runs_per_agent_challenge_day: config
                .quotas
                .official_runs_per_agent_challenge_day,
            max_active_official_jobs: config.quotas.max_active_official_jobs,
            max_active_agents: config.quotas.max_active_agents,
        },
        usage: AdminCapacityUsageDto {
            active_agents,
            active_validation_jobs,
            active_official_jobs,
        },
    })
}

/// Generate pioneer code text from the optional admin-selected label.
fn generate_pioneer_code(label: Option<&str>) -> Result<PioneerCode> {
    PioneerCode::generate(label).map_err(|error| ServiceError::BadRequest(error.to_string()))
}

/// Parse an optional RFC3339 timestamp from an API request field.
fn parse_optional_rfc3339(
    raw: Option<&str>,
    field: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    raw.map(|value| {
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&chrono::Utc))
            .map_err(|error| ServiceError::BadRequest(format!("{field} must be RFC3339: {error}")))
    })
    .transpose()
}

fn present_pioneer_code_list(codes: &[PioneerCodeRecord]) -> Result<PioneerCodeListResponse> {
    Ok(PioneerCodeListResponse {
        items: codes
            .iter()
            .map(present_pioneer_code)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn present_pioneer_code_detail(
    code: &PioneerCodeRecord,
    uses: &[PioneerCodeUseRecord],
) -> Result<PioneerCodeDetailResponse> {
    Ok(PioneerCodeDetailResponse {
        code: present_pioneer_code(code)?,
        uses: uses
            .iter()
            .map(present_pioneer_code_use)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn present_pioneer_code(code: &PioneerCodeRecord) -> Result<PioneerCodeDto> {
    Ok(PioneerCodeDto {
        id: code.id.clone(),
        code_display: code.code_display.clone(),
        label: code.label.clone(),
        note: code.note.clone(),
        max_uses: code.max_uses,
        use_count: code.use_count,
        status: PioneerCodeStatus::from_storage_value(&code.status).ok_or_else(|| {
            ServiceError::Internal(format!(
                "stored invalid pioneer-code status `{}`",
                code.status
            ))
        })?,
        expires_at: code.expires_at.map(|expires_at| expires_at.to_rfc3339()),
        created_by_display: code.created_by_display.clone(),
        created_at: code.created_at.to_rfc3339(),
        revoked_at: code.revoked_at.map(|revoked_at| revoked_at.to_rfc3339()),
    })
}

fn present_pioneer_code_use(use_record: &PioneerCodeUseRecord) -> Result<PioneerCodeUseDto> {
    Ok(PioneerCodeUseDto {
        subject_kind: use_record.subject_kind,
        human_id: use_record.human_id.clone(),
        human_github_login: use_record.human_github_login.clone(),
        agent_id: use_record.agent_id.clone(),
        agent_display_name: use_record.agent_display_name.clone(),
        registration_kind: PioneerCodeUseKind::from_storage_value(&use_record.registration_kind)
            .ok_or_else(|| {
                ServiceError::Internal(format!(
                    "stored invalid pioneer-code registration kind `{}`",
                    use_record.registration_kind
                ))
            })?,
        used_at: use_record.used_at.to_rfc3339(),
    })
}

fn present_human(record: HumanRecord) -> Result<AdminHumanDto> {
    Ok(AdminHumanDto {
        human_id: record.human_id,
        status: HumanStatus::from_storage_value(&record.status).ok_or_else(|| {
            ServiceError::Internal(format!("stored invalid human status `{}`", record.status))
        })?,
        github_user_id: record.github_user_id,
        github_login: record.github_login,
        roles: record.roles,
        created_at: record.created_at.to_rfc3339(),
        disabled_at: record
            .disabled_at
            .map(|disabled_at| disabled_at.to_rfc3339()),
    })
}

fn present_admin_service_token(record: AdminServiceTokenRecord) -> AdminServiceTokenDto {
    AdminServiceTokenDto {
        id: record.id,
        label: record.label,
        status: record.status,
        created_by_human_id: record.created_by_human_id,
        created_at: record.created_at.to_rfc3339(),
        last_used_at: record
            .last_used_at
            .map(|last_used_at| last_used_at.to_rfc3339()),
        expires_at: record.expires_at.map(|expires_at| expires_at.to_rfc3339()),
        revoked_by_human_id: record.revoked_by_human_id,
        revoked_at: record.revoked_at.map(|revoked_at| revoked_at.to_rfc3339()),
    }
}
