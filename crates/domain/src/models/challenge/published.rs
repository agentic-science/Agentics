//! Published challenge API DTOs.

use serde::{Deserialize, Serialize};

use crate::models::localization::LocalizedText;
use crate::models::names::{ChallengeKeyword, ChallengeName, MoltbookSubmoltName};
use crate::models::urls::{MoltbookPostUrl, MoltbookSubmoltUrl};
use crate::storage::StorageKey;

use super::bundle::{
    ChallengeEligibilitySpec, ChallengeSolutionPublicationPolicy, ChallengeVisibilitySpec,
    PublicChallengeBundleSpec,
};
use super::lifecycle::ChallengeLifecycleStatus;
use super::targets::ChallengeTargetSpec;

/// One row in the public challenge catalog.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeListItemDto {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub starts_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
}

/// Public challenge catalog response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeListResponse {
    pub items: Vec<ChallengeListItemDto>,
    pub total_count: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

/// Public Moltbook community metadata exposed on challenge detail surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MoltbookCommunityDto {
    pub submolt_name: MoltbookSubmoltName,
    pub submolt_url: MoltbookSubmoltUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_url: Option<MoltbookPostUrl>,
}

/// Public challenge detail response with spec and Markdown statement.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDetailResponse {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub spec: PublicChallengeBundleSpec,
    pub statement_markdown: String,
    pub moltbook: MoltbookCommunityDto,
}

/// Admin-facing challenge metadata response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeAdminResponse {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[serde(default)]
    pub keywords: Vec<ChallengeKeyword>,
    pub status: ChallengeLifecycleStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// One row in the admin challenge list.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengeListItemDto {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[serde(default)]
    pub keywords: Vec<ChallengeKeyword>,
    pub status: ChallengeLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<ChallengeTargetSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<ChallengeEligibilitySpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ChallengeVisibilitySpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution_publication: Option<ChallengeSolutionPublicationPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_benchmark_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
    pub created_at: String,
    pub updated_at: String,
}

/// Admin challenge list response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengeListResponse {
    pub items: Vec<AdminChallengeListItemDto>,
}

/// Admin response returned after publishing a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublishChallengeResponse {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub bundle_key: StorageKey,
    pub public_bundle_key: StorageKey,
    pub statement_key: StorageKey,
}
