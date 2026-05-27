//! Admin read and capacity workflows.

use agentics_config::Config;
use agentics_domain::models::challenge::AdminChallengeListResponse;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
};
use agentics_error::Result;
use agentics_persistence::Repositories;

const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// List challenge shells and published benchmark contracts for admins.
pub async fn list_admin_challenges(pool: &sqlx::PgPool) -> Result<AdminChallengeListResponse> {
    let items = Repositories::new(pool).challenges().list_admin().await?;
    Ok(AdminChallengeListResponse { items })
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    pool: &sqlx::PgPool,
) -> Result<AdminSolutionSubmissionListResponse> {
    let items = Repositories::new(pool)
        .solution_submissions()
        .list_admin(100)
        .await?;
    Ok(AdminSolutionSubmissionListResponse { items })
}

/// List latest service heartbeats for admin operations.
pub async fn list_admin_service_heartbeats(
    pool: &sqlx::PgPool,
) -> Result<AdminServiceHeartbeatListResponse> {
    let items = Repositories::new(pool)
        .maintenance()
        .list_service_heartbeats()
        .await?;
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
