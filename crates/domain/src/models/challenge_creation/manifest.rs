//! Public challenge creation manifest models.

use serde::{Deserialize, Serialize};

use crate::models::localization::LocalizedText;
use crate::models::names::{ChallengeKeyword, ChallengeName};
use crate::models::paths::RepoRelativePath;

use super::private_assets::ChallengePrivateAssetRequirement;

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
