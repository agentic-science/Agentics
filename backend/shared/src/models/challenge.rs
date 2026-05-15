//! Challenge bundle and challenge-facing DTOs.

use serde::{Deserialize, Serialize};

use super::names::{ChallengeName, MetricName, ResourceProfileName, RunName, TargetName};
use super::paths::{BundleRelativePath, RunInputPath, RunOutputPath};
use super::urls::{ExternalDataUrl, MoltbookSubmoltUrl};
use crate::zip_project::ZipProjectNetworkAccess;

/// Parsed `spec.json` contract for a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    /// Plain-text summary used in compact challenge catalog surfaces.
    pub challenge_summary: String,
    pub solution: SolutionSpec,
    pub scorer: ScorerSpec,
    pub targets: Vec<ChallengeTargetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
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
    /// Optional external community metadata for this challenge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community: Option<CommunitySpec>,
    /// Metric definitions and ranking metadata used to interpret scorer output.
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

/// Scorer entrypoint and output-file contract for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScorerSpec {
    pub command: Vec<String>,
    pub result_file: BundleRelativePath,
}

/// Supported Docker platforms for targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DockerPlatform {
    #[serde(rename = "linux/arm64")]
    LinuxArm64,
    #[serde(rename = "linux/amd64")]
    LinuxAmd64,
}

impl DockerPlatform {
    /// Canonical Docker platform string used in Docker API requests.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinuxArm64 => "linux/arm64",
            Self::LinuxAmd64 => "linux/amd64",
        }
    }
}

/// Accelerator family used by a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetAccelerator {
    Cpu,
    Gpu,
}

impl TargetAccelerator {
    /// Stable string form used in user-facing summaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

/// One execution and ranking target declared by a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeTargetSpec {
    pub name: TargetName,
    pub docker_platform: DockerPlatform,
    pub accelerator: TargetAccelerator,
    pub validation_enabled: bool,
    pub resource_profile: ResourceProfileSpec,
}

/// Resource envelope and Docker images declared by a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResourceProfileSpec {
    pub name: ResourceProfileName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_description: Option<String>,
    pub solution_image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution_image_digest: Option<String>,
    pub scorer_image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scorer_image_digest: Option<String>,
    pub timeout_sec: u64,
    pub memory_limit_mb: u64,
    pub cpu_limit_millis: u32,
    pub disk_limit_mb: u64,
    pub setup_network_access: ZipProjectNetworkAccess,
    pub build_network_access: ZipProjectNetworkAccess,
    pub run_network_access: ZipProjectNetworkAccess,
    pub scorer_network_access: ZipProjectNetworkAccess,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware: Option<HardwareProfileSpec>,
}

/// Optional hardware metadata advertised with a resource profile.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HardwareProfileSpec {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_memory_gb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_minimum: Option<String>,
}

/// Challenge-owned run manifest locations for standardized `zip_project` execution.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeExecutionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_runs: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_prepare: Option<ChallengePrepareSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_runs: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_prepare: Option<ChallengePrepareSpec>,
}

/// Optional scorer-image command that prepares generated benchmark inputs.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengePrepareSpec {
    pub command: Vec<String>,
    /// Relative path, under the prepared workspace, to the generated run manifest.
    pub result_runs_file: BundleRelativePath,
    pub network_access: ZipProjectNetworkAccess,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproducibility_notes: Option<String>,
    /// Informational list of external resources the prepare phase may use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_data: Vec<ChallengePrepareExternalDataSpec>,
    /// Future cache metadata. The v0.2.5 MVP does not cache prepare output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_key_hint: Option<String>,
}

/// Informational external data metadata for challenge-owned prepare commands.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengePrepareExternalDataSpec {
    pub url: ExternalDataUrl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Challenge-owned list of scorer-controlled solution invocations.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeRunManifest {
    #[serde(default)]
    pub runs: Vec<ChallengeRunSpec>,
}

/// One solution invocation prepared by the worker and later scored by the scorer.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeRunSpec {
    pub run_name: RunName,
    pub interface: ChallengeRunInterface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_files: Vec<ChallengeRunInputFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_files: Vec<RunOutputPath>,
}

/// Supported worker-managed solution input/output interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeRunInterface {
    Stdio,
    FileSystem,
}

/// One input file materialized into `AGENTICS_INPUT_DIR` for a file-mode run.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeRunInputFile {
    pub path: RunInputPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_json: Option<serde_json::Value>,
}

/// Dataset layout and visibility policy declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Directory containing private benchmark data or private prepare config used by official runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_benchmark_dir: Option<BundleRelativePath>,
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

/// External community link metadata owned by the challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommunitySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moltbook_submolt_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moltbook_submolt_url: Option<MoltbookSubmoltUrl>,
}

/// Whether a metric is better when it is larger or smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    Maximize,
    Minimize,
}

/// Visibility level for a metric emitted by the scorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricVisibility {
    /// Visible in validation feedback and official result views.
    Public,
    /// Visible only after a ranking-visible official evaluation.
    Official,
}

/// One metric that a scorer may emit in aggregate or per-run result payloads.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetricDefinitionSpec {
    pub name: MetricName,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub direction: MetricDirection,
    pub visibility: MetricVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_description: Option<String>,
}

/// Ranking configuration for a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RankingSpec {
    pub primary_metric_name: MetricName,
    #[serde(default)]
    #[schemars(required)]
    pub tie_breaker_metric_names: Vec<MetricName>,
}

/// Metric schema embedded in `spec.json`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetricSchemaSpec {
    pub metrics: Vec<MetricDefinitionSpec>,
    pub ranking: RankingSpec,
}

impl MetricSchemaSpec {
    /// Look up a metric definition by name.
    pub fn metric(&self, metric_name: &MetricName) -> Option<&MetricDefinitionSpec> {
        self.metrics
            .iter()
            .find(|metric| &metric.name == metric_name)
    }

    /// Primary ranking metric declared by this challenge.
    pub fn primary_metric(&self) -> Option<&MetricDefinitionSpec> {
        self.metric(&self.ranking.primary_metric_name)
    }
}

impl Default for MetricSchemaSpec {
    fn default() -> Self {
        Self {
            metrics: vec![MetricDefinitionSpec {
                name: MetricName::score(),
                label: "Score".to_string(),
                unit: None,
                direction: MetricDirection::Maximize,
                visibility: MetricVisibility::Public,
                metric_description: Some("Normalized compatibility score in [0, 1].".to_string()),
            }],
            ranking: RankingSpec {
                primary_metric_name: MetricName::score(),
                tie_breaker_metric_names: vec![],
            },
        }
    }
}

/// One row in the public challenge catalog.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeListItemDto {
    pub name: ChallengeName,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
}

/// Public challenge catalog response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeListResponse {
    pub items: Vec<ChallengeListItemDto>,
}

/// Public challenge detail response with spec and Markdown statement.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDetailResponse {
    pub name: ChallengeName,
    pub title: String,
    pub summary: String,
    pub spec: ChallengeBundleSpec,
    pub statement_markdown: String,
}

/// Admin-facing challenge metadata response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeAdminResponse {
    pub name: ChallengeName,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// One row in the admin challenge list.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengeListItemDto {
    pub name: ChallengeName,
    pub title: String,
    pub summary: String,
    pub status: String,
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
    pub bundle_path: String,
    pub statement_path: String,
}
