//! Service request and actor types for challenge draft workflows.

use agentics_domain::models::challenge_creation::{
    CreateChallengeDraftRequest, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
use agentics_domain::models::ids::{AgentId, ChallengeDraftId};

/// Authenticated creator identity passed in from the HTTP boundary.
#[derive(Debug, Clone)]
pub struct ChallengeDraftCreator {
    pub agent_id: AgentId,
    pub github_user_id: i64,
    pub github_login: String,
}

/// Authenticated admin identity passed in from the HTTP boundary.
#[derive(Debug, Clone)]
pub struct ChallengeDraftAdmin {
    pub username: String,
}

/// Request to create a GitHub-backed challenge draft.
#[derive(Debug, Clone)]
pub struct CreateChallengeDraftServiceRequest {
    pub creator: ChallengeDraftCreator,
    pub body: CreateChallengeDraftRequest,
}

/// Request to upload one private asset ZIP for a draft.
#[derive(Debug, Clone)]
pub struct UploadChallengePrivateAssetServiceRequest {
    pub creator_agent_id: AgentId,
    pub draft_id: ChallengeDraftId,
    pub body: UploadChallengePrivateAssetRequest,
}

/// Request to validate a draft against a local checkout.
#[derive(Debug, Clone)]
pub struct ValidateChallengeDraftServiceRequest {
    pub admin: ChallengeDraftAdmin,
    pub draft_id: ChallengeDraftId,
    pub body: ValidateChallengeDraftRequest,
}

/// Request to approve, reject, or abandon a draft.
#[derive(Debug, Clone)]
pub struct ReviewChallengeDraftServiceRequest {
    pub admin: ChallengeDraftAdmin,
    pub draft_id: ChallengeDraftId,
    pub body: ReviewChallengeDraftRequest,
}

/// Request to publish an approved draft.
#[derive(Debug, Clone)]
pub struct PublishChallengeDraftServiceRequest {
    pub admin: ChallengeDraftAdmin,
    pub draft_id: ChallengeDraftId,
    pub body: ValidateChallengeDraftRequest,
}
