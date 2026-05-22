//! HTTP handlers for the public, agent, admin, and health APIs.

mod admin;
mod artifacts;
mod creator;
mod score_distribution;

pub use admin::*;
pub use creator::{
    create_challenge_shortlist_revision, get_challenge_shortlist, get_creator_challenge_stats,
    list_creator_challenge_participants,
};

use axum::{
    Json,
    extract::{Path, Query, RawQuery, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::warn;
use url::form_urlencoded;
use uuid::Uuid;

use shared::auth;
use shared::config::AgentRegistrationMode;
use shared::db::{self, QueueEvaluationJobInput};
use shared::error::{AppError, Result};
use shared::models::challenge::{
    ChallengeBundleSpec, ChallengeResultDetailVisibility, ChallengeSolutionPublicationPolicy,
    ChallengeVisibility, MoltbookCommunityDto,
};
use shared::models::evaluation::{EvaluationJobStatus, ScoringMode};
use shared::models::ids::{
    AgentId, AgentPioneerCodeId, AgentTokenId, EvaluationJobId, SolutionSubmissionId,
};
use shared::models::names::{ChallengeKeyword, ChallengeName, MetricName, TargetName};
use shared::models::pioneer_codes::PioneerCode;
use shared::models::pioneer_codes::PioneerCodeStatus;
use shared::models::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse, AgentStatus,
    CreatePioneerCodeRequest, CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse,
    DisableAgentResponse, EvaluationJobResponse, LeaderboardResponse, PioneerCodeDetailResponse,
    PioneerCodeListResponse, PublicSolutionSubmissionListResponse, PublicStatsResponse,
    RankedLeaderboardEntryDto, RankingContextResponse, RegisterAgentRequest, RegisterAgentResponse,
    RevokePioneerCodeResponse, ScoreDistributionResponse, SolutionSubmissionArtifactResponse,
    SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};
use shared::storage::StorageKey;
use shared::validation::public_api::{
    self, DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT, DEFAULT_PUBLIC_LEADERBOARD_LIMIT,
    DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT, PublicPagination,
};

use crate::extractors::{AdminAuth, AgentAuth, SolutionSubmissionPath, ValidatedJson};
use crate::pioneer_code_security::{is_invalid_pioneer_code, reject_failed_pioneer_code};
use crate::presenters;
use crate::state::AppState;

const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// Parses a boundary string into a domain type and converts validation failures to API errors.
fn parse_request_value<T>(raw: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    raw.parse::<T>()
        .map_err(|e| AppError::BadRequest(e.to_string()))
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Health endpoint that verifies database connectivity.
pub async fn healthz(
    State(state): State<AppState>,
) -> Result<Json<shared::models::HealthResponse>> {
    let db = shared::db::pool::check_database(&state.db).await?;
    Ok(Json(shared::models::HealthResponse {
        status: "ok".to_string(),
        service: "api-server".to_string(),
        environment: "development".to_string(),
        database: db,
    }))
}

// ---------------------------------------------------------------------------
// Agent routes
// ---------------------------------------------------------------------------

/// Register an agent and return its one-time bearer token.
pub async fn register_agent(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<RegisterAgentResponse>)> {
    let max_active_agents = i64::from(state.config.max_active_agents);

    let token = auth::create_agent_token();
    let token_hash = auth::hash_agent_token(&token);

    let input = db::RegisterAgentInput {
        agent_id: AgentId::generate(),
        token_id: AgentTokenId::generate(),
        token_hash,
        display_name: body.display_name.trim().to_string(),
        agent_description: body.agent_description.trim().to_string(),
        owner: body.owner.trim().to_string(),
        model_info: body.model_info,
    };

    let agent = match state.config.agent_registration_mode() {
        AgentRegistrationMode::PioneerCode => {
            let Some(code) = body.pioneer_code.as_ref() else {
                return Err(reject_failed_pioneer_code().await);
            };
            let Ok(code) = PioneerCode::try_new(code.expose_secret().to_string()) else {
                return Err(reject_failed_pioneer_code().await);
            };
            let code_hash = auth::hash_opaque_token(code.expose_secret());
            match db::register_agent_with_pioneer_code(
                &state.db,
                &input,
                &code_hash,
                db::PioneerCodeRegistrationKind::AgentApi,
                max_active_agents,
            )
            .await
            {
                Ok(agent) => agent,
                Err(error) if is_invalid_pioneer_code(&error) => {
                    return Err(reject_failed_pioneer_code().await);
                }
                Err(error) => return Err(error),
            }
        }
        AgentRegistrationMode::Public => {
            db::register_agent(&state.db, &input, max_active_agents).await?
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_register_agent(&agent, &token)?),
    ))
}

/// List published challenges for authenticated agents.
pub async fn list_agent_challenges(
    _agent: AgentAuth,
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<shared::models::challenge::ChallengeListResponse>> {
    let query = ChallengeCatalogQuery::from_raw(raw_query.as_deref())?;
    let page = query.page()?;
    let filters = query.filters()?;
    let challenges =
        db::list_published_challenges(&state.db, page.limit, page.offset, &filters).await?;
    Ok(Json(shared::models::challenge::ChallengeListResponse {
        items: challenges.items,
        total_count: challenges.total_count,
        limit: challenges.limit,
        offset: challenges.offset,
        has_more: challenges.has_more,
    }))
}

/// Fetch challenge details for authenticated agents.
pub async fn get_agent_challenge(
    _agent: AgentAuth,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, parse_request_value::<ChallengeName>(&name)?).await
}

/// List published challenges on the public API.
pub async fn list_challenges(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<shared::models::challenge::ChallengeListResponse>> {
    let query = ChallengeCatalogQuery::from_raw(raw_query.as_deref())?;
    let page = query.page()?;
    let filters = query.filters()?;
    let challenges =
        db::list_published_challenges(&state.db, page.limit, page.offset, &filters).await?;
    Ok(Json(shared::models::challenge::ChallengeListResponse {
        items: challenges.items,
        total_count: challenges.total_count,
        limit: challenges.limit,
        offset: challenges.offset,
        has_more: challenges.has_more,
    }))
}

/// Fetch aggregate public observer counters.
pub async fn get_public_stats(State(state): State<AppState>) -> Result<Json<PublicStatsResponse>> {
    let (challenge_count, agent_count, solution_submission_count) =
        db::public_observer_stats(&state.db).await?;
    Ok(Json(PublicStatsResponse {
        challenge_count,
        agent_count,
        solution_submission_count,
    }))
}

/// Fetch public challenge details by challenge name.
pub async fn get_challenge(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, parse_request_value::<ChallengeName>(&name)?).await
}

/// Shared challenge-detail response path used by public and agent routes.
async fn get_challenge_detail_response(
    state: AppState,
    challenge_name: ChallengeName,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    let challenge = db::get_public_challenge(&state.db, &challenge_name).await?;
    let challenge = challenge.ok_or(AppError::NotFound)?;

    let statement = tokio::fs::read_to_string(challenge.statement_path.as_path()).await?;
    let moltbook = MoltbookCommunityDto {
        submolt_name: state.config.moltbook_submolt_name.clone(),
        submolt_url: state.config.moltbook_submolt_url.clone(),
        discussion_url: challenge.moltbook_discussion_url.clone(),
    };
    Ok(Json(presenters::present_challenge_detail(
        &challenge, &statement, moltbook,
    )?))
}

/// Create a ranking-visible solution submission, store its ZIP artifact, and queue official evaluation.
pub async fn create_solution_submission(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<CreateSolutionSubmissionRequest>,
) -> Result<(StatusCode, Json<CreateSolutionSubmissionResponse>)> {
    create_solution_submission_for_mode(state, agent, body, ScoringMode::Official).await
}

/// Creates a submission for either official or validation mode after admission checks pass.
async fn create_solution_submission_for_mode(
    state: AppState,
    agent: AgentAuth,
    body: CreateSolutionSubmissionRequest,
    eval_type: ScoringMode,
) -> Result<(StatusCode, Json<CreateSolutionSubmissionResponse>)> {
    let challenge_name = body.challenge_name;
    let target = body.target.clone();
    let admission = db::ensure_published_challenge_supports_eval_type(
        &state.db,
        &challenge_name,
        &target,
        eval_type,
        &agent.agent_id,
    )
    .await?;
    let canonical_challenge_name = admission.challenge_name.clone();
    let challenge_lifetime_limit = challenge_lifetime_limit(&admission, eval_type);
    ensure_submission_quota_available(
        &state,
        &agent.agent_id,
        &canonical_challenge_name,
        &target,
        eval_type,
        challenge_lifetime_limit,
    )
    .await?;
    db::ensure_parent_solution_submission_matches_scope(
        &state.db,
        body.parent_solution_submission_id.as_ref(),
        &agent.agent_id,
        &canonical_challenge_name,
        &target,
    )
    .await?;

    let artifact_bytes = artifacts::base64_decode(&body.artifact_base64).ok_or(AppError::Base64)?;
    let manifest = shared::zip_project::ZipProjectManifest::from_zip_bytes(&artifact_bytes)?;

    let solution_submission_id = SolutionSubmissionId::generate();
    let job_id = EvaluationJobId::generate();
    let artifact_key =
        StorageKey::try_new(format!("solution-submissions/{solution_submission_id}.zip"))?;
    let temporary_artifact_key = StorageKey::try_new(format!(
        "_tmp/solution-submissions/{}-{}.zip",
        solution_submission_id,
        Uuid::new_v4()
    ))?;
    let temporary_artifact_key = state
        .storage
        .put(&temporary_artifact_key, &artifact_bytes)
        .await?;

    let quota_limit = match eval_type {
        ScoringMode::Validation => i64::from(state.config.validation_runs_per_agent_challenge_day),
        ScoringMode::Official => i64::from(state.config.official_runs_per_agent_challenge_day),
    };
    let max_active_official_jobs = (eval_type == ScoringMode::Official)
        .then_some(i64::from(state.config.max_active_official_jobs));

    let solution_submission = db::create_solution_submission_with_job(
        &state.db,
        &db::CreateSolutionSubmissionInput {
            solution_submission_id: solution_submission_id.clone(),
            job_id: job_id.clone(),
            agent_id: agent.agent_id,
            challenge_name: canonical_challenge_name,
            target,
            artifact_key: artifact_key.clone(),
            note: manifest.note,
            eval_type,
            explanation: body.explanation.trim().to_string(),
            parent_solution_submission_id: body.parent_solution_submission_id,
            credit_text: body.credit_text.trim().to_string(),
            quota_admission: db::SolutionSubmissionQuotaAdmission {
                window_seconds: SUBMISSION_QUOTA_WINDOW_SECONDS,
                per_agent_challenge_limit: quota_limit,
                challenge_lifetime_limit,
                max_active_official_jobs,
            },
        },
    )
    .await;
    let solution_submission = match solution_submission {
        Ok(solution_submission) => solution_submission,
        Err(error) => {
            cleanup_storage_key(&state, &temporary_artifact_key).await;
            return Err(error);
        }
    };

    if let Err(error) = state
        .storage
        .promote(&temporary_artifact_key, &artifact_key)
        .await
    {
        cleanup_solution_submission_record(&state, &solution_submission.id).await;
        cleanup_storage_key(&state, &temporary_artifact_key).await;
        return Err(error);
    }

    if let Err(error) = db::mark_evaluation_job_ready(&state.db, &job_id).await {
        cleanup_solution_submission_record(&state, &solution_submission.id).await;
        cleanup_storage_key(&state, &artifact_key).await;
        cleanup_storage_key(&state, &temporary_artifact_key).await;
        return Err(error);
    }
    let solution_submission = db::get_solution_submission_by_id(&state.db, &solution_submission.id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(
                "solution submission disappeared after staged job was marked ready".to_string(),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_create_solution_submission(
            &solution_submission,
        )?),
    ))
}

