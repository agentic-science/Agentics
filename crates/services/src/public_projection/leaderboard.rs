use agentics_contracts::validation::public_api::{self, DEFAULT_PUBLIC_LEADERBOARD_LIMIT};
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    LeaderboardResponse, RankedLeaderboardEntryDto, RankingContextResponse,
};
use agentics_error::{Result, ServiceError};
use agentics_persistence::Repositories;

use super::visibility::{
    ensure_ranking_scope_matches_submission, ensure_visibility_allows_public,
    load_challenge_policy, public_visible_solution_submission,
};

/// Fetch public ranking context for a visible submission when the challenge allows it.
pub async fn get_public_solution_submission_ranking_context(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<RankingContextResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_ranking_scope_matches_submission(&solution_submission, challenge_name, target)?;
    let (_challenge, spec) =
        load_challenge_policy(pool, &solution_submission.challenge_name).await?;
    public_api::resolve_required_public_target(&spec, Some(target.as_str()))?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    build_ranking_context(pool, challenge_name, target, &solution_submission.id).await
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: Option<&str>,
    limit: Option<i64>,
) -> Result<LeaderboardResponse> {
    let (challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let limit =
        public_api::bounded_public_limit(limit, DEFAULT_PUBLIC_LEADERBOARD_LIMIT, "leaderboard")?;
    let items = Repositories::new(pool)
        .leaderboard()
        .list_entries(challenge_name, &target, limit)
        .await?;
    Ok(LeaderboardResponse {
        challenge_name: challenge.challenge_name,
        target,
        items,
    })
}

/// Builds rank, percentile, and nearby leaderboard rows for one submitted solution.
pub async fn build_ranking_context(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<RankingContextResponse> {
    let repos = Repositories::new(pool);
    let challenge = repos
        .challenges()
        .get_public(challenge_name)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let entries = repos
        .leaderboard()
        .list_entries(challenge_name, target, 10_000)
        .await?;
    let total_ranked = i64::try_from(entries.len())
        .map_err(|_| ServiceError::Internal("leaderboard entry count overflow".to_string()))?;
    let ranked_entries = entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let rank_index = index
                .checked_add(1)
                .ok_or_else(|| ServiceError::Internal("leaderboard rank overflow".to_string()))?;
            let rank = i64::try_from(rank_index)
                .map_err(|_| ServiceError::Internal("leaderboard rank overflow".to_string()))?;
            Ok(RankedLeaderboardEntryDto { rank, entry })
        })
        .collect::<Result<Vec<_>>>()?;
    let index = ranked_entries
        .iter()
        .position(|entry| entry.entry.best_solution_submission_id == *solution_submission_id);
    let rank = index
        .map(|index| {
            index
                .checked_add(1)
                .ok_or_else(|| ServiceError::Internal("leaderboard rank overflow".to_string()))
                .and_then(|rank_index| {
                    i64::try_from(rank_index).map_err(|_| {
                        ServiceError::Internal("leaderboard rank overflow".to_string())
                    })
                })
        })
        .transpose()?;
    let percentile = rank.and_then(|rank| {
        if total_ranked <= 0 {
            return None;
        }
        total_ranked
            .checked_sub(rank)
            .and_then(|delta| delta.checked_add(1))
            .map(|position_from_bottom| position_from_bottom as f64 / total_ranked as f64)
    });
    let entry =
        index.and_then(|index| ranked_entries.get(index).map(|ranked| ranked.entry.clone()));
    let nearby_entries = if let Some(index) = index {
        let start = index.saturating_sub(3);
        let end = index
            .checked_add(4)
            .map(|end| end.min(ranked_entries.len()))
            .ok_or_else(|| ServiceError::Internal("leaderboard context overflow".to_string()))?;
        ranked_entries
            .get(start..end)
            .ok_or_else(|| ServiceError::Internal("leaderboard context range invalid".to_string()))?
            .to_vec()
    } else {
        ranked_entries.iter().take(5).cloned().collect()
    };

    Ok(RankingContextResponse {
        challenge_name: challenge.challenge_name,
        target: target.clone(),
        solution_submission_id: solution_submission_id.clone(),
        rank,
        total_ranked,
        percentile,
        is_agent_best: entry.is_some(),
        entry,
        nearby_entries,
    })
}
