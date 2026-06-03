//! HTTP handlers for the public, agent, admin, and health APIs.

mod admin;
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
use agentics_contracts::validation::public_api::PublicChallengeCatalogQuery;
use agentics_domain::models::auth::{
    AdminHumanListResponse, AdminHumanRoleResponse, AdminServiceTokenCreatedResponse,
    AdminServiceTokenListResponse, CreateAdminServiceTokenRequest, RevokeAdminServiceTokenResponse,
};
use agentics_domain::models::evaluation::{EvaluationJobStatus, ScoringMode};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::{
    AdminCapacityResponse, AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    CreatePioneerCodeRequest, CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse,
    DisableAgentResponse, EvaluationJobResponse, LeaderboardResponse, PioneerCodeDetailResponse,
    PioneerCodeListResponse, PublicSolutionSubmissionListResponse, PublicStatsResponse,
    RankingContextResponse, RegisterAgentRequest, RegisterAgentResponse, RevokePioneerCodeResponse,
    ScoreDistributionResponse, SolutionSubmissionArtifactResponse, SolutionSubmissionLogsResponse,
    SolutionSubmissionResponse, SolutionSubmissionResultReportResponse,
};
use agentics_error::ServiceError;
use agentics_services::auth;
use agentics_services::challenge_metadata;
use agentics_services::evaluation_lifecycle::{self, QueueEvaluationJobRequest};
use agentics_services::public_projection;
use agentics_services::solution_submissions::{self, CreateSolutionSubmissionServiceRequest};

use crate::extractors::{AdminAuth, AgentAuth, SolutionSubmissionPath, ValidatedJson};
use crate::state::AppState;

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
    Ok((
        StatusCode::CREATED,
        Json(auth::register_agent(&state.db, &state.config, body).await?),
    ))
}

/// List published challenges for authenticated agents.
pub async fn list_agent_challenges(
    _agent: AgentAuth,
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<agentics_domain::models::challenge::ChallengeListResponse>> {
    let query = parse_challenge_catalog_query(raw_query.as_deref())?;
    Ok(Json(
        public_projection::list_challenges(&state.db, &query).await?,
    ))
}

/// Fetch challenge details for authenticated agents.
pub async fn get_agent_challenge(
    _agent: AgentAuth,
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
) -> Result<Json<agentics_domain::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(
        state,
        parse_request_value::<ChallengeName>(&challenge_name)?,
    )
    .await
}

/// List published challenges on the public API.
pub async fn list_challenges(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<agentics_domain::models::challenge::ChallengeListResponse>> {
    let query = parse_challenge_catalog_query(raw_query.as_deref())?;
    Ok(Json(
        public_projection::list_challenges(&state.db, &query).await?,
    ))
}

/// Fetch aggregate public observer counters.
pub async fn get_public_stats(State(state): State<AppState>) -> Result<Json<PublicStatsResponse>> {
    Ok(Json(public_projection::get_public_stats(&state.db).await?))
}

/// Fetch public challenge details by challenge name.
pub async fn get_challenge(
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
) -> Result<Json<agentics_domain::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(
        state,
        parse_request_value::<ChallengeName>(&challenge_name)?,
    )
    .await
}

/// Shared challenge-detail response path used by public and agent routes.
async fn get_challenge_detail_response(
    state: AppState,
    challenge_name: ChallengeName,
) -> Result<Json<agentics_domain::models::challenge::ChallengeDetailResponse>> {
    Ok(Json(
        public_projection::get_challenge_detail(
            &state.db,
            state.storage.as_ref(),
            &state.config,
            &challenge_name,
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

fn parse_challenge_catalog_query(raw: Option<&str>) -> Result<PublicChallengeCatalogQuery> {
    let mut limit = None;
    let mut offset = None;
    let mut search = None;
    let mut keywords = Vec::new();
    if let Some(raw) = raw {
        for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
            match key.as_ref() {
                "limit" => limit = Some(value.into_owned()),
                "offset" => offset = Some(value.into_owned()),
                "q" => search = Some(value.into_owned()),
                "keyword" => keywords.push(value.into_owned()),
                _ => {}
            }
        }
    }
    Ok(PublicChallengeCatalogQuery::try_from_raw_parts(
        limit.as_deref(),
        offset.as_deref(),
        search,
        keywords,
    )?)
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
    Ok(Json(
        public_projection::get_owner_solution_submission(&state.db, &id, &agent.agent_id).await?,
    ))
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
    Ok(Json(
        public_projection::get_owner_solution_submission_result_report(
            &state.db,
            &id,
            &agent.agent_id,
        )
        .await?,
    ))
}

/// Fetch owner-visible runner logs for one solution submission.
pub async fn get_solution_submission_logs(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
) -> Result<Json<SolutionSubmissionLogsResponse>> {
    Ok(Json(
        public_projection::get_owner_solution_submission_logs(
            &state.db,
            state.storage.as_ref(),
            &state.config,
            &id,
            &agent.agent_id,
        )
        .await?,
    ))
}

/// Fetch a submission's owner-visible ranking context in an explicit scope.
pub async fn get_solution_submission_ranking_context(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    agent: AgentAuth,
    Query(query): Query<RankingContextQuery>,
) -> Result<Json<RankingContextResponse>> {
    Ok(Json(
        public_projection::get_owner_solution_submission_ranking_context(
            &state.db,
            &id,
            &agent.agent_id,
            &query.challenge_name,
            &query.target,
        )
        .await?,
    ))
}

// ---------------------------------------------------------------------------
// Public routes
// ---------------------------------------------------------------------------

/// List solution submissions that are visible after completed official evaluation.
pub async fn list_public_solution_submissions(
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
    Query(query): Query<PublicListQuery>,
) -> Result<Json<PublicSolutionSubmissionListResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    Ok(Json(
        public_projection::list_public_solution_submissions(
            &state.db,
            &challenge_name,
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
            &query.challenge_name,
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
    Ok(Json(
        public_projection::get_public_solution_submission_artifact(
            &state.db,
            state.storage.as_ref(),
            &id,
        )
        .await?,
    ))
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    Ok(Json(
        public_projection::get_leaderboard(
            &state.db,
            &challenge_name,
            query.target.as_deref(),
            query.limit,
        )
        .await?,
    ))
}

/// Fetch a visible score distribution for a metric in one explicit target scope.
pub async fn get_score_distribution(
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
    Query(query): Query<ScoreDistributionQuery>,
) -> Result<Json<ScoreDistributionResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    let metric_name = parse_request_value::<MetricName>(&query.metric)?;
    Ok(Json(
        public_projection::get_score_distribution(
            &state.db,
            &challenge_name,
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
    challenge_name: ChallengeName,
    target: TargetName,
}
