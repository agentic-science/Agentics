//! HTTP handlers for the public, agent, admin, and health APIs.

use std::path::{Path as FsPath, PathBuf};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::warn;
use uuid::Uuid;

use shared::auth;
use shared::challenge_bundle;
use shared::challenge_creation;
use shared::config::Config;
use shared::db::{self, QueueEvaluationJobInput};
use shared::error::{AppError, Result};
use shared::models::challenge::{
    ChallengeBundleSpec, ChallengeEligibilityType, ChallengeResultDetailVisibility,
    ChallengeSolutionPublicationPolicy, ChallengeVisibility, PublishChallengeResponse,
};
use shared::models::evaluation::ScoringMode;
use shared::models::ids::SolutionSubmissionId;
use shared::models::names::{ChallengeName, MetricName, TargetName};
use shared::models::paths::AdminBundlePath;
use shared::models::request::{
    AdminCapacityResponse, AdminCapacityUsageDto, AdminQuotaSettingsDto,
    AdminServiceHeartbeatListResponse, AdminSolutionSubmissionListResponse,
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse, CreateChallengeRequest,
    CreateChallengeShortlistRevisionRequest, CreateSolutionSubmissionRequest,
    CreateSolutionSubmissionResponse, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse, DisableAgentResponse, EvaluationJobResponse,
    HideSolutionSubmissionResponse, LeaderboardEntryDto, LeaderboardResponse,
    PublicSolutionSubmissionListResponse, PublishChallengeRequest, RankedLeaderboardEntryDto,
    RankingContextResponse, RegisterAgentRequest, RegisterAgentResponse,
    ScoreDistributionBucketDto, ScoreDistributionQuantileDto, ScoreDistributionResponse,
    SolutionSubmissionArtifactFileDto, SolutionSubmissionArtifactResponse,
    SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};
use shared::storage::StorageKey;
use shared::zip_project::{
    MAX_ZIP_PROJECT_ARTIFACT_BYTES, MAX_ZIP_PROJECT_FILE_COUNT, MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
};

use crate::extractors::{AdminAuth, AgentAuth, CreatorAuth, SolutionSubmissionPath, ValidatedJson};
use crate::presenters;
use crate::state::AppState;

const MAX_INLINE_TEXT_BYTES: u64 = 200_000;
const MAX_TOTAL_INLINE_TEXT_BYTES: u64 = 1_000_000;
const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;
const STAGED_EVALUATION_JOB_DELAY_SECONDS: i64 = 315_360_000;
const DEFAULT_PUBLIC_LIST_LIMIT: i64 = 50;
const MAX_PUBLIC_LIST_LIMIT: i64 = 100;

fn parse_challenge_name(raw: &str) -> Result<ChallengeName> {
    ChallengeName::try_new(raw.to_string()).map_err(|e| AppError::BadRequest(e.to_string()))
}

fn parse_target(raw: &str) -> Result<TargetName> {
    TargetName::try_new(raw.to_string()).map_err(|e| AppError::BadRequest(e.to_string()))
}

fn parse_metric_name(raw: &str) -> Result<MetricName> {
    MetricName::try_new(raw.to_string()).map_err(|e| AppError::BadRequest(e.to_string()))
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
    let active_agents = db::count_active_agents(&state.db).await?;
    let max_active_agents = i64::from(state.config.max_active_agents);
    if active_agents >= max_active_agents {
        return Err(AppError::TooManyRequests(format!(
            "agent registration quota exceeded: {active_agents} of {max_active_agents} active agents are already registered"
        )));
    }

    let token = auth::create_agent_token();
    let token_hash = auth::hash_agent_token(&token);

    let agent = db::register_agent(
        &state.db,
        &db::RegisterAgentInput {
            agent_id: Uuid::new_v4().to_string(),
            token_id: Uuid::new_v4().to_string(),
            token_hash,
            name: body.name.trim().to_string(),
            agent_description: body.agent_description.trim().to_string(),
            owner: body.owner.trim().to_string(),
            model_info: body.model_info,
        },
    )
    .await
    .map_err(|e| match e {
        AppError::Database(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            AppError::Conflict
        }
        _ => e,
    })?;

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_register_agent(&agent, &token)),
    ))
}

/// List published challenges for authenticated agents.
pub async fn list_agent_challenges(
    _agent: AgentAuth,
    State(state): State<AppState>,
) -> Result<Json<shared::models::challenge::ChallengeListResponse>> {
    let challenges = db::list_published_challenges(&state.db).await?;
    Ok(Json(shared::models::challenge::ChallengeListResponse {
        items: challenges,
    }))
}

/// Fetch challenge details for authenticated agents.
pub async fn get_agent_challenge(
    _agent: AgentAuth,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, parse_challenge_name(&name)?).await
}

/// List published challenges on the public API.
pub async fn list_challenges(
    State(state): State<AppState>,
) -> Result<Json<shared::models::challenge::ChallengeListResponse>> {
    let challenges = db::list_published_challenges(&state.db).await?;
    Ok(Json(shared::models::challenge::ChallengeListResponse {
        items: challenges,
    }))
}

