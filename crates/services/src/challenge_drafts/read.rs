use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeDraftListResponse,
    CreatorChallengeDraftResponse,
};
use agentics_domain::models::ids::{AgentId, ChallengeDraftId};
use agentics_error::{Result, ServiceError};
use agentics_persistence::Repositories;

use super::presentation::{admin_private_asset_response, draft_response};

/// Fetch a challenge draft owned by the authenticated agent.
pub async fn get_challenge_draft(
    pool: &sqlx::PgPool,
    creator_agent_id: &AgentId,
    draft_id: &ChallengeDraftId,
) -> Result<CreatorChallengeDraftResponse> {
    let draft = Repositories::new(pool)
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    let draft = draft_response(draft);
    if draft.creator_agent_id != *creator_agent_id {
        return Err(ServiceError::NotFound);
    }
    Ok(draft.into())
}

/// List GitHub-backed challenge drafts for admin review.
pub async fn list_admin_challenge_drafts(
    pool: &sqlx::PgPool,
) -> Result<ChallengeDraftListResponse> {
    let items = Repositories::new(pool)
        .challenge_drafts()
        .list(100)
        .await?
        .into_iter()
        .map(draft_response)
        .collect();
    Ok(ChallengeDraftListResponse { items })
}

/// List every private asset lifecycle record for one draft for admin review.
pub async fn list_admin_challenge_draft_private_assets(
    pool: &sqlx::PgPool,
    draft_id: &ChallengeDraftId,
) -> Result<AdminChallengePrivateAssetListResponse> {
    let repos = Repositories::new(pool);
    repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    let items = repos
        .challenge_drafts()
        .list_private_asset_states(draft_id.as_str())
        .await?
        .into_iter()
        .map(admin_private_asset_response)
        .collect();
    Ok(AdminChallengePrivateAssetListResponse { items })
}