/// Removes a staged submission row after storage or job admission fails.
async fn cleanup_solution_submission_record(
    state: &AppState,
    solution_submission_id: &SolutionSubmissionId,
) {
    if let Err(error) = db::delete_solution_submission(&state.db, solution_submission_id).await {
        warn!(
            solution_submission_id = %solution_submission_id,
            error = %error,
            "failed to clean up staged solution submission after storage admission failure"
        );
    }
}

/// Removes a staged artifact object after submission admission fails.
async fn cleanup_storage_key(state: &AppState, storage_key: &StorageKey) {
    if let Err(error) = state.storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up staged storage object after admission failure"
        );
    }
}

/// Performs pre-upload quota checks so oversized or abusive requests fail before artifact decode.
async fn ensure_submission_quota_available(
    state: &AppState,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
    eval_type: ScoringMode,
    challenge_lifetime_limit: Option<i64>,
) -> Result<()> {
    let limit = match eval_type {
        ScoringMode::Validation => i64::from(state.config.validation_runs_per_agent_challenge_day),
        ScoringMode::Official => i64::from(state.config.official_runs_per_agent_challenge_day),
    };
    let used = db::count_recent_runs_for_agent_challenge(
        &state.db,
        agent_id,
        challenge_name,
        target,
        eval_type,
        SUBMISSION_QUOTA_WINDOW_SECONDS,
    )
    .await?;

    if used >= limit {
        return Err(AppError::TooManyRequests(format!(
            "{} quota exceeded for challenge `{challenge_name}`: {used} of {limit} runs used in the last 24 hours",
            eval_type.as_str()
        )));
    }

    if let Some(limit) = challenge_lifetime_limit {
        let used = db::count_lifetime_runs_for_agent_challenge(
            &state.db,
            agent_id,
            challenge_name,
            target,
            eval_type,
        )
        .await?;
        if used >= limit {
            return Err(AppError::TooManyRequests(format!(
                "{} challenge limit exceeded for challenge `{challenge_name}`: {used} of {limit} lifetime runs used",
                eval_type.as_str()
            )));
        }
    }

    if eval_type == ScoringMode::Official {
        let active = db::count_active_evaluation_jobs(&state.db, ScoringMode::Official).await?;
        let max_active = i64::from(state.config.max_active_official_jobs);
        if active >= max_active {
            return Err(AppError::TooManyRequests(format!(
                "official evaluation queue is full: {active} of {max_active} official jobs are queued or running"
            )));
        }
    }

    Ok(())
}

