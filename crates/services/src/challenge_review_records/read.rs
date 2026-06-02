use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeReviewRecordListResponse,
    CreatorChallengeReviewRecordResponse,
};
use agentics_domain::models::ids::{AgentId, ChallengeReviewRecordId};
use agentics_error::{Result, ServiceError};
use agentics_persistence::Repositories;

use super::presentation::{admin_private_asset_response, review_record_response};

/// Fetch a challenge review record owned by the authenticated agent.
pub async fn get_challenge_review_record(
    pool: &sqlx::PgPool,
    creator_agent_id: &AgentId,
    review_record_id: &ChallengeReviewRecordId,
) -> Result<CreatorChallengeReviewRecordResponse> {
    let review_record = Repositories::new(pool)
        .challenge_review_records()
        .get(review_record_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    let review_record = review_record_response(review_record);
    if review_record.creator_agent_id != *creator_agent_id {
        return Err(ServiceError::NotFound);
    }
    Ok(review_record.into())
}

/// List GitHub-backed challenge review records for admin review.
pub async fn list_admin_challenge_review_records(
    pool: &sqlx::PgPool,
) -> Result<ChallengeReviewRecordListResponse> {
    let items = Repositories::new(pool)
        .challenge_review_records()
        .list(100)
        .await?
        .into_iter()
        .map(review_record_response)
        .collect();
    Ok(ChallengeReviewRecordListResponse { items })
}

/// List every private asset lifecycle record for one review_record for admin review.
pub async fn list_admin_challenge_review_record_private_assets(
    pool: &sqlx::PgPool,
    review_record_id: &ChallengeReviewRecordId,
) -> Result<AdminChallengePrivateAssetListResponse> {
    let repos = Repositories::new(pool);
    repos
        .challenge_review_records()
        .get(review_record_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    let items = repos
        .challenge_review_records()
        .list_private_asset_states(review_record_id.as_str())
        .await?
        .into_iter()
        .map(admin_private_asset_response)
        .collect();
    Ok(AdminChallengePrivateAssetListResponse { items })
}
