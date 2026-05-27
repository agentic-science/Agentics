//! Creator-owned challenge statistics, participant, and shortlist routes.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::error::ApiResult as Result;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreateChallengeShortlistRevisionRequest, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse,
};
use agentics_services::creator as creator_service;

use crate::extractors::{CreatorAuth, ValidatedJson};
use crate::state::AppState;

use super::parse_request_value;

/// Optional target query used by creator-owned challenge views.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatorChallengeQuery {
    target: Option<String>,
}

/// Fetch owner-visible aggregate challenge statistics for shortlist decisions.
pub async fn get_creator_challenge_stats(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(challenge_name): Path<String>,
    Query(query): Query<CreatorChallengeQuery>,
) -> Result<Json<CreatorChallengeStatsResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    let response = creator_service::get_creator_challenge_stats(
        &state.db,
        &creator.agent_id,
        &challenge_name,
        query.target.as_deref(),
    )
    .await?;
    Ok(Json(response))
}

/// Fetch owner-visible participant rows for shortlist decisions.
pub async fn list_creator_challenge_participants(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(challenge_name): Path<String>,
    Query(query): Query<CreatorChallengeQuery>,
) -> Result<Json<CreatorChallengeParticipantsResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    let response = creator_service::list_creator_challenge_participants(
        &state.db,
        &creator.agent_id,
        &challenge_name,
        query.target.as_deref(),
    )
    .await?;
    Ok(Json(response))
}

/// Append a delta-only owner-managed shortlist revision.
pub async fn create_challenge_shortlist_revision(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(challenge_name): Path<String>,
    ValidatedJson(body): ValidatedJson<CreateChallengeShortlistRevisionRequest>,
) -> Result<(StatusCode, Json<ChallengeShortlistRevisionResponse>)> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    let response = creator_service::create_challenge_shortlist_revision(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        &creator.agent_id,
        &challenge_name,
        body,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// Fetch the effective owner-managed shortlist union.
pub async fn get_challenge_shortlist(
    State(state): State<AppState>,
    creator: CreatorAuth,
    Path(challenge_name): Path<String>,
) -> Result<Json<ChallengeShortlistResponse>> {
    let challenge_name = parse_request_value::<ChallengeName>(&challenge_name)?;
    let response =
        creator_service::get_challenge_shortlist(&state.db, &creator.agent_id, &challenge_name)
            .await?;
    Ok(Json(response))
}
