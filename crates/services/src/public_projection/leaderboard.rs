use agentics_contracts::validation::public_api::{self, DEFAULT_PUBLIC_LEADERBOARD_LIMIT};
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::ids::{AgentId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    LeaderboardEntryDto, LeaderboardResponse, RankedLeaderboardEntryDto, RankingContextResponse,
};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{LeaderboardRecord, Repositories};

use super::metrics::official_primary_metric;
use super::visibility::{
    ensure_ranking_scope_matches_submission, ensure_visibility_allows_public,
    load_challenge_policy, public_visible_solution_submission,
};

const MAX_RANKING_CONTEXT_LEADERBOARD_ROWS: usize = 10_000;
const RANKING_CONTEXT_TRUNCATION_WARNING: &str = "ranking context is limited to the first 10000 leaderboard rows; totals and percentiles are based on that truncated set";

/// Fetch a submission's owner-visible ranking context in an explicit scope.
pub async fn get_owner_solution_submission_ranking_context(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<RankingContextResponse> {
    let solution_submission = Repositories::new(pool)
        .solution_submissions()
        .get_by_id(id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if solution_submission.agent_id != *agent_id {
        return Err(ServiceError::NotFound);
    }
    ensure_ranking_scope_matches_submission(&solution_submission, challenge_name, target)?;
    build_ranking_context(pool, challenge_name, target, &solution_submission.id).await
}

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
        .list_entries(challenge_name, &target, limit, &spec)
        .await?
        .into_iter()
        .map(|record| present_leaderboard_entry(record, &spec))
        .collect();
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
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;
    let fetch_limit = i64::try_from(MAX_RANKING_CONTEXT_LEADERBOARD_ROWS + 1)
        .map_err(|_| ServiceError::Internal("leaderboard fetch limit overflow".to_string()))?;
    let mut entries = repos
        .leaderboard()
        .list_entries(challenge_name, target, fetch_limit, &spec)
        .await?
        .into_iter()
        .map(|record| present_leaderboard_entry(record, &spec))
        .collect::<Vec<_>>();
    let truncated = entries.len() > MAX_RANKING_CONTEXT_LEADERBOARD_ROWS;
    if truncated {
        entries.truncate(MAX_RANKING_CONTEXT_LEADERBOARD_ROWS);
    }
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
        warnings: if truncated {
            vec![RANKING_CONTEXT_TRUNCATION_WARNING.to_string()]
        } else {
            Vec::new()
        },
    })
}

/// Project one persistence leaderboard record into the public DTO surface.
fn present_leaderboard_entry(
    record: LeaderboardRecord,
    spec: &ChallengeBundleSpec,
) -> LeaderboardEntryDto {
    let official_primary_metric = official_primary_metric(&record.official_metrics, spec);
    LeaderboardEntryDto {
        target: record.target,
        agent_id: record.agent_id,
        agent_display_name: record.agent_display_name,
        best_solution_submission_id: record.best_solution_submission_id,
        best_rank_score: record.best_rank_score,
        rank_score: record.best_rank_score,
        official_primary_metric,
        updated_at: record.updated_at.to_rfc3339(),
    }
}
