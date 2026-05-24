//! Backend-owned public and audience-specific projection helpers.

use chrono::{DateTime, Utc};

use agentics_config::Config;
use agentics_contracts::validation::public_api::{
    self, DEFAULT_PUBLIC_LEADERBOARD_LIMIT, DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT,
};
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeDetailResponse, ChallengeResultDetailVisibility,
    ChallengeSolutionPublicationPolicy, ChallengeVisibility, MetricVisibility,
    MoltbookCommunityDto,
};
use agentics_domain::models::evaluation::{
    EvaluationDto, EvaluationJobDto, EvaluationJobStatus, MetricValue, ScoringMode,
    SolutionSubmissionStatus,
};
use agentics_domain::models::ids::{ChallengeId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::{
    CreateSolutionSubmissionResponse, LeaderboardResponse, PublicSolutionSubmissionListResponse,
    RankedLeaderboardEntryDto, RankingContextResponse, ScoreDistributionBucketDto,
    ScoreDistributionQuantileDto, ScoreDistributionResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};
use agentics_persistence::{
    ChallengeRecord, LeaderboardMetricEntry, Repositories, SolutionSubmissionRecord,
};

/// Audience-specific projection for solution submission details.
#[derive(Debug, Clone, Copy)]
pub enum SolutionSubmissionAudience {
    /// The submitting agent can see its artifact path, job id, and validation details.
    Owner,
    /// Public viewers can only see ranking-visible data.
    Public,
}

impl SolutionSubmissionAudience {
    /// Returns whether this audience may see the stored solution artifact key.
    fn includes_artifact_key(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see the current evaluation job handle.
    fn includes_evaluation_job(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see validation-mode evaluation details.
    fn includes_validation_details(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see submitter-facing official aggregate feedback.
    fn includes_official_aggregate_feedback(self) -> bool {
        matches!(self, Self::Owner)
    }
}

/// Fetch public challenge details by challenge id.
pub async fn get_challenge_detail(
    pool: &sqlx::PgPool,
    config: &Config,
    challenge_id: &ChallengeId,
) -> Result<ChallengeDetailResponse> {
    let challenge = Repositories::new(pool)
        .challenges()
        .get_public(challenge_id)
        .await?;
    let challenge = challenge.ok_or(ServiceError::NotFound)?;
    let statement = tokio::fs::read_to_string(challenge.statement_path.as_path()).await?;
    let moltbook = MoltbookCommunityDto {
        submolt_name: config.moltbook_submolt_name.clone(),
        submolt_url: config.moltbook_submolt_url.clone(),
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
        challenge_id: challenge.challenge_id.clone(),
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
        challenge_id: solution_submission.challenge_id.clone(),
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
        challenge_id: solution_submission.challenge_id.clone(),
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
    challenge_id: &ChallengeId,
    target: Option<&str>,
    limit: Option<i64>,
) -> Result<PublicSolutionSubmissionListResponse> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_id).await?;
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
        .list_public_for_challenge(challenge_id, &target, limit)
        .await?;
    let total_count = repos
        .solution_submissions()
        .count_public_for_challenge(challenge_id, &target)
        .await?;
    Ok(PublicSolutionSubmissionListResponse { total_count, items })
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_result_detail_visible(pool, &solution_submission.challenge_id).await?;
    present_solution_submission(&solution_submission, SolutionSubmissionAudience::Public)
}

/// Fetch a public redacted result report when the challenge visibility allows it.
pub async fn get_public_solution_submission_result_report(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionResultReportResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_result_detail_visible(pool, &solution_submission.challenge_id).await?;
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
    ensure_public_solution_artifact_visible(pool, &solution_submission.challenge_id).await?;
    Ok(solution_submission)
}

/// Fetch public ranking context for a visible submission when the challenge allows it.
pub async fn get_public_solution_submission_ranking_context(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
    challenge_id: &ChallengeId,
    target: &TargetName,
) -> Result<RankingContextResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_ranking_scope_matches_submission(&solution_submission, challenge_id, target)?;
    let (_challenge, spec) = load_challenge_policy(pool, &solution_submission.challenge_id).await?;
    public_api::resolve_required_public_target(&spec, Some(target.as_str()))?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    build_ranking_context(pool, challenge_id, target, &solution_submission.id).await
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    pool: &sqlx::PgPool,
    challenge_id: &ChallengeId,
    target: Option<&str>,
    limit: Option<i64>,
) -> Result<LeaderboardResponse> {
    let (challenge, spec) = load_challenge_policy(pool, challenge_id).await?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let limit =
        public_api::bounded_public_limit(limit, DEFAULT_PUBLIC_LEADERBOARD_LIMIT, "leaderboard")?;
    let items = Repositories::new(pool)
        .leaderboard()
        .list_entries(challenge_id, &target, limit)
        .await?;
    Ok(LeaderboardResponse {
        challenge_id: challenge.challenge_id,
        challenge_name: challenge.challenge_name,
        target,
        items,
    })
}

/// Fetch a visible score distribution for a metric in one explicit target scope.
pub async fn get_score_distribution(
    pool: &sqlx::PgPool,
    challenge_id: &ChallengeId,
    target: Option<&str>,
    metric_name: MetricName,
) -> Result<ScoreDistributionResponse> {
    let (challenge, spec) = load_challenge_policy(pool, challenge_id).await?;
    ensure_visibility_allows_public(spec.visibility.score_distribution, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let entries = Repositories::new(pool)
        .leaderboard()
        .list_entries_with_metric_payloads(challenge_id, &target, 10_000)
        .await?;
    build_score_distribution_response(
        challenge.challenge_id,
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
    challenge_id: &ChallengeId,
    target: &TargetName,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<RankingContextResponse> {
    let repos = Repositories::new(pool);
    let challenge = repos
        .challenges()
        .get_public(challenge_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let entries = repos
        .leaderboard()
        .list_entries(challenge_id, target, 10_000)
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
        challenge_id: challenge.challenge_id,
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

/// Loads a visible public submission by id.
async fn public_visible_solution_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionRecord> {
    let solution_submission = Repositories::new(pool)
        .solution_submissions()
        .get_public_by_id(id)
        .await?;
    let solution_submission = solution_submission.ok_or(ServiceError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(ServiceError::NotFound);
    }
    Ok(solution_submission)
}

/// Loads the public challenge record together with its parsed policy-bearing spec.
async fn load_challenge_policy(
    pool: &sqlx::PgPool,
    challenge_id: &ChallengeId,
) -> Result<(ChallengeRecord, ChallengeBundleSpec)> {
    let challenge = Repositories::new(pool)
        .challenges()
        .get_public(challenge_id)
        .await?;
    let challenge = challenge.ok_or(ServiceError::NotFound)?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    Ok((challenge, spec))
}

/// Enforces whether unauthenticated users may inspect a submission's detailed result report.
async fn ensure_public_result_detail_visible(
    pool: &sqlx::PgPool,
    challenge_id: &ChallengeId,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_id).await?;
    ensure_public_result_detail_visible_for_spec(&spec)
}

/// Enforces whether unauthenticated users may inspect detailed results for a parsed spec.
fn ensure_public_result_detail_visible_for_spec(spec: &ChallengeBundleSpec) -> Result<()> {
    match spec.visibility.result_detail {
        ChallengeResultDetailVisibility::SubmitterLivePublicLive => Ok(()),
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
            if challenge_has_closed(spec)? =>
        {
            Ok(())
        }
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
        | ChallengeResultDetailVisibility::SubmitterOnly => Err(ServiceError::NotFound),
    }
}

/// Enforces whether unauthenticated users may download a submission artifact.
async fn ensure_public_solution_artifact_visible(
    pool: &sqlx::PgPool,
    challenge_id: &ChallengeId,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_id).await?;
    match spec.visibility.result_detail {
        ChallengeResultDetailVisibility::SubmitterLivePublicLive => {}
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
            if challenge_has_closed(&spec)? => {}
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
        | ChallengeResultDetailVisibility::SubmitterOnly => return Err(ServiceError::NotFound),
    }

    match spec.solution_publication {
        ChallengeSolutionPublicationPolicy::Public => Ok(()),
        ChallengeSolutionPublicationPolicy::PublicAfterClose if challenge_has_closed(&spec)? => {
            Ok(())
        }
        ChallengeSolutionPublicationPolicy::Private
        | ChallengeSolutionPublicationPolicy::PublicAfterClose => Err(ServiceError::NotFound),
    }
}

/// Applies challenge visibility policy to an aggregate public surface.
fn ensure_visibility_allows_public(
    visibility: ChallengeVisibility,
    spec: &ChallengeBundleSpec,
) -> Result<()> {
    match visibility {
        ChallengeVisibility::PublicLive => Ok(()),
        ChallengeVisibility::PublicAfterClose if challenge_has_closed(spec)? => Ok(()),
        ChallengeVisibility::PublicAfterClose | ChallengeVisibility::Hidden => {
            Err(ServiceError::NotFound)
        }
    }
}

/// Returns whether the current wall clock is past the challenge close time.
fn challenge_has_closed(spec: &ChallengeBundleSpec) -> Result<bool> {
    let Some(closes_at) = spec.closes_at.as_deref() else {
        return Ok(false);
    };
    let closes_at = DateTime::parse_from_rfc3339(closes_at)
        .map_err(|e| ServiceError::Internal(format!("invalid persisted challenge closes_at: {e}")))?
        .with_timezone(&Utc);
    Ok(Utc::now() >= closes_at)
}

/// Rejects ranking-context requests whose scope does not match the submission record.
pub fn ensure_ranking_scope_matches_submission(
    solution_submission: &SolutionSubmissionRecord,
    challenge_id: &ChallengeId,
    target: &TargetName,
) -> Result<()> {
    if solution_submission.challenge_id != *challenge_id || solution_submission.target != *target {
        return Err(ServiceError::BadRequest(
            "ranking scope must match the solution submission challenge_id and target".to_string(),
        ));
    }
    Ok(())
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

/// Build a distribution response from the visible best leaderboard entries in scope.
pub(super) fn build_score_distribution_response(
    challenge_id: ChallengeId,
    challenge_name: ChallengeName,
    target: TargetName,
    metric_name: MetricName,
    spec: &ChallengeBundleSpec,
    entries: Vec<LeaderboardMetricEntry>,
) -> Result<ScoreDistributionResponse> {
    ensure_metric_is_publicly_distributable(&metric_name, spec)?;
    let mut values = entries
        .iter()
        .filter_map(|entry| metric_value_from_leaderboard_entry(entry, &metric_name, spec))
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let count = i64::try_from(values.len())
        .map_err(|_| ServiceError::Internal("score distribution count overflow".to_string()))?;
    let (min, max, mean, quantiles, histogram) = if values.is_empty() {
        (None, None, None, Vec::new(), Vec::new())
    } else {
        let min = values.first().copied().ok_or_else(|| {
            ServiceError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let max = values.last().copied().ok_or_else(|| {
            ServiceError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let sum: f64 = values.iter().sum();
        let mean = sum / values.len() as f64;
        (
            Some(min),
            Some(max),
            Some(mean),
            build_quantiles(&values)?,
            build_histogram(&values)?,
        )
    };

    Ok(ScoreDistributionResponse {
        challenge_id,
        challenge_name,
        target,
        metric_name,
        count,
        min,
        max,
        mean,
        quantiles,
        histogram,
    })
}

/// Select the metric value that participates in one distribution.
fn metric_value_from_leaderboard_entry(
    entry: &LeaderboardMetricEntry,
    metric_name: &MetricName,
    spec: &ChallengeBundleSpec,
) -> Option<f64> {
    match metric_name.as_str() {
        "rank_score" | "best_rank_score" => Some(entry.best_rank_score),
        _ if metric_name == &spec.metric_schema.ranking.primary_metric_name => {
            metric_value_by_name(&entry.official_metrics, metric_name)
                .or_else(|| metric_value_by_name(&entry.aggregate_metrics, metric_name))
        }
        _ => None,
    }
}

/// Find one metric by name in an evaluator aggregate metric payload.
fn metric_value_by_name(metrics: &[MetricValue], metric_name: &MetricName) -> Option<f64> {
    metrics
        .iter()
        .find(|metric| &metric.metric_name == metric_name)
        .map(|metric| metric.value)
}

/// Reject distribution requests that would require private aggregate metrics.
fn ensure_metric_is_publicly_distributable(
    metric_name: &MetricName,
    spec: &ChallengeBundleSpec,
) -> Result<()> {
    if matches!(metric_name.as_str(), "rank_score" | "best_rank_score") {
        return Ok(());
    }

    if metric_name == &spec.metric_schema.ranking.primary_metric_name
        && spec
            .metric_schema
            .metric(metric_name)
            .is_some_and(|metric| metric.visibility == MetricVisibility::Public)
    {
        return Ok(());
    }

    Err(ServiceError::Forbidden(
        "score distribution is available only for rank_score, best_rank_score, or the public primary ranking metric"
            .to_string(),
    ))
}

/// Build nearest-rank quantiles used by the public distribution API.
fn build_quantiles(values: &[f64]) -> Result<Vec<ScoreDistributionQuantileDto>> {
    [
        (0.0, 0usize, 4usize),
        (0.25, 1usize, 4usize),
        (0.5, 2usize, 4usize),
        (0.75, 3usize, 4usize),
        (0.9, 9usize, 10usize),
        (1.0, 4usize, 4usize),
    ]
    .into_iter()
    .map(|(quantile, numerator, denominator)| {
        Ok(ScoreDistributionQuantileDto {
            quantile,
            value: nearest_rank_quantile(values, numerator, denominator)?,
        })
    })
    .collect()
}

/// Select one nearest-rank quantile from already-sorted finite values.
fn nearest_rank_quantile(values: &[f64], numerator: usize, denominator: usize) -> Result<f64> {
    let max_index = values.len().saturating_sub(1);
    let rounded_index = max_index
        .checked_mul(numerator)
        .and_then(|value| value.checked_add(denominator / 2))
        .and_then(|value| value.checked_div(denominator))
        .ok_or_else(|| ServiceError::Internal("quantile index overflow".to_string()))?
        .min(max_index);
    values
        .get(rounded_index)
        .copied()
        .ok_or_else(|| ServiceError::Internal("quantile index out of range".to_string()))
}

/// Build at most ten histogram buckets for already-sorted finite values.
fn build_histogram(values: &[f64]) -> Result<Vec<ScoreDistributionBucketDto>> {
    let min = values
        .first()
        .copied()
        .ok_or_else(|| ServiceError::Internal("histogram values unexpectedly empty".to_string()))?;
    let max = values
        .last()
        .copied()
        .ok_or_else(|| ServiceError::Internal("histogram values unexpectedly empty".to_string()))?;
    if min == max {
        return Ok(vec![ScoreDistributionBucketDto {
            lower: min,
            upper: max,
            count: i64::try_from(values.len())
                .map_err(|_| ServiceError::Internal("histogram count overflow".to_string()))?,
        }]);
    }

    let bucket_count = values.len().min(10);
    let width = (max - min) / bucket_count as f64;
    let mut counts = vec![0i64; bucket_count];
    for value in values {
        let index = histogram_bucket_index(*value, min, width, bucket_count)?;
        let count = counts
            .get_mut(index)
            .ok_or_else(|| ServiceError::Internal("histogram bucket index invalid".to_string()))?;
        *count = count
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("histogram count overflow".to_string()))?;
    }

    let mut buckets = Vec::with_capacity(counts.len());
    for (index, count) in counts.into_iter().enumerate() {
        let lower = min + width * index as f64;
        let upper = match index.checked_add(1) {
            Some(next_index) if next_index == bucket_count => max,
            Some(next_index) => min + width * next_index as f64,
            None => {
                return Err(ServiceError::Internal(
                    "histogram bucket index overflow".to_string(),
                ));
            }
        };
        buckets.push(ScoreDistributionBucketDto {
            lower,
            upper,
            count,
        });
    }
    Ok(buckets)
}

/// Locate the histogram bucket for a value without using unchecked indexing.
fn histogram_bucket_index(value: f64, min: f64, width: f64, bucket_count: usize) -> Result<usize> {
    for index in 0..bucket_count {
        let next_index = index
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("histogram bucket index overflow".to_string()))?;
        if next_index == bucket_count {
            return Ok(index);
        }
        let upper = min + width * next_index as f64;
        if value < upper {
            return Ok(index);
        }
    }
    bucket_count
        .checked_sub(1)
        .ok_or_else(|| ServiceError::Internal("histogram bucket count invalid".to_string()))
}

#[cfg(test)]
mod tests {
    use agentics_contracts::zip_project::ZipProjectNetworkAccess;
    use agentics_domain::error::ServiceError;
    use agentics_domain::models::challenge::{
        ChallengeBundleSpec, ChallengeEligibilitySpec, ChallengeEligibilityType,
        ChallengeExecutionSpec, ChallengeResultDetailVisibility,
        ChallengeSolutionPublicationPolicy, ChallengeTargetSpec, ChallengeVisibility,
        ChallengeVisibilitySpec, DatasetsSpec, DockerPlatform, EvaluatorSpec,
        EvaluatorStageProfiles, MetricDefinitionSpec, MetricDirection, MetricSchemaSpec,
        MetricVisibility, PrivateBenchmarkPolicy, RankingSpec, ResourceProfileSpec,
        SeparatedEvaluatorExecutionSpec, SolutionSpec, SolutionStageProfiles, StageResourceProfile,
        TargetAccelerator,
    };
    use agentics_domain::models::evaluation::{MetricValue, ScoreVisibility};
    use agentics_domain::models::ids::ChallengeId;
    use agentics_domain::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
    use agentics_domain::models::localization::LocalizedText;
    use agentics_domain::models::names::{
        ChallengeKeyword, ChallengeName, MetricName, ResourceProfileName, TargetName,
    };
    use agentics_domain::models::paths::BundleRelativePath;
    use agentics_persistence::LeaderboardMetricEntry;

    use super::build_score_distribution_response;

    /// Parse a valid challenge name for a focused score-distribution test.
    fn challenge_name(value: &str) -> ChallengeName {
        ChallengeName::try_new(value.to_string()).expect("test challenge name is valid")
    }

    /// Parse a valid challenge id for a focused score-distribution test.
    fn challenge_id(value: &str) -> ChallengeId {
        ChallengeId::try_new(value).expect("test challenge id is valid")
    }

    /// Parse a valid challenge keyword for a focused score-distribution test.
    fn challenge_keyword(value: &str) -> ChallengeKeyword {
        ChallengeKeyword::try_new(value.to_string()).expect("test challenge keyword is valid")
    }

    /// Parse a valid metric name for a focused score-distribution test.
    fn metric_name(value: &str) -> MetricName {
        MetricName::try_new(value.to_string()).expect("test metric name is valid")
    }

    /// Parse a valid target name for a focused score-distribution test.
    fn target_name(value: &str) -> TargetName {
        TargetName::try_new(value.to_string()).expect("test target name is valid")
    }

    /// Parse a valid resource profile name for a focused score-distribution test.
    fn resource_profile_name(value: &str) -> ResourceProfileName {
        ResourceProfileName::try_new(value.to_string())
            .expect("test resource profile name is valid")
    }

    /// Parse a bundle-relative path for a focused score-distribution test.
    fn bundle_path(value: &str) -> BundleRelativePath {
        BundleRelativePath::try_new(value).expect("test bundle path is valid")
    }

    /// Build a local Agentics image reference for focused score-distribution tests.
    fn local_image(value: &str) -> ChallengeImageReference {
        ChallengeImageReference::Local {
            reference: LocalAgenticsImageReference::try_new(value)
                .expect("test local image is valid"),
        }
    }

    /// Build one stage resource profile for focused score-distribution tests.
    fn stage_profile(
        timeout_sec: u64,
        memory_limit_mb: u64,
        cpu_limit_millis: u32,
        disk_limit_mb: u64,
        network_access: ZipProjectNetworkAccess,
    ) -> StageResourceProfile {
        StageResourceProfile {
            timeout_sec,
            memory_limit_mb,
            cpu_limit_millis,
            disk_limit_mb,
            network_access,
        }
    }

    /// Build a minimal challenge contract whose primary metric is minimized.
    fn minimized_metric_spec() -> ChallengeBundleSpec {
        ChallengeBundleSpec {
            schema_version: 1,
            challenge_name: challenge_name("latency-challenge"),
            challenge_title: "Latency Challenge".to_string(),
            summary: LocalizedText::new("Measure raw latency.", "测量原始延迟。"),
            keywords: vec![challenge_keyword("latency")],
            solution: SolutionSpec {
                protocol: "zip_project".to_string(),
                manifest_file: bundle_path("agentics.solution.json"),
            },
            targets: vec![ChallengeTargetSpec {
                name: target_name("linux-arm64-cpu"),
                docker_platform: DockerPlatform::LinuxArm64,
                accelerator: TargetAccelerator::None,
                validation_enabled: true,
                resource_profile: ResourceProfileSpec {
                    name: resource_profile_name("agentics-cpu-small"),
                    resource_description: None,
                    solution_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                    evaluator_image: local_image("agentics-linux-arm64-cpu:ubuntu26.04-local"),
                    solution: SolutionStageProfiles {
                        setup: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Enabled),
                        build: stage_profile(
                            30,
                            512,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Disabled,
                        ),
                        run: Some(stage_profile(
                            30,
                            512,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Disabled,
                        )),
                    },
                    evaluator: EvaluatorStageProfiles {
                        setup: stage_profile(
                            30,
                            512,
                            1000,
                            1024,
                            ZipProjectNetworkAccess::Disabled,
                        ),
                        run: stage_profile(30, 512, 1000, 1024, ZipProjectNetworkAccess::Disabled),
                    },
                    hardware_metadata: None,
                },
            }],
            starts_at: "2026-01-01T00:00:00Z".to_string(),
            closes_at: None,
            eligibility: ChallengeEligibilitySpec {
                eligibility_type: ChallengeEligibilityType::Open,
            },
            validation_submission_limit: None,
            official_submission_limit: None,
            visibility: ChallengeVisibilitySpec {
                leaderboard: ChallengeVisibility::PublicLive,
                score_distribution: ChallengeVisibility::PublicLive,
                result_detail: ChallengeResultDetailVisibility::SubmitterLivePublicLive,
            },
            solution_publication: ChallengeSolutionPublicationPolicy::Public,
            execution: ChallengeExecutionSpec::SeparatedEvaluator(
                SeparatedEvaluatorExecutionSpec {
                    separated_evaluator: EvaluatorSpec {
                        command: vec![
                            "python".to_string(),
                            "separated-evaluator/run.py".to_string(),
                        ],
                        result_file: bundle_path("result.json"),
                    },
                    validation_runs: Some(bundle_path("public/runs.json")),
                    validation_setup: None,
                    official_runs: Some(bundle_path("private-benchmark/runs.json")),
                    official_evaluation_setup: None,
                },
            ),
            datasets: DatasetsSpec {
                public_dir: bundle_path("public"),
                private_benchmark_dir: Some(bundle_path("private-benchmark")),
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: true,
            },
            metric_schema: MetricSchemaSpec {
                metrics: vec![MetricDefinitionSpec {
                    name: metric_name("latency_ms"),
                    label: "Latency".to_string(),
                    unit: Some("ms".to_string()),
                    direction: MetricDirection::Minimize,
                    visibility: MetricVisibility::Public,
                    metric_description: None,
                }],
                ranking: RankingSpec {
                    primary_metric_name: metric_name("latency_ms"),
                    tie_breaker_metric_names: Vec::new(),
                },
            },
        }
    }

    /// Build one leaderboard entry with distinct primary metric and rank scores.
    fn entry(raw_latency: f64, rank_score: f64) -> LeaderboardMetricEntry {
        LeaderboardMetricEntry {
            best_rank_score: rank_score,
            aggregate_metrics: vec![MetricValue {
                metric_name: metric_name("latency_ms"),
                value: raw_latency,
            }],
            official_metrics: vec![MetricValue {
                metric_name: metric_name("latency_ms"),
                value: raw_latency,
            }],
        }
    }

    /// Verifies primary-metric distributions use raw metric values, not rank values.
    #[test]
    fn primary_metric_distribution_uses_raw_metric_values_for_minimized_metrics() {
        let spec = minimized_metric_spec();
        let response = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("latency_ms"),
            &spec,
            vec![entry(20.0, -20.0), entry(50.0, -50.0)],
        )
        .expect("score distribution should build");

        assert_eq!(response.count, 2);
        assert_eq!(response.min, Some(20.0));
        assert_eq!(response.max, Some(50.0));
    }

    /// Verifies rank-score distributions intentionally use comparator values.
    #[test]
    fn rank_score_distribution_uses_comparator_values() {
        let spec = minimized_metric_spec();
        let response = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("rank_score"),
            &spec,
            vec![entry(20.0, -20.0), entry(50.0, -50.0)],
        )
        .expect("score distribution should build");

        assert_eq!(response.count, 2);
        assert_eq!(response.min, Some(-50.0));
        assert_eq!(response.max, Some(-20.0));
    }

    /// Verifies official-only primary metrics are not distributable through the public endpoint.
    #[test]
    fn primary_metric_distribution_rejects_official_only_metric() {
        let mut spec = minimized_metric_spec();
        spec.metric_schema.metrics[0].visibility = MetricVisibility::Official;

        let error = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("latency_ms"),
            &spec,
            vec![entry(20.0, -20.0)],
        )
        .expect_err("official-only primary metric should be rejected");
        assert!(matches!(error, ServiceError::Forbidden(_)));

        let error = build_score_distribution_response(
            challenge_id("11111111-1111-4111-8111-111111111111"),
            challenge_name("latency-challenge"),
            target_name("linux-arm64-cpu"),
            metric_name("official_score"),
            &spec,
            vec![entry(20.0, -20.0)],
        )
        .expect_err("official_score built-in is no longer exposed");
        assert!(matches!(error, ServiceError::Forbidden(_)));
    }
}
