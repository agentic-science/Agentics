//! Backend-owned public and audience-specific projection helpers.

mod artifact;
mod challenge;
mod leaderboard;
mod metrics;
mod score_distribution;
mod submission;
mod visibility;

pub use artifact::{get_owner_solution_submission_logs, get_public_solution_submission_artifact};
pub use challenge::{get_challenge_detail, list_challenges, present_challenge_detail};
pub use leaderboard::{
    build_ranking_context, get_leaderboard, get_owner_solution_submission_ranking_context,
    get_public_solution_submission_ranking_context,
};
pub use score_distribution::get_score_distribution;
pub use submission::{
    get_owner_solution_submission, get_owner_solution_submission_record,
    get_owner_solution_submission_result_report, get_public_artifact_submission,
    get_public_solution_submission, get_public_solution_submission_result_report,
    list_public_solution_submissions, present_create_solution_submission,
    present_solution_submission,
};
pub use visibility::{SolutionSubmissionAudience, ensure_ranking_scope_matches_submission};

use agentics_domain::models::request::PublicStatsResponse;
use agentics_error::Result;
use agentics_persistence::Repositories;

/// Fetch aggregate public observer counters.
pub async fn get_public_stats(pool: &sqlx::PgPool) -> Result<PublicStatsResponse> {
    let stats = Repositories::new(pool)
        .solution_submissions()
        .observer_stats()
        .await?;
    Ok(PublicStatsResponse {
        challenge_count: stats.challenge_count,
        agent_count: stats.agent_count,
        public_completed_submission_count: stats.public_completed_submission_count,
        total_solution_attempt_count: stats.total_solution_attempt_count,
    })
}