/// Fetch public challenge details by challenge name.
pub async fn get_challenge(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    get_challenge_detail_response(state, parse_challenge_name(&name)?).await
}

/// Shared challenge-detail response path used by public and agent routes.
async fn get_challenge_detail_response(
    state: AppState,
    challenge_name: ChallengeName,
) -> Result<Json<shared::models::challenge::ChallengeDetailResponse>> {
    let challenge = db::get_public_challenge(&state.db, &challenge_name).await?;
    let challenge = challenge.ok_or(AppError::NotFound)?;

    let statement = tokio::fs::read_to_string(&challenge.statement_path).await?;
    Ok(Json(presenters::present_challenge_detail(
        &challenge, &statement,
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

    let artifact_bytes = base64_decode(&body.artifact_base64).ok_or(AppError::Base64)?;
    if artifact_bytes.len() as u64 > MAX_ZIP_PROJECT_ARTIFACT_BYTES {
        return Err(AppError::BadRequest(format!(
            "artifact zip must be at most {} bytes",
            MAX_ZIP_PROJECT_ARTIFACT_BYTES
        )));
    }

    if !is_likely_zip(&artifact_bytes) {
        return Err(AppError::BadRequest("artifact 必须是 zip 文件".to_string()));
    }
    let manifest = shared::zip_project::parse_zip_project_manifest_from_zip_bytes(&artifact_bytes)?;

    let solution_submission_id = SolutionSubmissionId::try_new(Uuid::new_v4().to_string())
        .map_err(|e| {
            AppError::Internal(format!("generated invalid solution submission id: {e}"))
        })?;
    let job_id = Uuid::new_v4().to_string();
    let artifact_path =
        StorageKey::try_new(format!("solution-submissions/{solution_submission_id}.zip"))?;
    let temporary_artifact_path = StorageKey::try_new(format!(
        "_tmp/solution-submissions/{}-{}.zip",
        solution_submission_id,
        Uuid::new_v4()
    ))?;
    let temporary_artifact_path = state
        .storage
        .put(&temporary_artifact_path, &artifact_bytes)
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
            artifact_path: artifact_path.to_string(),
            language: manifest.runtime.language,
            eval_type,
            explanation: body.explanation.trim().to_string(),
            parent_solution_submission_id: body.parent_solution_submission_id,
            credit_text: body.credit_text.trim().to_string(),
            initial_job_delay_seconds: Some(STAGED_EVALUATION_JOB_DELAY_SECONDS),
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
            cleanup_storage_key(&state, &temporary_artifact_path).await;
            return Err(error);
        }
    };

    if let Err(error) = state
        .storage
        .promote(&temporary_artifact_path, &artifact_path)
        .await
    {
        cleanup_solution_submission_record(&state, &solution_submission.id).await;
        cleanup_storage_key(&state, &temporary_artifact_path).await;
        return Err(error);
    }

    if let Err(error) = db::mark_evaluation_job_ready(&state.db, &job_id).await {
        cleanup_solution_submission_record(&state, &solution_submission.id).await;
        cleanup_storage_key(&state, &artifact_path).await;
        cleanup_storage_key(&state, &temporary_artifact_path).await;
        return Err(error);
    }

    Ok((
        StatusCode::CREATED,
        Json(presenters::present_create_solution_submission(
            &solution_submission,
        )),
    ))
}

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

async fn cleanup_storage_key(state: &AppState, storage_key: &StorageKey) {
    if let Err(error) = state.storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up staged storage object after admission failure"
        );
    }
}

async fn ensure_submission_quota_available(
    state: &AppState,
    agent_id: &str,
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

fn challenge_lifetime_limit(
    admission: &db::PublishedChallengeAdmission,
    eval_type: ScoringMode,
) -> Option<i64> {
    match eval_type {
        ScoringMode::Validation => admission.validation_submission_limit,
        ScoringMode::Official => admission.official_submission_limit,
    }
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
    )))
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
        ),
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
    read_solution_submission_logs(&state, &solution_submission).await
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
    let challenge_name = parse_challenge_name(&name)?;
    ensure_public_result_detail_visible(&state.db, &challenge_name).await?;
    let items = db::list_public_solution_submissions_for_challenge(
        &state.db,
        &challenge_name,
        query.limit(),
    )
    .await?;
    Ok(Json(PublicSolutionSubmissionListResponse { items }))
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_public_result_detail_visible(&state.db, &solution_submission.challenge_name).await?;
    Ok(Json(presenters::present_solution_submission(
        &solution_submission,
        presenters::SolutionSubmissionAudience::Public,
    )))
}

/// Fetch a public redacted result report when the challenge visibility allows it.
pub async fn get_public_solution_submission_result_report(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
) -> Result<Json<SolutionSubmissionResultReportResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_public_result_detail_visible(&state.db, &solution_submission.challenge_name).await?;
    Ok(Json(SolutionSubmissionResultReportResponse {
        solution_submission: presenters::present_solution_submission(
            &solution_submission,
            presenters::SolutionSubmissionAudience::Public,
        ),
    }))
}

