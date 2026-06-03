//! Challenge review record response DTOs.

use serde::{Deserialize, Serialize};

use crate::models::auth::GithubUserId;
use crate::models::github::GithubPullRequestNumber;
use crate::models::hashes::{GitCommitSha, Sha256Digest};
use crate::models::ids::{ChallengeReviewRecordId, ChallengeReviewValidationRecordId, HumanId};
use crate::models::names::ChallengeName;
use crate::models::paths::RepoRelativePath;
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};

use super::lifecycle::{ChallengeReviewRecordStatus, ChallengeReviewValidationStatus};
use super::manifest::{ChallengeCreationManifest, ChallengeCreationRequestKind};
use super::private_assets::ChallengePrivateAssetResponse;

/// API response for one validation record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeReviewValidationRecordResponse {
    pub id: ChallengeReviewValidationRecordId,
    pub review_record_id: ChallengeReviewRecordId,
    pub status: ChallengeReviewValidationStatus,
    pub message: String,
    pub repository_path: String,
    pub manifest_sha256: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<Sha256Digest>,
    pub created_at: String,
}

/// Creator-facing validation record response without server-local checkout paths.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeReviewValidationRecordResponse {
    pub id: ChallengeReviewValidationRecordId,
    pub review_record_id: ChallengeReviewRecordId,
    pub status: ChallengeReviewValidationStatus,
    pub message: String,
    pub manifest_sha256: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<Sha256Digest>,
    pub created_at: String,
}

/// API response for one challenge review record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeReviewRecordResponse {
    pub id: ChallengeReviewRecordId,
    pub challenge_name: ChallengeName,
    pub request: ChallengeCreationRequestKind,
    pub status: ChallengeReviewRecordStatus,
    pub creator_human_id: HumanId,
    pub creator_github_user_id: GithubUserId,
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
    pub validation_records: Vec<ChallengeReviewValidationRecordResponse>,
    pub created_at: String,
    pub updated_at: String,
}

/// Creator-facing response for one challenge review record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeReviewRecordResponse {
    pub id: ChallengeReviewRecordId,
    pub challenge_name: ChallengeName,
    pub request: ChallengeCreationRequestKind,
    pub status: ChallengeReviewRecordStatus,
    pub creator_human_id: HumanId,
    pub creator_github_user_id: GithubUserId,
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
    pub validation_records: Vec<CreatorChallengeReviewValidationRecordResponse>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ChallengeReviewValidationRecordResponse>
    for CreatorChallengeReviewValidationRecordResponse
{
    /// Drop admin-only checkout path data for creator surfaces.
    fn from(record: ChallengeReviewValidationRecordResponse) -> Self {
        Self {
            id: record.id,
            review_record_id: record.review_record_id,
            status: record.status,
            message: record.message,
            manifest_sha256: record.manifest_sha256,
            bundle_sha256: record.bundle_sha256,
            created_at: record.created_at,
        }
    }
}

impl From<ChallengeReviewRecordResponse> for CreatorChallengeReviewRecordResponse {
    /// Drop admin-only checkout path data for creator surfaces.
    fn from(review_record: ChallengeReviewRecordResponse) -> Self {
        Self {
            id: review_record.id,
            challenge_name: review_record.challenge_name,
            request: review_record.request,
            status: review_record.status,
            creator_human_id: review_record.creator_human_id,
            creator_github_user_id: review_record.creator_github_user_id,
            creator_github_login: review_record.creator_github_login,
            repo_url: review_record.repo_url,
            pr_number: review_record.pr_number,
            pr_url: review_record.pr_url,
            commit_sha: review_record.commit_sha,
            challenge_path: review_record.challenge_path,
            manifest_sha256: review_record.manifest_sha256,
            manifest: review_record.manifest,
            validation_bundle_sha256: review_record.validation_bundle_sha256,
            approved_bundle_sha256: review_record.approved_bundle_sha256,
            validation_message: review_record.validation_message,
            published_challenge_name: review_record.published_challenge_name,
            private_assets: review_record.private_assets,
            validation_records: review_record
                .validation_records
                .into_iter()
                .map(CreatorChallengeReviewValidationRecordResponse::from)
                .collect(),
            created_at: review_record.created_at,
            updated_at: review_record.updated_at,
        }
    }
}

/// List response for admin challenge review record review.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeReviewRecordListResponse {
    pub items: Vec<ChallengeReviewRecordResponse>,
}

/// Admin response returned after abandoning stale review records and deleting
/// purge-eligible unpublished private asset records.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeReviewRecordCleanupResponse {
    pub abandoned_review_records: i64,
    pub purged_private_assets: i64,
    pub purged_temporary_storage_objects: i64,
}
