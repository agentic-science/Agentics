//! Service request and actor types for challenge review record workflows.

use agentics_domain::models::challenge_creation::{
    ChallengeReviewDecisionRequest, CreateChallengeReviewRecordRequest,
    UploadChallengePrivateAssetRequest, ValidateChallengeReviewRecordRequest,
};
use agentics_domain::models::ids::{AgentId, ChallengeReviewRecordId};

/// Authenticated creator identity passed in from the HTTP boundary.
#[derive(Debug, Clone)]
pub struct ChallengeReviewRecordCreator {
    pub agent_id: AgentId,
    pub github_user_id: i64,
    pub github_login: String,
}

/// Authenticated admin identity passed in from the HTTP boundary.
#[derive(Debug, Clone)]
pub struct ChallengeReviewRecordAdmin {
    pub username: String,
}

/// Request to create a GitHub-backed challenge review record.
#[derive(Debug, Clone)]
pub struct CreateChallengeReviewRecordServiceRequest {
    pub creator: ChallengeReviewRecordCreator,
    pub body: CreateChallengeReviewRecordRequest,
}

/// Request to upload one private asset ZIP for a review_record.
#[derive(Debug, Clone)]
pub struct UploadChallengePrivateAssetServiceRequest {
    pub creator_agent_id: AgentId,
    pub review_record_id: ChallengeReviewRecordId,
    pub body: UploadChallengePrivateAssetRequest,
}

/// Request to validate a review_record against a local checkout.
#[derive(Debug, Clone)]
pub struct ValidateChallengeReviewRecordServiceRequest {
    pub admin: ChallengeReviewRecordAdmin,
    pub review_record_id: ChallengeReviewRecordId,
    pub body: ValidateChallengeReviewRecordRequest,
}

/// Request to approve, reject, or abandon a review_record.
#[derive(Debug, Clone)]
pub struct ChallengeReviewDecisionServiceRequest {
    pub admin: ChallengeReviewRecordAdmin,
    pub review_record_id: ChallengeReviewRecordId,
    pub body: ChallengeReviewDecisionRequest,
}

/// Request to publish an approved review_record.
#[derive(Debug, Clone)]
pub struct PublishChallengeReviewRecordServiceRequest {
    pub admin: ChallengeReviewRecordAdmin,
    pub review_record_id: ChallengeReviewRecordId,
    pub body: ValidateChallengeReviewRecordRequest,
}
