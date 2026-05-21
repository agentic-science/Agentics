//! Public GitHub challenge-creation and draft lifecycle models.

use serde::{Deserialize, Serialize};

use super::github::GithubPullRequestNumber;
use super::hashes::{GitCommitSha, Sha256Digest};
use super::ids::{
    AgentId, ChallengeDraftId, ChallengeDraftValidationRecordId, ChallengePrivateAssetId,
};
use super::localization::LocalizedText;
use super::names::{AssetName, ChallengeKeyword, ChallengeName};
use super::paths::RepoRelativePath;
use super::urls::{GithubPullRequestUrl, GithubRepoRemote};
use crate::storage::StorageKey;

/// Public challenge manifest file expected at the root of a challenge proposal.
pub const AGENTICS_CHALLENGE_MANIFEST_FILE: &str = "agentics.challenge.json";

/// Public manifest submitted through the reviewed challenge repository.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeCreationManifest {
    pub schema_version: i32,
    pub request: ChallengeCreationRequestKind,
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub readme_path: RepoRelativePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_path: Option<RepoRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ChallengeArchiveRequestSpec>,
    #[serde(default)]
    pub private_assets: Vec<ChallengePrivateAssetRequirement>,
    #[serde(default)]
    pub ci: ChallengeCreationCiSpec,
}

/// Lifecycle request represented by a public manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeCreationRequestKind {
    NewChallenge,
    ArchiveChallenge,
}

impl ChallengeCreationRequestKind {
    /// Stable database string for this creation request.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewChallenge => "new_challenge",
            Self::ArchiveChallenge => "archive_challenge",
        }
    }

    /// Parse a stable database string for this creation request.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "new_challenge" => Some(Self::NewChallenge),
            "archive_challenge" => Some(Self::ArchiveChallenge),
            _ => None,
        }
    }
}

/// Public archive request metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeArchiveRequestSpec {
    pub reason: String,
}

/// Private asset that must be uploaded directly to Agentics for a draft.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengePrivateAssetRequirement {
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_note: Option<String>,
}

/// Supported private asset classes for challenge creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengePrivateAssetKind {
    PrivateBenchmarkData,
    PrivateEvaluatorPackage,
    PrivateSeeds,
    PrivateReferenceOutputs,
}

impl ChallengePrivateAssetKind {
    /// Stable database string for this private asset kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateBenchmarkData => "private_benchmark_data",
            Self::PrivateEvaluatorPackage => "private_evaluator_package",
            Self::PrivateSeeds => "private_seeds",
            Self::PrivateReferenceOutputs => "private_reference_outputs",
        }
    }

    /// Parse a stable database string for this private asset kind.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "private_benchmark_data" => Some(Self::PrivateBenchmarkData),
            "private_evaluator_package" => Some(Self::PrivateEvaluatorPackage),
            "private_seeds" => Some(Self::PrivateSeeds),
            "private_reference_outputs" => Some(Self::PrivateReferenceOutputs),
            _ => None,
        }
    }
}

/// CI expectations for the public challenge repository.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeCreationCiSpec {
    #[serde(default = "default_required")]
    pub validate_manifest: bool,
    #[serde(default = "default_required")]
    pub validate_public_bundle: bool,
    #[serde(default)]
    pub smoke_test_public_validation: bool,
}

impl Default for ChallengeCreationCiSpec {
    /// Handles default for this module.
    fn default() -> Self {
        Self {
            validate_manifest: true,
            validate_public_bundle: true,
            smoke_test_public_validation: false,
        }
    }
}

/// Handles default required for this module.
fn default_required() -> bool {
    true
}

/// Creator-authenticated request for binding a public GitHub PR to a draft.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeDraftRequest {
    pub repo_url: GithubRepoRemote,
    pub pr_number: GithubPullRequestNumber,
    pub pr_url: GithubPullRequestUrl,
    pub commit_sha: GitCommitSha,
    pub challenge_path: RepoRelativePath,
    pub pr_author_github_user_id: i64,
    pub manifest: ChallengeCreationManifest,
}

/// Draft status used by the review lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDraftStatus {
    Draft,
    Validated,
    Approved,
    Publishing,
    Rejected,
    Published,
    Abandoned,
}

impl ChallengeDraftStatus {
    /// Stable database string for this draft status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Validated => "validated",
            Self::Approved => "approved",
            Self::Publishing => "publishing",
            Self::Rejected => "rejected",
            Self::Published => "published",
            Self::Abandoned => "abandoned",
        }
    }

    /// Parse a stable database string for this draft status.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "validated" => Some(Self::Validated),
            "approved" => Some(Self::Approved),
            "publishing" => Some(Self::Publishing),
            "rejected" => Some(Self::Rejected),
            "published" => Some(Self::Published),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

/// Validation record status for a challenge draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDraftValidationStatus {
    Running,
    Passed,
    Failed,
}

impl ChallengeDraftValidationStatus {
    /// Stable database string for this validation outcome.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    /// Parse a stable database string for this validation outcome.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// API response for one private benchmark asset bound to a draft.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengePrivateAssetResponse {
    pub id: ChallengePrivateAssetId,
    pub draft_id: ChallengeDraftId,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub size_bytes: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    pub uploader_agent_id: AgentId,
    pub created_at: String,
}

/// Internal lifecycle status for one private asset upload record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengePrivateAssetStatus {
    Pending,
    Active,
    Failed,
}

impl ChallengePrivateAssetStatus {
    /// Stable database string for this private asset lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Failed => "failed",
        }
    }

    /// Parse a stable database string for this private asset lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Admin-only response for one private asset upload lifecycle record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengePrivateAssetResponse {
    pub id: ChallengePrivateAssetId,
    pub draft_id: ChallengeDraftId,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub status: ChallengePrivateAssetStatus,
    pub size_bytes: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary_storage_key: Option<StorageKey>,
    pub uploader_agent_id: AgentId,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
}

/// Admin-only list response for private asset upload lifecycle records.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengePrivateAssetListResponse {
    pub items: Vec<AdminChallengePrivateAssetResponse>,
}

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

/// Payload for uploading a private benchmark asset to Agentics storage.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadChallengePrivateAssetRequest {
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub asset_base64: String,
}

/// Admin payload for validating a draft against a checked-out repository path.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateChallengeDraftRequest {
    pub repository_path: String,
}

/// Admin payload for accepting or rejecting a challenge draft.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewChallengeDraftRequest {
    #[serde(default)]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_validation_bundle_sha256: Option<Sha256Digest>,
}

/// Admin response returned after abandoning stale drafts and deleting
/// purge-eligible unpublished private asset records.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDraftCleanupResponse {
    pub abandoned_drafts: i64,
    pub purged_private_assets: i64,
}
