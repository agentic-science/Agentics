//! Backend-owned public and audience-specific projection helpers.

use agentics_config::Config;
use agentics_contracts::validation::public_api::{
    self, DEFAULT_PUBLIC_LEADERBOARD_LIMIT, DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT,
};
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeDetailResponse, MoltbookCommunityDto,
};
use agentics_domain::models::evaluation::{
    EvaluationDto, EvaluationJobDto, EvaluationJobStatus, MetricValue, ScoringMode,
    SolutionSubmissionStatus,
};
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::{
    CreateSolutionSubmissionResponse, LeaderboardResponse, PublicSolutionSubmissionListResponse,
    RankedLeaderboardEntryDto, RankingContextResponse, ScoreDistributionResponse,
    SolutionSubmissionResponse, SolutionSubmissionResultReportResponse,
};
use agentics_persistence::{ChallengeRecord, Repositories, SolutionSubmissionRecord};
use agentics_storage::{Storage, StorageWriteIntent};

mod score_distribution;
mod visibility;

pub use visibility::{SolutionSubmissionAudience, ensure_ranking_scope_matches_submission};
use visibility::{
    ensure_public_result_detail_visible, ensure_public_result_detail_visible_for_spec,
    ensure_public_solution_artifact_visible, ensure_visibility_allows_public,
    load_challenge_policy, public_visible_solution_submission,
};

/// Fetch public challenge details by challenge name.
pub async fn get_challenge_detail(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    challenge_name: &ChallengeName,
) -> Result<ChallengeDetailResponse> {
    let challenge = Repositories::new(pool)
        .challenges()
        .get_public(challenge_name)
        .await?;
    let challenge = challenge.ok_or(ServiceError::NotFound)?;
    let statement_bytes = storage
        .get(
            &challenge.statement_key,
            StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
        )
        .await?;
    let statement = String::from_utf8(statement_bytes).map_err(|e| {
        ServiceError::Internal(format!("stored challenge statement is not UTF-8: {e}"))
    })?;
    let moltbook = MoltbookCommunityDto {
        submolt_name: config.moltbook.submolt_name.clone(),
        submolt_url: config.moltbook.submolt_url.clone(),
        discussion_url: challenge.moltbook_discussion_url.clone(),
    };
    present_challenge_detail(&challenge, &statement, moltbook)
}

/// Present public challenge details from a published challenge record and statement body.
pub fn present_challenge_detail(
    challenge: &ChallengeRecord,
    statement: &str,
    moltbook: MoltbookCommunityDto,
) -> Result<ChallengeDetailResponse> {
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| ServiceError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    Ok(ChallengeDetailResponse {
        challenge_name: challenge.challenge_name.clone(),
        title: challenge.title.clone(),
        summary: challenge.summary.clone(),
        keywords: spec.keywords.clone(),
        spec: spec.into(),
        statement_markdown: statement.to_string(),
        moltbook,
    })
}

/// Present the response returned immediately after solution submission creation.
pub fn present_create_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
) -> Result<CreateSolutionSubmissionResponse> {
    let evaluation_job_id = solution_submission
        .evaluation_job_id
        .clone()
        .ok_or_else(|| {
            ServiceError::Internal(
                "created solution submission is missing its initial evaluation job id".to_string(),
            )
        })?;

    Ok(CreateSolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        status: solution_submission_status_from_storage(&solution_submission.status)?,
        challenge_name: solution_submission.challenge_name.clone(),
        target: solution_submission.target.clone(),
        artifact_key: solution_submission.artifact_key.clone(),
        note: solution_submission.note.clone(),
        evaluation_job_id,
        created_at: solution_submission.created_at.to_rfc3339(),
    })
}

/// Present a solution submission while applying audience and benchmark visibility policy.
pub fn present_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
    audience: SolutionSubmissionAudience,
) -> Result<SolutionSubmissionResponse> {
    let evaluation = present_evaluation(solution_submission.evaluation.as_ref(), audience);
    let validation_evaluation = if audience.includes_validation_details() {
        present_evaluation(solution_submission.validation_evaluation.as_ref(), audience)
    } else {
        None
    };
    let official_primary_metric =
        solution_submission
            .official_evaluation
            .as_ref()
            .and_then(|evaluation| {
                MetricValue::find_by_name(
                    &evaluation.aggregate_metrics,
                    &solution_submission
                        .challenge_spec
                        .metric_schema
                        .ranking
                        .primary_metric_name,
                )
            });
    let official_evaluation =
        present_evaluation(solution_submission.official_evaluation.as_ref(), audience);
    let evaluation_job = if audience.includes_evaluation_job() {
        solution_submission
            .evaluation_job_id
            .as_ref()
            .map(|id| {
                Ok::<_, ServiceError>(EvaluationJobDto {
                    id: id.clone(),
                    target: solution_submission.target.clone(),
                    status: evaluation_job_status_from_storage(
                        solution_submission
                            .evaluation_job_status
                            .as_deref()
                            .unwrap_or("queued"),
                    )?,
                })
            })
            .transpose()?
    } else {
        None
    };

    Ok(SolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        challenge_name: solution_submission.challenge_name.clone(),
        challenge_title: solution_submission.challenge_title.clone(),
        target: solution_submission.target.clone(),
        agent_id: solution_submission.agent_id.clone(),
        agent_display_name: solution_submission.agent_display_name.clone(),
        status: solution_submission_status_from_storage(&solution_submission.status)?,
        note: solution_submission.note.clone(),
        explanation: solution_submission.explanation.clone(),
        parent_solution_submission_id: solution_submission.parent_solution_submission_id.clone(),
        credit_text: solution_submission.credit_text.clone(),
        official_primary_metric,
        visible_after_eval: solution_submission.visible_after_eval,
        artifact_key: if audience.includes_artifact_key() {
            Some(solution_submission.artifact_key.clone())
        } else {
            None
        },
        evaluation_job,
        evaluation,
        validation_evaluation,
        official_evaluation,
        created_at: solution_submission.created_at.to_rfc3339(),
        updated_at: solution_submission.updated_at.to_rfc3339(),
    })
}

