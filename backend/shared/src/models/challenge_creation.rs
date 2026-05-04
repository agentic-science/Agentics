//! Public GitHub challenge-creation and draft lifecycle models.

use serde::{Deserialize, Serialize};

/// Public challenge manifest file expected at the root of a challenge proposal.
pub const AGENTICS_CHALLENGE_MANIFEST_FILE: &str = "agentics.challenge.json";

/// Public manifest submitted through the reviewed challenge repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeCreationManifest {
    pub schema_version: i32,
    pub request: ChallengeCreationRequestKind,
    pub challenge_id: String,
    pub title: String,
    pub summary: String,
    pub readme_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<ChallengeCreationVersionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ChallengeArchiveRequestSpec>,
    #[serde(default)]
    pub private_assets: Vec<ChallengePrivateAssetRequirement>,
    #[serde(default)]
    pub ci: ChallengeCreationCiSpec,
}

/// Lifecycle request represented by a public manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeCreationRequestKind {
    NewChallenge,
    NewVersion,
    ArchiveChallenge,
}

/// Version metadata for new-challenge and new-version requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeCreationVersionSpec {
    pub version: String,
    pub bundle_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_version: Option<String>,
}

/// Public archive request metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeArchiveRequestSpec {
    pub reason: String,
}

/// Private asset that must be uploaded directly to Agentics for a draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengePrivateAssetRequirement {
    pub asset_id: String,
    pub kind: ChallengePrivateAssetKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_note: Option<String>,
}

/// Supported private asset classes for challenge creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengePrivateAssetKind {
    PrivateBenchmarkData,
    PrivateScorerPackage,
    PrivateSeeds,
    PrivateReferenceOutputs,
}

/// CI expectations for the public challenge repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    fn default() -> Self {
        Self {
            validate_manifest: true,
            validate_public_bundle: true,
            smoke_test_public_validation: false,
        }
    }
}

fn default_required() -> bool {
    true
}

/// GitHub identity link stored on an Agentics agent account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubIdentityResponse {
    pub agent_id: String,
    pub github_user_id: i64,
    pub github_login: String,
}

/// Payload for linking an authenticated agent to a manually verified GitHub identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkGithubIdentityRequest {
    pub github_user_id: i64,
    pub github_login: String,
}

/// Agent-authenticated request for binding a public GitHub PR to a draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeDraftRequest {
    pub repo_url: String,
    pub pr_number: i32,
    pub pr_url: String,
    pub commit_sha: String,
    pub challenge_path: String,
    pub pr_author_github_user_id: i64,
    pub manifest: ChallengeCreationManifest,
}

/// Draft status used by the review lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDraftStatus {
    Draft,
    Validated,
    Approved,
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
            Self::Rejected => "rejected",
            Self::Published => "published",
            Self::Abandoned => "abandoned",
        }
    }
}

/// Validation record status for a challenge draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDraftValidationStatus {
    Passed,
    Failed,
}

impl ChallengeDraftValidationStatus {
    /// Stable database string for this validation outcome.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

/// API response for one private benchmark asset bound to a draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengePrivateAssetResponse {
    pub id: String,
    pub draft_id: String,
    pub asset_id: String,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub size_bytes: i64,
    pub sha256: String,
    pub storage_uri: String,
    pub uploader_agent_id: String,
    pub created_at: String,
}

/// API response for one validation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeDraftValidationRecordResponse {
    pub id: String,
    pub draft_id: String,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub repository_path: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

/// API response for one challenge draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeDraftResponse {
    pub id: String,
    pub challenge_id: String,
    pub request: ChallengeCreationRequestKind,
    pub status: ChallengeDraftStatus,
    pub creator_agent_id: String,
    pub creator_github_user_id: i64,
    pub creator_github_login: String,
    pub repo_url: String,
    pub pr_number: i32,
    pub pr_url: String,
    pub commit_sha: String,
    pub challenge_path: String,
    pub manifest_sha256: String,
    pub manifest: ChallengeCreationManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_repository_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_challenge_version_id: Option<String>,
    #[serde(default)]
    pub private_assets: Vec<ChallengePrivateAssetResponse>,
    #[serde(default)]
    pub validation_records: Vec<ChallengeDraftValidationRecordResponse>,
    pub created_at: String,
    pub updated_at: String,
}

/// List response for admin challenge draft review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeDraftListResponse {
    pub items: Vec<ChallengeDraftResponse>,
}

/// Payload for uploading a private benchmark asset to Agentics storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadChallengePrivateAssetRequest {
    pub asset_id: String,
    pub kind: ChallengePrivateAssetKind,
    #[serde(default)]
    pub required: bool,
    pub asset_base64: String,
}

/// Admin payload for validating a draft against a checked-out repository path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidateChallengeDraftRequest {
    pub repository_path: String,
}

/// Admin payload for accepting or rejecting a challenge draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewChallengeDraftRequest {
    #[serde(default)]
    pub message: String,
}
