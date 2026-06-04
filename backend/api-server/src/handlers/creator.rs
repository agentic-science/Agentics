//! Creator-owned challenge statistics, participant, and shortlist routes.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::error::ApiResult as Result;
use agentics_domain::models::auth::{
    CreateCreatorApiTokenRequest, CreatorApiTokenCreatedResponse, CreatorApiTokenListResponse,
    RevokeCreatorApiTokenResponse,
};
use agentics_domain::models::ids::CreatorApiTokenId;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreateChallengeShortlistRevisionRequest, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse,
};
use agentics_services::creator as creator_service;

use crate::extractors::{CreatorAuth, CreatorWebAuth, ValidatedJson};
use crate::state::AppState;

use super::parse_request_value;

/// Optional target query used by creator-owned challenge views.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatorChallengeQuery {
    target: Option<String>,
}

/// List API tokens owned by the signed-in creator.
pub async fn list_creator_api_tokens(
    State(state): State<AppState>,
    CreatorWebAuth(creator): CreatorWebAuth,
) -> Result<Json<CreatorApiTokenListResponse>> {
    Ok(Json(
        creator_service::list_creator_api_tokens(&state.db, &creator.human_id).await?,
    ))
}

/// Create a creator API token for CLI workflows.
pub async fn create_creator_api_token(
    State(state): State<AppState>,
    CreatorWebAuth(creator): CreatorWebAuth,
    ValidatedJson(body): ValidatedJson<CreateCreatorApiTokenRequest>,
) -> Result<(StatusCode, Json<CreatorApiTokenCreatedResponse>)> {
    Ok((
        StatusCode::CREATED,
        Json(creator_service::create_creator_api_token(&state.db, &creator.human_id, body).await?),
    ))
}

/// Revoke one API token owned by the signed-in creator.
pub async fn revoke_creator_api_token(
    State(state): State<AppState>,
    CreatorWebAuth(creator): CreatorWebAuth,
    Path(id): Path<String>,
) -> Result<Json<RevokeCreatorApiTokenResponse>> {
    let id = CreatorApiTokenId::try_new(id)
        .map_err(|error| agentics_error::ServiceError::BadRequest(error.to_string()))?;
    Ok(Json(
        creator_service::revoke_creator_api_token(&state.db, &creator.human_id, &id).await?,
    ))
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
        &creator.human_id,
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
        &creator.human_id,
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
        &creator.human_id,
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
        creator_service::get_challenge_shortlist(&state.db, &creator.human_id, &challenge_name)
            .await?;
    Ok(Json(response))
}
