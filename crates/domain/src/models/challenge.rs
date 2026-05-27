//! Challenge bundle and challenge-facing DTOs.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::localization::LocalizedText;
use super::names::{ChallengeKeyword, ChallengeName, MoltbookSubmoltName, TargetName};
use super::paths::BundleRelativePath;
use super::urls::{MoltbookPostUrl, MoltbookSubmoltUrl};
use crate::storage::StorageKey;

mod execution;
mod metrics;
mod targets;

pub use execution::{
    ChallengeExecutionMode, ChallengeExecutionSpec, ChallengeRunInputFile, ChallengeRunInterface,
    ChallengeRunManifest, ChallengeRunSpec, ChallengeSetupSpec, CoexecutedBenchmarkExecutionSpec,
    CoexecutedBenchmarkSetupSpec, EvaluatorSpec, PipedStdioExecutionSpec,
    PipedStdioSessionManifest, PipedStdioSetupSpec, PublicChallengeExecutionSpec,
    PublicCoexecutedBenchmarkExecutionSpec, PublicPipedStdioExecutionSpec,
    PublicSeparatedEvaluatorExecutionSpec, SeparatedEvaluatorExecutionSpec,
};
pub use metrics::{
    MetricDefinitionSpec, MetricDirection, MetricSchemaSpec, MetricVisibility, RankingSpec,
};
pub use targets::{
    ChallengeTargetSpec, DockerPlatform, EvaluatorStageProfiles, HardwareProfileSpec,
    ResourceProfileSpec, SolutionStageProfiles, StageResourceProfile, TargetAccelerator,
};

/// Persistent lifecycle state for a challenge shell or published benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeLifecycleStatus {
    Draft,
    Active,
    Archived,
}

impl ChallengeLifecycleStatus {
    /// Stable database string for a challenge lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    /// Parse a stable database string for a challenge lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

impl fmt::Display for ChallengeLifecycleStatus {
    /// Format the challenge status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Minimum public keywords that a challenge must declare.
pub const MIN_CHALLENGE_KEYWORDS: usize = 1;

/// Maximum public keywords that a challenge may declare.
pub const MAX_CHALLENGE_KEYWORDS: usize = 6;

/// Parsed `spec.json` contract for a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct ChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    /// Localized summary used in compact challenge catalog surfaces.
    pub summary: LocalizedText,
    /// Required public keywords used by catalog search and filtering.
    #[garde(length(min = MIN_CHALLENGE_KEYWORDS, max = MAX_CHALLENGE_KEYWORDS))]
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub solution: SolutionSpec,
    pub targets: Vec<ChallengeTargetSpec>,
    pub starts_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_submission_limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_submission_limit: Option<i64>,
    pub visibility: ChallengeVisibilitySpec,
    pub solution_publication: ChallengeSolutionPublicationPolicy,
    pub execution: ChallengeExecutionSpec,
    pub datasets: DatasetsSpec,
    /// Metric definitions and ranking metadata used to interpret evaluator output.
    #[serde(default)]
    #[schemars(required)]
    pub metric_schema: MetricSchemaSpec,
}

impl ChallengeBundleSpec {
    /// Look up one target declared by this challenge.
    pub fn target(&self, target: &TargetName) -> Option<&ChallengeTargetSpec> {
        self.targets
            .iter()
            .find(|candidate| &candidate.name == target)
    }

    /// Return the only target name when a challenge is unambiguous.
    pub fn sole_target(&self) -> Option<&TargetName> {
        match self.targets.as_slice() {
            [target] => Some(&target.name),
            _ => None,
        }
    }
}

/// Public projection of a challenge contract safe for unauthenticated clients.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    /// Localized summary used in compact challenge catalog surfaces.
    pub summary: LocalizedText,
    /// Required public keywords used by catalog search and filtering.
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub solution: SolutionSpec,
    pub targets: Vec<ChallengeTargetSpec>,
    pub starts_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_submission_limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_submission_limit: Option<i64>,
    pub visibility: ChallengeVisibilitySpec,
    pub solution_publication: ChallengeSolutionPublicationPolicy,
    pub execution: PublicChallengeExecutionSpec,
    pub datasets: PublicDatasetsSpec,
    /// Metric definitions and ranking metadata used to interpret evaluator output.
    #[serde(default)]
    #[schemars(required)]
    pub metric_schema: MetricSchemaSpec,
}

