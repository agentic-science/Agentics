//! Creator-owned challenge statistics, participant, and shortlist routes.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use shared::challenge_creation;
use shared::db;
use shared::error::{AppError, Result};
use shared::models::challenge::ChallengeBundleSpec;
use shared::models::ids::{AgentId, ChallengeShortlistRevisionId};
use shared::models::names::{ChallengeName, TargetName};
use shared::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreateChallengeShortlistRevisionRequest, CreatorChallengeParticipantsResponse,
    CreatorChallengeStatsResponse,
};
use shared::storage::StorageKey;

use crate::extractors::{CreatorAuth, ValidatedJson};
use crate::state::AppState;

use super::{cleanup_storage_key, parse_request_value};

/// Optional target query used by creator-owned challenge views.
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

    let revision_id = ChallengeShortlistRevisionId::generate();
    let sha256 = challenge_creation::sha256_digest(&raw_json);
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

/// Resolve and authorize a creator-owned challenge plus optional target filter.
async fn resolve_creator_challenge_scope(
    pool: &sqlx::PgPool,
    creator: &CreatorAuth,
    raw_challenge_name: &str,
    requested_target: Option<&str>,
) -> Result<(ChallengeName, Option<TargetName>)> {
    let challenge_name = parse_request_value::<ChallengeName>(raw_challenge_name)?;
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

/// Validate an optional target query against a published challenge spec.
fn resolve_target_from_spec(
    spec_json: &serde_json::Value,
    requested_target: Option<&str>,
) -> Result<Option<TargetName>> {
    let Some(target) = requested_target else {
        return Ok(None);
    };

    let spec: ChallengeBundleSpec =
        serde_json::from_value(spec_json.clone()).map_err(|e| AppError::Internal(e.to_string()))?;
    let target = parse_request_value::<TargetName>(target)?;
    if spec.target(&target).is_some() {
        return Ok(Some(target));
    }
    Err(AppError::BadRequest(format!(
        "challenge does not support target `{target}`"
    )))
}

/// Deduplicate a shortlist delta and reject empty uploads before persistence.
fn normalize_shortlist_agent_ids(agent_ids: &[AgentId]) -> Result<Vec<AgentId>> {
    let mut unique = std::collections::BTreeSet::new();
    for agent_id in agent_ids {
        unique.insert(agent_id.clone());
    }
    if unique.is_empty() {
        return Err(AppError::BadRequest(
            "agent_ids_to_add must contain at least one agent id".to_string(),
        ));
    }
    Ok(unique.into_iter().collect())
}