/// Selects the challenge-level run limit that applies to the requested scoring mode.
fn challenge_lifetime_limit(
    admission: &db::PublishedChallengeAdmission,
    eval_type: ScoringMode,
) -> Option<i64> {
    match eval_type {
        ScoringMode::Validation => admission.validation_submission_limit,
        ScoringMode::Official => admission.official_submission_limit,
    }
}

/// Query parameters accepted by the public challenge catalog endpoint.
#[derive(Debug, Clone, Default)]
pub struct ChallengeCatalogQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
    keyword: Vec<String>,
}

impl ChallengeCatalogQuery {
    /// Parse raw URL query parameters while preserving repeated keyword filters.
    fn from_raw(raw: Option<&str>) -> Result<Self> {
        let mut query = Self::default();
        let Some(raw) = raw else {
            return Ok(query);
        };
        for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
            match key.as_ref() {
                "limit" => {
                    query.limit = Some(parse_i64_query_param(&value, "limit")?);
                }
                "offset" => {
                    query.offset = Some(parse_i64_query_param(&value, "offset")?);
                }
                "q" => {
                    query.q = Some(value.into_owned());
                }
                "keyword" => {
                    query.keyword.push(value.into_owned());
                }
                _ => {}
            }
        }
        Ok(query)
    }

    /// Returns validated challenge-list pagination parameters.
    fn page(&self) -> Result<PublicPagination> {
        public_api::public_pagination(
            self.limit,
            self.offset,
            DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT,
            "challenge list",
        )
    }

    /// Returns validated search and keyword filters for challenge catalog queries.
    fn filters(&self) -> Result<db::ChallengeCatalogFilters> {
        let search = normalized_challenge_search(self.q.as_deref())?;
        let keywords = self
            .keyword
            .iter()
            .map(|raw| ChallengeKeyword::try_new(raw.clone()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AppError::Validation(e.to_string()))?;
        if keywords.len() > 6 {
            return Err(AppError::Validation(
                "challenge catalog filters accept at most 6 keywords".to_string(),
            ));
        }
        Ok(db::ChallengeCatalogFilters { search, keywords })
    }
}

