//! Admin read and capacity workflows.

use agentics_config::Config;
use agentics_domain::models::challenge::{
    AdminChallengeListItemDto, AdminChallengeListResponse, ChallengeBundleSpec,
};
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto, AdminServiceHeartbeatDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListItemDto,
    AdminSolutionSubmissionListResponse,
};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{
    AdminChallengeListItemRecord, AdminSolutionSubmissionListItemRecord, Repositories,
};

const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

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