/// Fetch public ranking context for a visible submission when the challenge allows it.
pub async fn get_public_solution_submission_ranking_context(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    State(state): State<AppState>,
    Query(query): Query<RankingContextQuery>,
) -> Result<Json<RankingContextResponse>> {
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_ranking_scope_matches_submission(&solution_submission, &query)?;
    let (_challenge, spec) =
        load_challenge_policy(&state.db, &solution_submission.challenge_name).await?;
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
    let solution_submission = db::get_solution_submission_by_id(&state.db, &id).await?;
    let solution_submission = solution_submission.ok_or(AppError::NotFound)?;
    if !solution_submission.visible_after_eval {
        return Err(AppError::NotFound);
    }
    ensure_public_solution_artifact_visible(&state.db, &solution_submission.challenge_name).await?;

    let artifact_path = StorageKey::try_new(&solution_submission.artifact_path)?;
    let artifact_bytes = state.storage.get(&artifact_path).await?;
    let artifact =
        read_solution_submission_artifact_summary(artifact_path.as_str(), artifact_bytes).await?;
    Ok(Json(artifact))
}

/// Fetch leaderboard rows for a challenge.
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>> {
    let challenge_name = parse_challenge_name(&name)?;
    let (challenge, spec) = load_challenge_policy(&state.db, &challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.leaderboard, &spec)?;
    let target = resolve_public_target(&state.db, &challenge_name, query.target.as_deref()).await?;
    let items =
        db::list_leaderboard_entries(&state.db, &challenge_name, &target, query.limit()).await?;
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
    let challenge_name = parse_challenge_name(&name)?;
    let metric_name = parse_metric_name(&query.metric)?;
    let (challenge, spec) = load_challenge_policy(&state.db, &challenge_name).await?;
    ensure_visibility_allows_public(spec.visibility.score_distribution, &spec)?;
    let target = resolve_public_target(&state.db, &challenge_name, query.target.as_deref()).await?;
    let entries = db::list_leaderboard_entries(&state.db, &challenge_name, &target, 10_000).await?;
    let response =
        build_score_distribution_response(challenge.challenge_name, target, metric_name, entries)?;
    Ok(Json(response))
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PublicListQuery {
    limit: Option<i64>,
}

impl PublicListQuery {
    fn limit(self) -> i64 {
        self.limit
            .unwrap_or(DEFAULT_PUBLIC_LIST_LIMIT)
            .clamp(1, MAX_PUBLIC_LIST_LIMIT)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeaderboardQuery {
    limit: Option<i64>,
    target: Option<String>,
}

impl LeaderboardQuery {
    fn limit(&self) -> i64 {
        self.limit
            .unwrap_or(DEFAULT_PUBLIC_LIST_LIMIT)
            .clamp(1, MAX_PUBLIC_LIST_LIMIT)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScoreDistributionQuery {
    target: Option<String>,
    metric: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankingContextQuery {
    challenge_name: ChallengeName,
    target: TargetName,
}

async fn resolve_public_target(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    requested_target: Option<&str>,
) -> Result<TargetName> {
    let challenge = db::get_published_challenge(pool, challenge_name).await?;
    let challenge = challenge.ok_or(AppError::NotFound)?;
    let spec: shared::models::challenge::ChallengeBundleSpec =
        serde_json::from_value(challenge.spec_json)
            .map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(target) = requested_target {
        let target = parse_target(target)?;
        if spec.target(&target).is_some() {
            return Ok(target);
        }
        return Err(AppError::BadRequest(format!(
            "challenge does not support target `{target}`"
        )));
    }

    Err(AppError::BadRequest(
        "target query parameter is required".to_string(),
    ))
}

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

async fn ensure_public_result_detail_visible(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    match spec.visibility.result_detail {
        ChallengeResultDetailVisibility::SubmitterLivePublicLive => Ok(()),
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
            if challenge_has_closed(&spec)? =>
        {
            Ok(())
        }
        ChallengeResultDetailVisibility::SubmitterLivePublicAfterClose
        | ChallengeResultDetailVisibility::SubmitterOnly => Err(AppError::NotFound),
    }
}

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

fn challenge_has_closed(spec: &ChallengeBundleSpec) -> Result<bool> {
    let Some(closes_at) = spec.closes_at.as_deref() else {
        return Ok(false);
    };
    let closes_at = DateTime::parse_from_rfc3339(closes_at)
        .map_err(|e| AppError::Internal(format!("invalid persisted challenge closes_at: {e}")))?
        .with_timezone(&Utc);
    Ok(Utc::now() >= closes_at)
}

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

fn build_score_distribution_response(
    challenge_name: ChallengeName,
    target: TargetName,
    metric_name: MetricName,
    entries: Vec<LeaderboardEntryDto>,
) -> Result<ScoreDistributionResponse> {
    let mut values = entries
        .iter()
        .filter_map(|entry| metric_value_from_leaderboard_entry(entry, &metric_name))
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let count = i64::try_from(values.len())
        .map_err(|_| AppError::Internal("score distribution count overflow".to_string()))?;
    let (min, max, mean, quantiles, histogram) = if values.is_empty() {
        (None, None, None, Vec::new(), Vec::new())
    } else {
        let min = values.first().copied().ok_or_else(|| {
            AppError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let max = values.last().copied().ok_or_else(|| {
            AppError::Internal("score distribution unexpectedly empty".to_string())
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

fn metric_value_from_leaderboard_entry(
    entry: &LeaderboardEntryDto,
    metric_name: &MetricName,
) -> Option<f64> {
    match metric_name.as_str() {
        "rank_score" | "best_rank_score" => Some(entry.best_rank_score),
        "official_score" => entry.official_score,
        _ => entry
            .aggregate_metrics
            .iter()
            .chain(entry.official_metrics.iter())
            .find(|metric| &metric.metric_name == metric_name)
            .map(|metric| metric.value),
    }
}

fn build_quantiles(values: &[f64]) -> Result<Vec<ScoreDistributionQuantileDto>> {
    [
        (0.0, 0usize, 4usize),
        (0.25, 1usize, 4usize),
        (0.5, 2usize, 4usize),
        (0.75, 3usize, 4usize),
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

fn nearest_rank_quantile(values: &[f64], numerator: usize, denominator: usize) -> Result<f64> {
    let max_index = values.len().saturating_sub(1);
    let rounded_index = max_index
        .checked_mul(numerator)
        .and_then(|value| value.checked_add(denominator / 2))
        .and_then(|value| value.checked_div(denominator))
        .ok_or_else(|| AppError::Internal("quantile index overflow".to_string()))?
        .min(max_index);
    values
        .get(rounded_index)
        .copied()
        .ok_or_else(|| AppError::Internal("quantile index out of range".to_string()))
}

fn build_histogram(values: &[f64]) -> Result<Vec<ScoreDistributionBucketDto>> {
    let min = values
        .first()
        .copied()
        .ok_or_else(|| AppError::Internal("histogram values unexpectedly empty".to_string()))?;
    let max = values
        .last()
        .copied()
        .ok_or_else(|| AppError::Internal("histogram values unexpectedly empty".to_string()))?;
    if min == max {
        return Ok(vec![ScoreDistributionBucketDto {
            lower: min,
            upper: max,
            count: i64::try_from(values.len())
                .map_err(|_| AppError::Internal("histogram count overflow".to_string()))?,
        }]);
    }

    let bucket_count = values.len().min(10);
    let width = (max - min) / bucket_count as f64;
    let mut counts = vec![0i64; bucket_count];
    for value in values {
        let index = histogram_bucket_index(*value, min, width, bucket_count)?;
        let count = counts
            .get_mut(index)
            .ok_or_else(|| AppError::Internal("histogram bucket index invalid".to_string()))?;
        *count = count
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("histogram count overflow".to_string()))?;
    }

    let mut buckets = Vec::with_capacity(counts.len());
    for (index, count) in counts.into_iter().enumerate() {
        let lower = min + width * index as f64;
        let upper = match index.checked_add(1) {
            Some(next_index) if next_index == bucket_count => max,
            Some(next_index) => min + width * next_index as f64,
            None => {
                return Err(AppError::Internal(
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

fn histogram_bucket_index(value: f64, min: f64, width: f64, bucket_count: usize) -> Result<usize> {
    for index in 0..bucket_count {
        let next_index = index
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("histogram bucket index overflow".to_string()))?;
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
        .ok_or_else(|| AppError::Internal("histogram bucket count invalid".to_string()))
}

// ---------------------------------------------------------------------------
// Creator routes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CreatorChallengeQuery {
    target: Option<String>,
}

/// Fetch owner-visible aggregate challenge statistics for shortlist decisions.
pub async fn get_creator_challenge_stats(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(name): Path<String>,
    Query(query): Query<CreatorChallengeQuery>,
) -> Result<Json<CreatorChallengeStatsResponse>> {
    let (challenge_name, target) =
        resolve_creator_challenge_scope(&state.db, &creator, &name, query.target.as_deref())
            .await?;
    let response =
        db::get_creator_challenge_stats(&state.db, &challenge_name, target.as_ref()).await?;
    Ok(Json(response))
}

/// Fetch owner-visible participant rows for shortlist decisions.
pub async fn list_creator_challenge_participants(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(name): Path<String>,
    Query(query): Query<CreatorChallengeQuery>,
) -> Result<Json<CreatorChallengeParticipantsResponse>> {
    let (challenge_name, target) =
        resolve_creator_challenge_scope(&state.db, &creator, &name, query.target.as_deref())
            .await?;
    let response =
        db::list_creator_challenge_participants(&state.db, &challenge_name, target.as_ref())
            .await?;
    Ok(Json(response))
}

/// Append a delta-only owner-managed shortlist revision.
pub async fn create_challenge_shortlist_revision(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(name): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateChallengeShortlistRevisionRequest>,
) -> Result<(StatusCode, Json<ChallengeShortlistRevisionResponse>)> {
    let (challenge_name, _) =
        resolve_creator_challenge_scope(&state.db, &creator, &name, None).await?;
    let requested_count = i64::try_from(body.agent_ids_to_add.len())
        .map_err(|_| AppError::BadRequest("shortlist payload is too large".to_string()))?;
    let raw_json = serde_json::to_vec(&body)
        .map_err(|e| AppError::Internal(format!("failed to encode shortlist revision: {e}")))?;
    let agent_ids_to_add = normalize_shortlist_agent_ids(&body.agent_ids_to_add)?;

    let revision_id = Uuid::new_v4().to_string();
    let sha256 = challenge_creation::sha256_hex(&raw_json);
    let storage_key = StorageKey::try_new(format!(
        "challenge-shortlists/{challenge_name}/{revision_id}.json"
    ))?;
    let stored_key = state.storage.put(&storage_key, &raw_json).await?;

    let response = db::create_challenge_shortlist_revision(
        &state.db,
        &db::CreateChallengeShortlistRevisionInput {
            revision_id,
            challenge_name,
            uploader_agent_id: creator.agent_id,
            storage_key: stored_key.clone(),
            sha256,
            requested_count,
            agent_ids_to_add,
        },
    )
    .await;

    match response {
        Ok(response) => Ok((StatusCode::CREATED, Json(response))),
        Err(error) => {
            cleanup_storage_key(&state, &stored_key).await;
            Err(error)
        }
    }
}

/// Fetch the effective owner-managed shortlist union.
pub async fn get_challenge_shortlist(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(name): Path<String>,
) -> Result<Json<ChallengeShortlistResponse>> {
    let (challenge_name, _) =
        resolve_creator_challenge_scope(&state.db, &creator, &name, None).await?;
    let response = db::list_challenge_shortlist(&state.db, &challenge_name).await?;
    Ok(Json(response))
}

async fn resolve_creator_challenge_scope(
    pool: &sqlx::PgPool,
    creator: &CreatorAuth,
    raw_challenge_name: &str,
    requested_target: Option<&str>,
) -> Result<(ChallengeName, Option<TargetName>)> {
    let challenge_name = parse_challenge_name(raw_challenge_name)?;
    let challenge = db::get_published_challenge(pool, &challenge_name).await?;
    let challenge = challenge.ok_or(AppError::NotFound)?;
    if !db::agent_owns_challenge(pool, &challenge.challenge_name, &creator.agent_id).await? {
        return Err(AppError::Forbidden(
            "agent is not an owner of this challenge".to_string(),
        ));
    }

    let target = resolve_target_from_spec(&challenge.spec_json, requested_target)?;
    Ok((challenge.challenge_name, target))
}

fn resolve_target_from_spec(
    spec_json: &serde_json::Value,
    requested_target: Option<&str>,
) -> Result<Option<TargetName>> {
    let Some(target) = requested_target else {
        return Ok(None);
    };

    let spec: ChallengeBundleSpec =
        serde_json::from_value(spec_json.clone()).map_err(|e| AppError::Internal(e.to_string()))?;
    let target = parse_target(target)?;
    if spec.target(&target).is_some() {
        return Ok(Some(target));
    }
    Err(AppError::BadRequest(format!(
        "challenge does not support target `{target}`"
    )))
}

fn normalize_shortlist_agent_ids(agent_ids: &[String]) -> Result<Vec<String>> {
    let mut unique = std::collections::BTreeSet::new();
    for agent_id in agent_ids {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            return Err(AppError::BadRequest(
                "agent_ids_to_add must not contain empty agent ids".to_string(),
            ));
        }
        let uuid = Uuid::parse_str(agent_id).map_err(|_| {
            AppError::BadRequest(
                "agent_ids_to_add must contain canonical UUID agent ids".to_string(),
            )
        })?;
        if uuid.to_string() != agent_id {
            return Err(AppError::BadRequest(
                "agent_ids_to_add must contain canonical UUID agent ids".to_string(),
            ));
        }
        unique.insert(agent_id.to_string());
    }
    if unique.is_empty() {
        return Err(AppError::BadRequest(
            "agent_ids_to_add must contain at least one agent id".to_string(),
        ));
    }
    Ok(unique.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Admin routes
// ---------------------------------------------------------------------------

/// List challenge shells and published benchmark contracts for admins.
pub async fn list_admin_challenges(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<shared::models::challenge::AdminChallengeListResponse>> {
    let items = db::list_admin_challenges(&state.db).await?;
    Ok(Json(
        shared::models::challenge::AdminChallengeListResponse { items },
    ))
}

/// Create or update a challenge shell.
pub async fn create_challenge(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<CreateChallengeRequest>,
) -> Result<(
    StatusCode,
    Json<shared::models::challenge::ChallengeAdminResponse>,
)> {
    let challenge =
        db::create_or_update_challenge(&state.db, &body.name, &body.title, &body.summary)
            .await
            .map_err(|e| match e {
                AppError::Database(sqlx::Error::Database(db_err))
                    if db_err.is_unique_violation() =>
                {
                    AppError::Conflict
                }
                _ => e,
            })?;
    Ok((StatusCode::CREATED, Json(challenge)))
}

/// Validate and publish a challenge bundle.
pub async fn publish_challenge(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(challenge_name): Path<String>,
    ValidatedJson(body): ValidatedJson<PublishChallengeRequest>,
) -> Result<(StatusCode, Json<PublishChallengeResponse>)> {
    let challenge_name = parse_challenge_name(&challenge_name)?;
    let bundle_path = if FsPath::new(&body.bundle_path).is_absolute() {
        PathBuf::from(&body.bundle_path)
    } else {
        FsPath::new(&state.config.challenges_root).join(&body.bundle_path)
    };
    let bundle_path = AdminBundlePath::from_existing_dir(&bundle_path)?;

    challenge_bundle::validate_challenge_bundle(bundle_path.as_path()).await?;
    let spec = challenge_bundle::read_challenge_bundle_spec(bundle_path.as_path()).await?;
    if state.config.require_digest_pinned_images {
        challenge_bundle::validate_digest_pinned_images(&spec)?;
    }

    if spec.challenge_name != challenge_name {
        return Err(AppError::BadRequest(format!(
            "challenge bundle id mismatch: expected {}, got {}",
            challenge_name, spec.challenge_name
        )));
    }
    if spec.eligibility.eligibility_type == ChallengeEligibilityType::PrivateShortlist {
        return Err(AppError::BadRequest(
            "private_shortlist challenges must be published through the creator draft flow so an owner can manage the shortlist"
                .to_string(),
        ));
    }

    let managed_bundle_path =
        copy_admin_bundle_to_managed_storage(&state.config, bundle_path.as_path()).await?;
    let statement_path = managed_bundle_path.join("statement.md");

    let challenge = db::publish_challenge(
        &state.db,
        &challenge_name,
        &managed_bundle_path.to_string_lossy(),
        &statement_path.to_string_lossy(),
        &spec,
        &spec.challenge_title,
        &spec.challenge_summary,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(challenge)))
}

async fn copy_admin_bundle_to_managed_storage(
    config: &Config,
    source: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let source_digest = challenge_bundle::challenge_bundle_tree_sha256(source).await?;
    let target = std::path::Path::new(&config.storage_root)
        .join("challenge-bundles")
        .join("admin")
        .join(&source_digest);
    if !tokio::fs::try_exists(&target).await? {
        let temp_target = target.with_extension(format!("tmp-{}", Uuid::new_v4()));
        if tokio::fs::try_exists(&temp_target).await? {
            tokio::fs::remove_dir_all(&temp_target).await?;
        }
        challenge_bundle::copy_challenge_bundle_dir(source, &temp_target, true).await?;
        match tokio::fs::rename(&temp_target, &target).await {
            Ok(()) => {}
            Err(error) if tokio::fs::try_exists(&target).await? => {
                tokio::fs::remove_dir_all(&temp_target).await.ok();
                tracing::debug!(
                    target = %target.display(),
                    error = %error,
                    "managed bundle target already exists after concurrent copy"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }

    challenge_bundle::validate_challenge_bundle(&target).await?;
    let managed_digest = challenge_bundle::challenge_bundle_tree_sha256(&target).await?;
    if managed_digest != source_digest {
        return Err(AppError::Validation(format!(
            "managed bundle copy digest mismatch for {}",
            target.display()
        )));
    }

    Ok(target)
}

/// List recent solution submissions for admin operations.
pub async fn list_admin_solution_submissions(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminSolutionSubmissionListResponse>> {
    let items = db::list_admin_solution_submissions(&state.db, 100).await?;
    Ok(Json(AdminSolutionSubmissionListResponse { items }))
}

/// List latest service heartbeats for admin operations.
pub async fn list_admin_service_heartbeats(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminServiceHeartbeatListResponse>> {
    let items = db::list_service_heartbeats(&state.db).await?;
    Ok(Json(AdminServiceHeartbeatListResponse { items }))
}

/// Show configured quota limits and current queue usage for admin capacity review.
pub async fn get_admin_capacity(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<AdminCapacityResponse>> {
    let active_agents = db::count_active_agents(&state.db).await?;
    let active_validation_jobs =
        db::count_active_evaluation_jobs(&state.db, ScoringMode::Validation).await?;
    let active_official_jobs =
        db::count_active_evaluation_jobs(&state.db, ScoringMode::Official).await?;

    Ok(Json(AdminCapacityResponse {
        quota_window_seconds: SUBMISSION_QUOTA_WINDOW_SECONDS,
        quotas: AdminQuotaSettingsDto {
            validation_runs_per_agent_challenge_day: state
                .config
                .validation_runs_per_agent_challenge_day,
            official_runs_per_agent_challenge_day: state
                .config
                .official_runs_per_agent_challenge_day,
            max_active_official_jobs: state.config.max_active_official_jobs,
            max_active_agents: state.config.max_active_agents,
        },
        usage: AdminCapacityUsageDto {
            active_agents,
            active_validation_jobs,
            active_official_jobs,
        },
    }))
}

/// Queue an official rejudge for an existing solution submission.
pub async fn rejudge(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: Uuid::new_v4().to_string(),
            solution_submission_id: id,
            eval_type: ScoringMode::Official,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            solution_submission_id: job.solution_submission_id,
            target: job.target,
            eval_type: ScoringMode::Official.as_str().to_string(),
            status: job.status,
        }),
    ))
}

/// Queue an official private benchmark run for an existing solution submission.
pub async fn official_run(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<EvaluationJobResponse>)> {
    let job = db::queue_evaluation_job(
        &state.db,
        &QueueEvaluationJobInput {
            job_id: Uuid::new_v4().to_string(),
            solution_submission_id: id,
            eval_type: ScoringMode::Official,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(EvaluationJobResponse {
            job_id: job.id,
            solution_submission_id: job.solution_submission_id,
            target: job.target,
            eval_type: ScoringMode::Official.as_str().to_string(),
            status: job.status,
        }),
    ))
}

/// Hide a solution submission from public views and repair leaderboard state.
pub async fn hide_solution_submission(
    SolutionSubmissionPath(id): SolutionSubmissionPath,
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<HideSolutionSubmissionResponse>> {
    db::hide_solution_submission(&state.db, &id).await?;
    Ok(Json(HideSolutionSubmissionResponse { id, hidden: true }))
}

/// Disable an agent and revoke its tokens.
pub async fn disable_agent(
    _admin: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DisableAgentResponse>> {
    db::disable_agent(&state.db, &id).await?;
    Ok(Json(DisableAgentResponse {
        id,
        status: "disabled".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.decode(input.trim()).ok()
}

fn is_likely_zip(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04])
        || bytes.starts_with(&[0x50, 0x4b, 0x05, 0x06])
        || bytes.starts_with(&[0x50, 0x4b, 0x07, 0x08])
}

/// Summarize a solution submission ZIP for safe public code browsing.
pub async fn read_solution_submission_artifact_summary(
    artifact_key: &str,
    artifact_bytes: Vec<u8>,
) -> Result<SolutionSubmissionArtifactResponse> {
    let archive_size = artifact_bytes.len() as u64;
    if archive_size > MAX_ZIP_PROJECT_ARTIFACT_BYTES {
        return Err(AppError::BadRequest(format!(
            "artifact zip must be at most {} bytes",
            MAX_ZIP_PROJECT_ARTIFACT_BYTES
        )));
    }

    let artifact_key = artifact_key.to_string();
    tokio::task::spawn_blocking(move || {
        read_solution_submission_artifact_summary_blocking(&artifact_key, artifact_bytes)
    })
    .await
    .map_err(|e| AppError::Internal(format!("artifact summary task failed: {e}")))?
}

async fn read_solution_submission_logs(
    state: &AppState,
    solution_submission: &db::SolutionSubmissionRecord,
) -> Result<Json<SolutionSubmissionLogsResponse>> {
    const MAX_LOG_RESPONSE_BYTES: usize = 200_000;

    let log_path = solution_submission
        .official_evaluation
        .as_ref()
        .and_then(|evaluation| evaluation.log_path.clone())
        .or_else(|| {
            solution_submission
                .validation_evaluation
                .as_ref()
                .and_then(|evaluation| evaluation.log_path.clone())
        });

    let Some(log_path) = log_path else {
        return Ok(Json(SolutionSubmissionLogsResponse {
            solution_submission_id: solution_submission.id.clone(),
            log_path: None,
            content: None,
            truncated: false,
        }));
    };

    let log_key = StorageKey::try_new(&log_path)?;
    let bytes = state.storage.get(&log_key).await?;
    let truncated = bytes.len() > MAX_LOG_RESPONSE_BYTES;
    let visible_bytes = if truncated {
        bytes
            .get(..MAX_LOG_RESPONSE_BYTES)
            .ok_or_else(|| AppError::Internal("log truncation range invalid".to_string()))?
    } else {
        bytes.as_slice()
    };
    let content = String::from_utf8_lossy(visible_bytes).to_string();

    Ok(Json(SolutionSubmissionLogsResponse {
        solution_submission_id: solution_submission.id.clone(),
        log_path: Some(log_path),
        content: Some(content),
        truncated,
    }))
}

fn read_solution_submission_artifact_summary_blocking(
    artifact_key: &str,
    artifact_bytes: Vec<u8>,
) -> Result<SolutionSubmissionArtifactResponse> {
    let archive_size = artifact_bytes.len();
    let reader = std::io::Cursor::new(artifact_bytes);
    let mut archive = zip::ZipArchive::new(reader)?;

    if archive.len() > MAX_ZIP_PROJECT_FILE_COUNT {
        return Err(AppError::BadRequest(format!(
            "artifact zip must contain at most {} entries",
            MAX_ZIP_PROJECT_FILE_COUNT
        )));
    }

    let mut files = Vec::new();
    let mut total_uncompressed_size = 0u64;
    let mut total_inline_text_bytes = 0u64;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }

        let entry_path = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string());
        let Some(entry_path) = entry_path else {
            continue;
        };

        let size = file.size();
        total_uncompressed_size = total_uncompressed_size
            .checked_add(size)
            .ok_or_else(|| AppError::BadRequest("artifact zip is too large".to_string()))?;
        if total_uncompressed_size > MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES {
            return Err(AppError::BadRequest(format!(
                "artifact zip must expand to at most {} bytes",
                MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES
            )));
        }

        let mut buf = Vec::new();
        let compressed_size = i64::try_from(file.compressed_size()).map_err(|_| {
            AppError::BadRequest(
                "artifact ZIP entry compressed size exceeds supported range".to_string(),
            )
        })?;
        let projected_inline_text_bytes = total_inline_text_bytes.checked_add(size);
        let should_try_inline = size <= MAX_INLINE_TEXT_BYTES
            && projected_inline_text_bytes
                .is_some_and(|projected| projected <= MAX_TOTAL_INLINE_TEXT_BYTES);
        if should_try_inline {
            std::io::Read::read_to_end(&mut file, &mut buf)?;
        }

        let inline_text = if should_try_inline {
            std::str::from_utf8(&buf).ok()
        } else {
            None
        };
        let is_text = inline_text.is_some() || is_text_like_path(&entry_path);

        let content = if let Some(text) = inline_text {
            total_inline_text_bytes = total_inline_text_bytes
                .checked_add(u64::try_from(buf.len()).map_err(|_| {
                    AppError::BadRequest(
                        "artifact inline text size exceeds supported range".to_string(),
                    )
                })?)
                .ok_or_else(|| {
                    AppError::BadRequest("artifact inline text budget overflow".to_string())
                })?;
            Some(text.to_string())
        } else {
            None
        };

        files.push(SolutionSubmissionArtifactFileDto {
            path: entry_path.clone(),
            size: i64::try_from(size).map_err(|_| {
                AppError::BadRequest("artifact ZIP entry size exceeds supported range".to_string())
            })?,
            compressed_size,
            language: Some(infer_language(&entry_path)),
            is_text,
            content,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SolutionSubmissionArtifactResponse {
        archive_name: std::path::Path::new(artifact_key)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        archive_size: i64::try_from(archive_size).map_err(|_| {
            AppError::BadRequest("artifact ZIP size exceeds supported range".to_string())
        })?,
        file_count: i64::try_from(files.len()).map_err(|_| {
            AppError::BadRequest("artifact ZIP file count exceeds supported range".to_string())
        })?,
        total_uncompressed_size: i64::try_from(total_uncompressed_size).map_err(|_| {
            AppError::BadRequest("artifact ZIP expanded size exceeds supported range".to_string())
        })?,
        files,
    })
}

fn is_text_like_path(file_path: &str) -> bool {
    !matches!(infer_language(file_path).as_str(), "plaintext")
        || matches!(
            std::path::Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("txt")
        )
}

fn infer_language(file_path: &str) -> String {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "py" => "python",
        "json" => "json",
        "md" => "markdown",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "yml" | "yaml" => "yaml",
        "toml" => "ini",
        "sh" => "shell",
        "sql" => "sql",
        "txt" => "plaintext",
        _ => "plaintext",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use super::*;

    fn temp_zip_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-{name}-{}.zip", Uuid::new_v4()))
    }

    fn write_zip(path: &PathBuf, entries: Vec<(String, Vec<u8>)>) {
        let file = std::fs::File::create(path).expect("failed to create test zip");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            archive
                .start_file(name, options)
                .expect("failed to start zip entry");
            archive
                .write_all(&bytes)
                .expect("failed to write zip entry");
        }

        archive.finish().expect("failed to finish test zip");
    }

    #[tokio::test]
    async fn artifact_summary_skips_unsafe_entry_names() {
        let path = temp_zip_path("unsafe-entry");
        write_zip(
            &path,
            vec![
                ("../escape.py".to_string(), b"print('bad')\n".to_vec()),
                ("main.py".to_string(), b"print('ok')\n".to_vec()),
            ],
        );

        let bytes = std::fs::read(&path).expect("failed to read test zip");
        let summary = read_solution_submission_artifact_summary(&path.to_string_lossy(), bytes)
            .await
            .expect("summary should succeed");
        drop(std::fs::remove_file(path));

        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files[0].path, "main.py");
    }

    #[tokio::test]
    async fn artifact_summary_rejects_too_many_entries() {
        let path = temp_zip_path("too-many");
        let entries = (0..=MAX_ZIP_PROJECT_FILE_COUNT)
            .map(|i| (format!("file-{i}.txt"), Vec::new()))
            .collect();
        write_zip(&path, entries);

        let bytes = std::fs::read(&path).expect("failed to read test zip");
        let result =
            read_solution_submission_artifact_summary(&path.to_string_lossy(), bytes).await;
        drop(std::fs::remove_file(path));

        assert!(
            matches!(result, Err(AppError::BadRequest(message)) if message.contains("at most"))
        );
    }

    #[tokio::test]
    async fn artifact_summary_does_not_inline_large_text_entries() {
        let path = temp_zip_path("large-text");
        write_zip(
            &path,
            vec![(
                "main.py".to_string(),
                vec![b'a'; (MAX_INLINE_TEXT_BYTES + 1) as usize],
            )],
        );

        let bytes = std::fs::read(&path).expect("failed to read test zip");
        let summary = read_solution_submission_artifact_summary(&path.to_string_lossy(), bytes)
            .await
            .expect("summary should succeed");
        drop(std::fs::remove_file(path));

        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files[0].path, "main.py");
        assert!(summary.files[0].is_text);
        assert!(summary.files[0].content.is_none());
    }
}
