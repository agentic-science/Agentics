//! HTTP handlers for the public, agent, admin, and health APIs.

mod admin;
mod artifacts;
mod creator;

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
use serde::Deserialize;
use url::form_urlencoded;

use crate::error::ApiResult as Result;
use agentics_config::AgentRegistrationMode;
use agentics_contracts::validation::public_api::{
    self, DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT, PublicPagination,
};
use agentics_domain::error::ServiceError;
use agentics_domain::models::evaluation::{EvaluationJobStatus, ScoringMode};
use agentics_domain::models::ids::{AgentId, AgentPioneerCodeId, AgentTokenId, ChallengeId};
use agentics_domain::models::names::{ChallengeKeyword, MetricName, TargetName};
use agentics_domain::models::pioneer_codes::PioneerCode;
use agentics_domain::models::pioneer_codes::PioneerCodeStatus;
use agentics_domain::models::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse, AgentStatus,
    CreatePioneerCodeRequest, CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse,
    DisableAgentResponse, EvaluationJobResponse, LeaderboardResponse, PioneerCodeDetailResponse,
    PioneerCodeListResponse, PublicSolutionSubmissionListResponse, PublicStatsResponse,
    RankingContextResponse, RegisterAgentRequest, RegisterAgentResponse, RevokePioneerCodeResponse,
    ScoreDistributionResponse, SolutionSubmissionArtifactResponse, SolutionSubmissionLogsResponse,
    SolutionSubmissionResponse, SolutionSubmissionResultReportResponse,
};
use agentics_persistence::{
    ChallengeCatalogFilters, PioneerCodeRegistrationKind, RegisterAgentInput, Repositories,
};
use agentics_services::auth;
use agentics_services::challenge_metadata;
use agentics_services::evaluation_lifecycle::{self, QueueEvaluationJobRequest};
use agentics_services::public_projection::{self, SolutionSubmissionAudience};
use agentics_services::solution_submissions::{self, CreateSolutionSubmissionServiceRequest};

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
    Ok(raw
        .parse::<T>()
        .map_err(|e| ServiceError::BadRequest(e.to_string()))?)
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Health endpoint that verifies database connectivity.
pub async fn healthz(
    State(state): State<AppState>,
) -> Result<Json<agentics_domain::models::HealthResponse>> {
    let db = agentics_persistence::pool::check_database(&state.db).await?;
    Ok(Json(agentics_domain::models::HealthResponse {
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

    let repos = Repositories::new(&state.db);
    let input = RegisterAgentInput {
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
                return Err(reject_failed_pioneer_code().await.into());
            };
            let Ok(code) = PioneerCode::try_new(code.expose_secret().to_string()) else {
                return Err(reject_failed_pioneer_code().await.into());
            };
            let code_hash = auth::hash_opaque_token(code.expose_secret());
            match repos
                .agents()
                .register_agent_with_pioneer_code(
                    &input,
                    &code_hash,
                    max_active_agents,
                    PioneerCodeRegistrationKind::AgentApi,
                )
                .await
            {
                Ok(agent) => agent,
                Err(error) if is_invalid_pioneer_code(&error) => {
                    return Err(reject_failed_pioneer_code().await.into());
                }
                Err(error) => return Err(error.into()),
            }
        }
        AgentRegistrationMode::Public => {
            repos
                .agents()
                .register_agent(&input, max_active_agents)
                .await?
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
) -> Result<Json<agentics_domain::models::challenge::ChallengeListResponse>> {
    let query = ChallengeCatalogQuery::from_raw(raw_query.as_deref())?;
    let page = query.page()?;
    let filters = query.filters()?;
    let challenges = Repositories::new(&state.db)
        .challenges()
        .list_published(page.limit, page.offset, &filters)
        .await?;
    Ok(Json(
        agentics_domain::models::challenge::ChallengeListResponse {
            items: challenges.items,
            total_count: challenges.total_count,
            limit: challenges.limit,
            offset: challenges.offset,
            has_more: challenges.has_more,
        },
    ))
}

/// Fetch challenge details for authenticated agents.
pub async fn get_agent_challenge(
    _agent: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<agentics_domain::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, parse_request_value::<ChallengeId>(&id)?).await
}

/// List published challenges on the public API.
pub async fn list_challenges(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<agentics_domain::models::challenge::ChallengeListResponse>> {
    let query = ChallengeCatalogQuery::from_raw(raw_query.as_deref())?;
    let page = query.page()?;
    let filters = query.filters()?;
    let challenges = Repositories::new(&state.db)
        .challenges()
        .list_published(page.limit, page.offset, &filters)
        .await?;
    Ok(Json(
        agentics_domain::models::challenge::ChallengeListResponse {
            items: challenges.items,
            total_count: challenges.total_count,
            limit: challenges.limit,
            offset: challenges.offset,
            has_more: challenges.has_more,
        },
    ))
}

/// Fetch aggregate public observer counters.
pub async fn get_public_stats(State(state): State<AppState>) -> Result<Json<PublicStatsResponse>> {
    let (challenge_count, agent_count, solution_submission_count) = Repositories::new(&state.db)
        .solution_submissions()
        .observer_stats()
        .await?;
    Ok(Json(PublicStatsResponse {
        challenge_count,
        agent_count,
        solution_submission_count,
    }))
}

/// Fetch public challenge details by challenge id.
pub async fn get_challenge(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<agentics_domain::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, parse_request_value::<ChallengeId>(&id)?).await
}

/// Shared challenge-detail response path used by public and agent routes.
async fn get_challenge_detail_response(
    state: AppState,
    challenge_id: ChallengeId,
) -> Result<Json<agentics_domain::models::challenge::ChallengeDetailResponse>> {
    Ok(Json(
        public_projection::get_challenge_detail(
            &state.db,
            state.storage.as_ref(),
            &state.config,
            &challenge_id,
        )
        .await?,
    ))
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
    let response = solution_submissions::create_solution_submission(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        CreateSolutionSubmissionServiceRequest {
            agent_id: agent.agent_id,
            body,
            eval_type,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
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
        Ok(public_api::public_pagination(
            self.limit,
            self.offset,
            DEFAULT_PUBLIC_CHALLENGE_LIST_LIMIT,
            "challenge list",
        )?)
    }

    /// Returns validated search and keyword filters for challenge catalog queries.
    fn filters(&self) -> Result<ChallengeCatalogFilters> {
        let search = normalized_challenge_search(self.q.as_deref())?;
        let keywords = self
            .keyword
            .iter()
            .map(|raw| ChallengeKeyword::try_new(raw.clone()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| ServiceError::Validation(e.to_string()))?;
        if keywords.len() > 6 {
            return Err(ServiceError::Validation(
                "challenge catalog filters accept at most 6 keywords".to_string(),
            )
            .into());
        }
        Ok(ChallengeCatalogFilters { search, keywords })
    }
}

/// Parse a signed integer query parameter for public catalog pagination.
fn parse_i64_query_param(value: &str, field: &str) -> Result<i64> {
    Ok(value
        .parse()
        .map_err(|_| ServiceError::BadRequest(format!("{field} must be an integer")))?)
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
        return Err(ServiceError::Validation(
            "challenge search query must be at most 120 UTF-8 bytes and contain no control characters"
                .to_string(),
        )
        .into());
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
    let solution_submission = Repositories::new(&state.db)
        .solution_submissions()
        .get_by_id(&id)
        .await?;
    let solution_submission = solution_submission.ok_or(ServiceError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(ServiceError::NotFound.into());
    }
    Ok(Json(public_projection::present_solution_submission(
        &solution_submission,
        SolutionSubmissionAudience::Owner,
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
    let solution_submission = Repositories::new(&state.db)
        .solution_submissions()
        .get_by_id(&id)
        .await?;
    let solution_submission = solution_submission.ok_or(ServiceError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(ServiceError::NotFound.into());
    }
    Ok(Json(SolutionSubmissionResultReportResponse {
        solution_submission: public_projection::present_solution_submission(
            &solution_submission,
            SolutionSubmissionAudience::Owner,
        )?,
    }))
}

/// Fetch owner-visible runner logs for one solution submission.
pub async fn get_solution_submission_logs(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
) -> Result<Json<SolutionSubmissionLogsResponse>> {
    let solution_submission = Repositories::new(&state.db)
        .solution_submissions()
        .get_by_id(&id)
        .await?;
    let solution_submission = solution_submission.ok_or(ServiceError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(ServiceError::NotFound.into());
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
    let solution_submission = Repositories::new(&state.db)
        .solution_submissions()
        .get_by_id(&id)
        .await?;
    let solution_submission = solution_submission.ok_or(ServiceError::NotFound)?;
    if solution_submission.agent_id != agent.agent_id {
        return Err(ServiceError::NotFound.into());
    }
    public_projection::ensure_ranking_scope_matches_submission(
        &solution_submission,
        &query.challenge_id,
        &query.target,
    )?;
    let response = public_projection::build_ranking_context(
        &state.db,
        &query.challenge_id,
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
    Path(id): Path<String>,
    Query(query): Query<PublicListQuery>,
) -> Result<Json<PublicSolutionSubmissionListResponse>> {
    let challenge_id = parse_request_value::<ChallengeId>(&id)?;
    Ok(Json(
        public_projection::list_public_solution_submissions(
            &state.db,
            &challenge_id,
            query.target.as_deref(),
            query.limit,
        )
        .await?,
    ))
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionResponse>> {
    Ok(Json(
        public_projection::get_public_solution_submission(&state.db, &id).await?,
    ))
}

/// Fetch a public redacted result report when the challenge visibility allows it.
pub async fn get_public_solution_submission_result_report(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionResultReportResponse>> {
    Ok(Json(
        public_projection::get_public_solution_submission_result_report(&state.db, &id).await?,
    ))
}

/// Fetch public ranking context for a visible submission when the challenge allows it.
pub async fn get_public_solution_submission_ranking_context(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    Query(query): Query<RankingContextQuery>,
) -> Result<Json<RankingContextResponse>> {
    Ok(Json(
        public_projection::get_public_solution_submission_ranking_context(
            &state.db,
            &id,
            &query.challenge_id,
            &query.target,
        )
        .await?,
    ))
}

/// Fetch a browsable artifact summary for a public solution submission.
pub async fn get_public_artifact(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionArtifactResponse>> {
    let solution_submission =
        public_projection::get_public_artifact_submission(&state.db, &id).await?;

    let artifact_key = solution_submission.artifact_key.clone();
    let artifact_bytes = state
        .storage
        .get(&artifact_key, artifacts::solution_artifact_intent())
        .await?;
    let artifact =
        artifacts::read_solution_submission_artifact_summary(artifact_key.as_str(), artifact_bytes)
            .await?;
    Ok(Json(artifact))
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>> {
    let challenge_id = parse_request_value::<ChallengeId>(&id)?;
    Ok(Json(
        public_projection::get_leaderboard(
            &state.db,
            &challenge_id,
            query.target.as_deref(),
            query.limit,
        )
        .await?,
    ))
}

/// Fetch a visible score distribution for a metric in one explicit target scope.
pub async fn get_score_distribution(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ScoreDistributionQuery>,
) -> Result<Json<ScoreDistributionResponse>> {
    let challenge_id = parse_request_value::<ChallengeId>(&id)?;
    let metric_name = parse_request_value::<MetricName>(&query.metric)?;
    Ok(Json(
        public_projection::get_score_distribution(
            &state.db,
            &challenge_id,
            query.target.as_deref(),
            metric_name,
        )
        .await?,
    ))
}

/// Query parameters accepted by public list endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct PublicListQuery {
    limit: Option<i64>,
    target: Option<String>,
}

/// Query parameters accepted by the public leaderboard endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct LeaderboardQuery {
    limit: Option<i64>,
    target: Option<String>,
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
    challenge_id: ChallengeId,
    target: TargetName,
}
