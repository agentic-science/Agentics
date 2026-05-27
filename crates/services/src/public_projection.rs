//! Backend-owned public and audience-specific projection helpers.

mod challenge;
mod leaderboard;
mod metrics;
mod score_distribution;
mod submission;
mod visibility;

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
    let (challenge_count, agent_count, solution_submission_count) = Repositories::new(pool)
        .solution_submissions()
        .observer_stats()
        .await?;
    Ok(PublicStatsResponse {
        challenge_count,
        agent_count,
        solution_submission_count,
    })
}
