//! Challenge draft response DTOs.

use serde::{Deserialize, Serialize};

use crate::models::github::GithubPullRequestNumber;
use crate::models::hashes::{GitCommitSha, Sha256Digest};
use crate::models::ids::{AgentId, ChallengeDraftId, ChallengeDraftValidationRecordId};
use crate::models::names::ChallengeName;
use crate::models::paths::RepoRelativePath;
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};

use super::lifecycle::{ChallengeDraftStatus, ChallengeDraftValidationStatus};
use super::manifest::{ChallengeCreationManifest, ChallengeCreationRequestKind};
use super::private_assets::ChallengePrivateAssetResponse;

/// API response for one validation record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDraftValidationRecordResponse {
    pub id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub repository_path: String,
    pub manifest_sha256: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<Sha256Digest>,
    pub created_at: String,
}

/// Creator-facing validation record response without server-local checkout paths.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeDraftValidationRecordResponse {
    pub id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub manifest_sha256: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<Sha256Digest>,
    pub created_at: String,
}

/// API response for one challenge draft.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDraftResponse {
    pub id: ChallengeDraftId,
    pub challenge_name: ChallengeName,
    pub request: ChallengeCreationRequestKind,
    pub status: ChallengeDraftStatus,
    pub creator_agent_id: AgentId,
    pub creator_github_user_id: i64,
    pub creator_github_login: String,
    pub repo_url: GithubRepoRemote,
    pub pr_number: GithubPullRequestNumber,
    pub pr_url: GithubPullRequestUrl,
    pub commit_sha: GitCommitSha,
    pub challenge_path: RepoRelativePath,
    pub manifest_sha256: Sha256Digest,
    pub manifest: ChallengeCreationManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_bundle_sha256: Option<Sha256Digest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_bundle_sha256: Option<Sha256Digest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_repository_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_challenge_name: Option<ChallengeName>,
    #[serde(default)]
    pub private_assets: Vec<ChallengePrivateAssetResponse>,
    #[serde(default)]
    pub validation_records: Vec<ChallengeDraftValidationRecordResponse>,
    pub created_at: String,
    pub updated_at: String,
}

/// Creator-facing response for one challenge draft.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeDraftResponse {
    pub id: ChallengeDraftId,
    pub challenge_name: ChallengeName,
    pub request: ChallengeCreationRequestKind,
    pub status: ChallengeDraftStatus,
    pub creator_agent_id: AgentId,
    pub creator_github_user_id: i64,
    pub creator_github_login: String,
    pub repo_url: GithubRepoRemote,
    pub pr_number: GithubPullRequestNumber,
    pub pr_url: GithubPullRequestUrl,
    pub commit_sha: GitCommitSha,
    pub challenge_path: RepoRelativePath,
    pub manifest_sha256: Sha256Digest,
    pub manifest: ChallengeCreationManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_bundle_sha256: Option<Sha256Digest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_bundle_sha256: Option<Sha256Digest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_challenge_name: Option<ChallengeName>,
    #[serde(default)]
    pub private_assets: Vec<ChallengePrivateAssetResponse>,
    #[serde(default)]
    pub validation_records: Vec<CreatorChallengeDraftValidationRecordResponse>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ChallengeDraftValidationRecordResponse>
    for CreatorChallengeDraftValidationRecordResponse
{
    /// Drop admin-only checkout path data for creator surfaces.
    fn from(record: ChallengeDraftValidationRecordResponse) -> Self {
        Self {
            id: record.id,
            draft_id: record.draft_id,
            status: record.status,
            message: record.message,
            manifest_sha256: record.manifest_sha256,
            bundle_sha256: record.bundle_sha256,
            created_at: record.created_at,
        }
    }
}

impl From<ChallengeDraftResponse> for CreatorChallengeDraftResponse {
    /// Drop admin-only checkout path data for creator surfaces.
    fn from(draft: ChallengeDraftResponse) -> Self {
        Self {
            id: draft.id,
            challenge_name: draft.challenge_name,
            request: draft.request,
            status: draft.status,
            creator_agent_id: draft.creator_agent_id,
            creator_github_user_id: draft.creator_github_user_id,
            creator_github_login: draft.creator_github_login,
            repo_url: draft.repo_url,
            pr_number: draft.pr_number,
            pr_url: draft.pr_url,
            commit_sha: draft.commit_sha,
            challenge_path: draft.challenge_path,
            manifest_sha256: draft.manifest_sha256,
            manifest: draft.manifest,
            validation_bundle_sha256: draft.validation_bundle_sha256,
            approved_bundle_sha256: draft.approved_bundle_sha256,
            validation_message: draft.validation_message,
            published_challenge_name: draft.published_challenge_name,
            private_assets: draft.private_assets,
            validation_records: draft
                .validation_records
                .into_iter()
                .map(CreatorChallengeDraftValidationRecordResponse::from)
                .collect(),
            created_at: draft.created_at,
            updated_at: draft.updated_at,
        }
    }
}

/// List response for admin challenge draft review.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDraftListResponse {
    pub items: Vec<ChallengeDraftResponse>,
}

/// Admin response returned after abandoning stale drafts and deleting
/// purge-eligible unpublished private asset records.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDraftCleanupResponse {
    pub abandoned_drafts: i64,
    pub purged_private_assets: i64,
    pub purged_temporary_storage_objects: i64,
}
