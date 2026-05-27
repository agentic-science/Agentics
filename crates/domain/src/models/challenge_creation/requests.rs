//! Challenge draft request DTOs.

use serde::{Deserialize, Serialize};

use crate::models::github::GithubPullRequestNumber;
use crate::models::hashes::{GitCommitSha, Sha256Digest};
use crate::models::names::AssetName;
use crate::models::paths::RepoRelativePath;
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};

use super::manifest::ChallengeCreationManifest;
use super::private_assets::ChallengePrivateAssetKind;

/// Creator-authenticated request for binding a public GitHub PR to a draft.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeDraftRequest {
    pub repo_url: GithubRepoRemote,
    pub pr_number: GithubPullRequestNumber,
    pub pr_url: GithubPullRequestUrl,
    pub commit_sha: GitCommitSha,
    pub challenge_path: RepoRelativePath,
    #[garde(range(min = 1))]
    pub pr_author_github_user_id: i64,
    pub manifest: ChallengeCreationManifest,
}

/// Payload for uploading a private benchmark asset to Agentics storage.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct UploadChallengePrivateAssetRequest {
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub asset_base64: String,
}

/// Admin payload for validating a draft against a checked-out repository path.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct ValidateChallengeDraftRequest {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub repository_path: String,
}

/// Admin payload for accepting or rejecting a challenge draft.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct ReviewChallengeDraftRequest {
    #[serde(default)]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_validation_bundle_sha256: Option<Sha256Digest>,
}