/// List public solution submissions visible for one challenge and target.
pub async fn list_public_solution_submissions(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: Option<&str>,
    limit: Option<i64>,
) -> Result<PublicSolutionSubmissionListResponse> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    ensure_public_result_detail_visible_for_spec(&spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let limit = public_api::bounded_public_limit(
        limit,
        DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT,
        "solution submission list",
    )?;
    let repos = Repositories::new(pool);
    let items = repos
        .solution_submissions()
        .list_public_for_challenge(challenge_name, &target, limit)
        .await?;
    let total_count = repos
        .solution_submissions()
        .count_public_for_challenge(challenge_name, &target)
        .await?;
    Ok(PublicSolutionSubmissionListResponse { total_count, items })
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_result_detail_visible(pool, &solution_submission.challenge_name).await?;
    present_solution_submission(&solution_submission, SolutionSubmissionAudience::Public)
}

/// Fetch a public redacted result report when the challenge visibility allows it.
pub async fn get_public_solution_submission_result_report(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionResultReportResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_result_detail_visible(pool, &solution_submission.challenge_name).await?;
    Ok(SolutionSubmissionResultReportResponse {
        solution_submission: present_solution_submission(
            &solution_submission,
            SolutionSubmissionAudience::Public,
        )?,
    })
}

/// Fetch the public submission record after enforcing visibility for artifact access.
pub async fn get_public_artifact_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionRecord> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_solution_artifact_visible(pool, &solution_submission.challenge_name).await?;
    Ok(solution_submission)
}

/// Fetch public ranking context for a visible submission when the challenge allows it.
pub async fn get_public_solution_submission_ranking_context(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<RankingContextResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    visibility::ensure_ranking_scope_matches_submission(
        &solution_submission,
        challenge_name,
        target,
    )?;
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

/// Fetch a visible score distribution for a metric in one explicit target scope.
pub async fn get_score_distribution(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: Option<&str>,
    metric_name: MetricName,
) -> Result<ScoreDistributionResponse> {
    let (challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.score_distribution, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let entries = Repositories::new(pool)
        .leaderboard()
        .list_entries_with_metric_payloads(challenge_name, &target, 10_000)
        .await?;
    score_distribution::build_score_distribution_response(
        challenge.challenge_name,
        target,
        metric_name,
        &spec,
        entries,
    )
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

/// Parse a persisted solution-submission status for response DTOs.
fn solution_submission_status_from_storage(value: &str) -> Result<SolutionSubmissionStatus> {
    SolutionSubmissionStatus::from_storage_value(value).ok_or_else(|| {
        ServiceError::Internal(format!(
            "stored invalid solution submission status `{value}`"
        ))
    })
}

/// Parse a persisted evaluation job status for response DTOs.
fn evaluation_job_status_from_storage(value: &str) -> Result<EvaluationJobStatus> {
    EvaluationJobStatus::from_storage_value(value).ok_or_else(|| {
        ServiceError::Internal(format!("stored invalid evaluation job status `{value}`"))
    })
}

/// Projects one persisted evaluation according to audience and benchmark privacy policy.
fn present_evaluation(
    evaluation: Option<&EvaluationDto>,
    audience: SolutionSubmissionAudience,
) -> Option<EvaluationDto> {
    let evaluation = evaluation?;
    match evaluation.eval_type {
        ScoringMode::Validation if audience.includes_validation_details() => {
            Some(evaluation.clone())
        }
        ScoringMode::Validation => None,
        ScoringMode::Official => Some(redact_private_benchmark_details(evaluation, audience)),
    }
}

/// Removes official-run fields that could reveal private benchmark cases or logs.
fn redact_private_benchmark_details(
    evaluation: &EvaluationDto,
    audience: SolutionSubmissionAudience,
) -> EvaluationDto {
    let include_aggregate_feedback = audience.includes_official_aggregate_feedback();
    EvaluationDto {
        id: evaluation.id.clone(),
        target: evaluation.target.clone(),
        status: evaluation.status,
        eval_type: evaluation.eval_type,
        rank_score: evaluation.rank_score,
        aggregate_metrics: if include_aggregate_feedback {
            evaluation.aggregate_metrics.clone()
        } else {
            Vec::new()
        },
        run_metrics: Vec::new(),
        public_results: Vec::new(),
        validation_summary: None,
        official_summary: if include_aggregate_feedback {
            evaluation.official_summary.clone()
        } else {
            None
        },
        log_key: None,
        started_at: evaluation.started_at.clone(),
        finished_at: evaluation.finished_at.clone(),
    }
}