/// Parse a signed integer query parameter for public catalog pagination.
fn parse_i64_query_param(value: &str, field: &str) -> Result<i64> {
    value
        .parse()
        .map_err(|_| AppError::BadRequest(format!("{field} must be an integer")))
}

/// Normalize a public challenge catalog text search query.
fn normalized_challenge_search(raw: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized.len() > 120 || normalized.chars().any(char::is_control) {
        return Err(AppError::Validation(
            "challenge search query must be at most 120 UTF-8 bytes and contain no control characters"
                .to_string(),
        ));
    }
    Ok(Some(normalized.to_string()))
}

/// Create a private validation run, store its ZIP artifact, and queue validation evaluation.
pub async fn create_validation_run(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<CreateSolutionSubmissionRequest>,
) -> Result<(StatusCode, Json<CreateSolutionSubmissionResponse>)> {
    create_solution_submission_for_mode(state, agent, body, ScoringMode::Validation).await
}

/// Fetch an authenticated solution submission view with artifact and job metadata.
pub async fn get_solution_submission(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
) -> Result<Json<SolutionSubmissionResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    Ok(Json(presenters::present_solution_submission(
        &solution_submission,
        presenters::SolutionSubmissionAudience::Owner,
    )?))
}

/// Fetch an authenticated validation run view owned by the caller.
pub async fn get_validation_run(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
) -> Result<Json<SolutionSubmissionResponse>> {
    get_solution_submission(SolutionSubmissionPath(id), State(state), agent).await
}