impl PublicChallengeBundleSpec {
    /// Look up one public target declared by this challenge.
    pub fn target(&self, target: &TargetName) -> Option<&ChallengeTargetSpec> {
        self.targets
            .iter()
            .find(|candidate| &candidate.name == target)
    }

    /// Return the only target name when a public challenge is unambiguous.
    pub fn sole_target(&self) -> Option<&TargetName> {
        match self.targets.as_slice() {
            [target] => Some(&target.name),
            _ => None,
        }
    }
}

impl From<ChallengeBundleSpec> for PublicChallengeBundleSpec {
    /// Remove private benchmark locator metadata from a full challenge contract.
    fn from(spec: ChallengeBundleSpec) -> Self {
        Self {
            schema_version: spec.schema_version,
            challenge_name: spec.challenge_name,
            challenge_title: spec.challenge_title,
            summary: spec.summary,
            keywords: spec.keywords,
            solution: spec.solution,
            targets: spec.targets,
            starts_at: spec.starts_at,
            closes_at: spec.closes_at,
            eligibility: spec.eligibility,
            validation_submission_limit: spec.validation_submission_limit,
            official_submission_limit: spec.official_submission_limit,
            visibility: spec.visibility,
            solution_publication: spec.solution_publication,
            execution: spec.execution.into(),
            datasets: PublicDatasetsSpec {
                public_dir: spec.datasets.public_dir,
                public_policy: spec.datasets.public_policy,
                private_benchmark_policy: spec.datasets.private_benchmark_policy,
                private_benchmark_enabled: spec.datasets.private_benchmark_enabled,
            },
            metric_schema: spec.metric_schema,
        }
    }
}

/// Eligibility policy for a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeEligibilitySpec {
    #[serde(rename = "type")]
    pub eligibility_type: ChallengeEligibilityType,
}

/// Stable eligibility policy names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeEligibilityType {
    Open,
    PrivateShortlist,
}

/// Visibility policy for challenge result surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeVisibilitySpec {
    pub leaderboard: ChallengeVisibility,
    pub score_distribution: ChallengeVisibility,
    pub result_detail: ChallengeResultDetailVisibility,
}

/// Visibility for public aggregate surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeVisibility {
    PublicLive,
    PublicAfterClose,
    Hidden,
}

/// Visibility for solution submission details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeResultDetailVisibility {
    SubmitterLivePublicLive,
    SubmitterLivePublicAfterClose,
    SubmitterOnly,
}

/// Policy controlling when solution artifacts may become public.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeSolutionPublicationPolicy {
    Private,
    Public,
    PublicAfterClose,
}

/// Local solution format constraints declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSpec {
    pub protocol: String,
    pub manifest_file: BundleRelativePath,
}

/// Dataset layout and visibility policy declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Directory containing private benchmark data or private setup config used by official runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_benchmark_dir: Option<BundleRelativePath>,
    /// Visibility policy for public validation case results.
    pub public_policy: super::evaluation::ScoreVisibility,
    /// Visibility policy for private benchmark results.
    pub private_benchmark_policy: PrivateBenchmarkPolicy,
    /// Whether official runs can evaluate against private benchmark data.
    pub private_benchmark_enabled: bool,
}

/// Public dataset metadata with private benchmark paths removed.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicDatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Visibility policy for public validation case results.
    pub public_policy: super::evaluation::ScoreVisibility,
    /// Visibility policy for private benchmark results.
    pub private_benchmark_policy: PrivateBenchmarkPolicy,
    /// Whether official runs can evaluate against private benchmark data.
    pub private_benchmark_enabled: bool,
}

/// Visibility policy allowed for private benchmark results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrivateBenchmarkPolicy {
    ScoreOnly,
}

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