/// Fetch an owner-visible result report for one solution submission.
pub async fn get_solution_submission_result_report(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
) -> Result<Json<SolutionSubmissionResultReportResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    Ok(Json(SolutionSubmissionResultReportResponse {
        solution_submission: presenters::present_solution_submission(
            &solution_submission,
            presenters::SolutionSubmissionAudience::Owner,
        )?,
    }))
}

/// Fetch owner-visible runner logs for one solution submission.
pub async fn get_solution_submission_logs(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
) -> Result<Json<SolutionSubmissionLogsResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    artifacts::read_solution_submission_logs(&state, &solution_submission).await
}

/// Fetch a submission's owner-visible ranking context in an explicit scope.
pub async fn get_solution_submission_ranking_context(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
    Query(query): Query<RankingContextQuery>,
) -> Result<Json<RankingContextResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    ensure_ranking_scope_matches_submission(&solution_submission, &query)?;
    let response = build_ranking_context(
        &state.db,
        &query.challenge_name,
        &query.target,
        &solution_submission.id,
    )
    .await?;
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Public routes
// ---------------------------------------------------------------------------

/// List solution submissions that are visible after completed official evaluation.
pub async fn list_public_solution_submissions(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<PublicListQuery>,
) -> Result<Json<PublicSolutionSubmissionListResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&name)?;
    let (_challenge, spec) = load_challenge_policy(&state.db, &challenge_name).await?;
    ensure_public_result_detail_visible_for_spec(&spec)?;
    let target = public_api::resolve_required_public_target(&spec, query.target.as_deref())?;
    let limit = query.limit()?;
    let items = db::list_public_solution_submissions_for_challenge(
        &state.db,
        &challenge_name,
        &target,
        limit,
    )
    .await?;
    let total_count =
        db::count_public_solution_submissions_for_challenge(&state.db, &challenge_name, &target)
            .await?;
    Ok(Json(PublicSolutionSubmissionListResponse {
        total_count,
        items,
    }))
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionResponse>> {
    let solution_submission = db::get_public_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_public_result_detail_visible(&state.db, &solution_submission.challenge_name).await?;
    Ok(Json(presenters::present_solution_submission(
        &solution_submission,
        presenters::SolutionSubmissionAudience::Public,
    )?))
}

/// Fetch a public redacted result report when the challenge visibility allows it.
pub async fn get_public_solution_submission_result_report(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionResultReportResponse>> {
    let solution_submission = db::get_public_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_public_result_detail_visible(&state.db, &solution_submission.challenge_name).await?;
    Ok(Json(SolutionSubmissionResultReportResponse {
        solution_submission: presenters::present_solution_submission(
            &solution_submission,
            presenters::SolutionSubmissionAudience::Public,
        )?,
    }))
}

/// Fetch public ranking context for a visible submission when the challenge allows it.
pub async fn get_public_solution_submission_ranking_context(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    Query(query): Query<RankingContextQuery>,
) -> Result<Json<RankingContextResponse>> {
    let solution_submission = db::get_public_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_ranking_scope_matches_submission(&solution_submission, &query)?;
    let (_challenge, spec) =
        load_challenge_policy(&state.db, &solution_submission.challenge_name).await?;
    public_api::resolve_required_public_target(&spec, Some(query.target.as_str()))?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    let response = build_ranking_context(
        &state.db,
        &query.challenge_name,
        &query.target,
        &solution_submission.id,
    )
    .await?;
    Ok(Json(response))
}

/// Fetch a browsable artifact summary for a public solution submission.
pub async fn get_public_artifact(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionArtifactResponse>> {
    let solution_submission = db::get_public_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_public_solution_artifact_visible(&state.db, &solution_submission.challenge_name).await?;

    let artifact_key = solution_submission.artifact_key.clone();
    let artifact_bytes = state.storage.get(&artifact_key).await?;
    let artifact =
        artifacts::read_solution_submission_artifact_summary(artifact_key.as_str(), artifact_bytes)
            .await?;
    Ok(Json(artifact))
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&name)?;
    let (challenge, spec) = load_challenge_policy(&state.db, &challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, query.target.as_deref())?;
    let items =
        db::list_leaderboard_entries(&state.db, &challenge_name, &target, query.limit()?).await?;
    Ok(Json(LeaderboardResponse {
        challenge_name: challenge.challenge_name,
        target,
        items,
    }))
}

/// Fetch a visible score distribution for a metric in one explicit target scope.
pub async fn get_score_distribution(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ScoreDistributionQuery>,
) -> Result<Json<ScoreDistributionResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&name)?;
    let metric_name = parse_request_value::<MetricName>(&query.metric)?;
    let (challenge, spec) = load_challenge_policy(&state.db, &challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.score_distribution, &spec)?;
    let target = public_api::resolve_required_public_target(&spec, query.target.as_deref())?;
    let entries = db::list_leaderboard_entries_with_metric_payloads(
        &state.db,
        &challenge_name,
        &target,
        10_000,
    )
    .await?;
    let response = score_distribution::build_score_distribution_response(
        challenge.challenge_name,
        target,
        metric_name,
        &spec,
        entries,
    )?;
    Ok(Json(response))
}

/// Query parameters accepted by public list endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct PublicListQuery {
    limit: Option<i64>,
    target: Option<String>,
}

impl PublicListQuery {
    /// Returns the requested list limit after applying the public API bounds.
    fn limit(&self) -> Result<i64> {
        public_api::bounded_public_limit(
            self.limit,
            DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT,
            "solution submission list",
        )
    }
}

/// Query parameters accepted by the public leaderboard endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct LeaderboardQuery {
    limit: Option<i64>,
    target: Option<String>,
}

impl LeaderboardQuery {
    /// Returns the requested leaderboard size after applying public API bounds.
    fn limit(&self) -> Result<i64> {
        public_api::bounded_public_limit(
            self.limit,
            DEFAULT_PUBLIC_LEADERBOARD_LIMIT,
            "leaderboard",
        )
    }
}

/// Query parameters accepted by the public score-distribution endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoreDistributionQuery {
    target: Option<String>,
    metric: String,
}

/// Query parameters that pin a submission ranking lookup to one challenge target.
#[derive(Debug, Clone, Deserialize)]
pub struct RankingContextQuery {
    challenge_name: ChallengeName,
    target: TargetName,
}

/// Loads the public challenge record together with its parsed policy-bearing spec.
async fn load_challenge_policy(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<(db::ChallengeRecord, ChallengeBundleSpec)> {
    let challenge = db::get_public_challenge(pool, challenge_name).await?;
    let challenge = challenge.ok_or(AppError::NotFound)?;
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((challenge, spec))
}

/// Enforces whether unauthenticated users may inspect a submission's detailed result report.
async fn ensure_public_result_detail_visible(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
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
        | ChallengeResultDetailVisibility::SubmitterOnly => Err(AppError::NotFound),
    }
}

/// Enforces whether unauthenticated users may download a submission artifact.
async fn ensure_public_solution_artifact_visible(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    match spec.visibility.result_detail {
        ChallengeResultDetailVisibility::SubmitterLivePublicLive => {}
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
            if challenge_has_closed(&spec)? => {}
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
        | ChallengeResultDetailVisibility::SubmitterOnly => return Err(AppError::NotFound),
    }

    match spec.solution_publication {
        ChallengeSolutionPublicationPolicy::Public => Ok(()),
        ChallengeSolutionPublicationPolicy::PublicAfterClose if challenge_has_closed(&spec)? => {
            Ok(())
        }
        ChallengeSolutionPublicationPolicy::Private
        | ChallengeSolutionPublicationPolicy::PublicAfterClose => Err(AppError::NotFound),
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
            Err(AppError::NotFound)
        }
    }
}

/// Returns whether the current wall clock is past the challenge close time.
fn challenge_has_closed(spec: &ChallengeBundleSpec) -> Result<bool> {
    let Some(closes_at) = spec.closes_at.as_deref() else {
        return Ok(false);
    };
    let closes_at = DateTime::parse_from_rfc3339(closes_at)
        .map_err(|e| AppError::Internal(format!("invalid persisted challenge closes_at: {e}")))?
        .with_timezone(&Utc);
    Ok(Utc::now() >= closes_at)
}

/// Rejects ranking-context requests whose scope does not match the submission record.
fn ensure_ranking_scope_matches_submission(
    solution_submission: &db::SolutionSubmissionRecord,
    query: &RankingContextQuery,
) -> Result<()> {
    if solution_submission.challenge_name != query.challenge_name
        || solution_submission.target != query.target
    {
        return Err(AppError::BadRequest(
            "ranking scope must match the solution submission challenge_name and target"
                .to_string(),
        ));
    }
    Ok(())
}

/// Builds rank, percentile, and nearby leaderboard rows for one submitted solution.
async fn build_ranking_context(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: &TargetName,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<RankingContextResponse> {
    let entries = db::list_leaderboard_entries(pool, challenge_name, target, 10_000).await?;
    let total_ranked = i64::try_from(entries.len())
        .map_err(|_| AppError::Internal("leaderboard entry count overflow".to_string()))?;
    let ranked_entries = entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let rank_index = index
                .checked_add(1)
                .ok_or_else(|| AppError::Internal("leaderboard rank overflow".to_string()))?;
            let rank = i64::try_from(rank_index)
                .map_err(|_| AppError::Internal("leaderboard rank overflow".to_string()))?;
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
                .ok_or_else(|| AppError::Internal("leaderboard rank overflow".to_string()))
                .and_then(|rank_index| {
                    i64::try_from(rank_index)
                        .map_err(|_| AppError::Internal("leaderboard rank overflow".to_string()))
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
            .ok_or_else(|| AppError::Internal("leaderboard context overflow".to_string()))?;
        ranked_entries
            .get(start..end)
            .ok_or_else(|| AppError::Internal("leaderboard context range invalid".to_string()))?
            .to_vec()
    } else {
        ranked_entries.iter().take(5).cloned().collect()
    };

    Ok(RankingContextResponse {
        challenge_name: challenge_name.clone(),
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
