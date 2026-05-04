//! Challenge bundle and challenge-facing DTOs.

use serde::{Deserialize, Serialize};

use super::CurrentVersionDto;
use crate::zip_project::ZipProjectNetworkAccess;

/// Parsed `spec.json` contract for a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_id: String,
    pub challenge_title: String,
    /// Plain-text summary used in compact challenge catalog surfaces.
    pub challenge_summary: String,
    pub challenge_version: String,
    pub solution: SolutionSpec,
    pub scorer: ScorerSpec,
    pub resource_profile: ResourceProfileSpec,
    pub execution: ChallengeExecutionSpec,
    pub datasets: DatasetsSpec,
    /// Optional external community metadata for this challenge version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community: Option<CommunitySpec>,
    /// Metric definitions and ranking metadata used to interpret scorer output.
    #[serde(default)]
    pub metric_schema: MetricSchemaSpec,
}

/// Local solution format constraints declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionSpec {
    pub protocol: String,
    pub manifest_file: String,
}

/// Scorer entrypoint and output-file contract for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerSpec {
    pub command: Vec<String>,
    pub result_file: String,
}

/// Resource envelope and Docker images declared by a challenge version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProfileSpec {
    pub id: String,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfileSpec {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Challenge-owned run manifest locations for standardized `zip_project` execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeExecutionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_runs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_runs: Option<String>,
}

/// Challenge-owned list of scorer-controlled solution invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRunManifest {
    #[serde(default)]
    pub runs: Vec<ChallengeRunSpec>,
}

/// One solution invocation prepared by the worker and later scored by the scorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRunSpec {
    pub run_id: String,
    pub interface: ChallengeRunInterface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_files: Vec<ChallengeRunInputFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_files: Vec<String>,
}

/// Supported worker-managed solution input/output interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeRunInterface {
    Stdio,
    FileSystem,
}

/// One input file materialized into `AGENTICS_INPUT_DIR` for a file-mode run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRunInputFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_json: Option<serde_json::Value>,
}

/// Dataset layout and visibility policy declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: String,
    /// Directory containing private benchmark data used only by official runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_benchmark_dir: Option<String>,
    /// Visibility policy for public validation case results.
    pub public_policy: super::evaluation::ScoreVisibility,
    /// Visibility policy for private benchmark results.
    pub private_benchmark_policy: String,
    /// Whether agents may request private validation runs for this version.
    #[serde(default)]
    pub validation_enabled: bool,
    /// Whether official runs can evaluate against private benchmark data.
    pub private_benchmark_enabled: bool,
}

/// External community link metadata owned by the challenge version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moltbook_submolt_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moltbook_submolt_url: Option<String>,
}

/// Whether a metric is better when it is larger or smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    Maximize,
    Minimize,
}

/// Visibility level for a metric emitted by the scorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricVisibility {
    /// Visible in validation feedback and official result views.
    Public,
    /// Visible only after a ranking-visible official evaluation.
    Official,
}

/// One metric that a scorer may emit in aggregate or per-run result payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinitionSpec {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub direction: MetricDirection,
    pub visibility: MetricVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Ranking configuration for a challenge version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingSpec {
    pub primary_metric_id: String,
    #[serde(default)]
    pub tie_breaker_metric_ids: Vec<String>,
}

/// Metric schema embedded in `spec.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSchemaSpec {
    pub metrics: Vec<MetricDefinitionSpec>,
    pub ranking: RankingSpec,
}

impl MetricSchemaSpec {
    /// Look up a metric definition by id.
    pub fn metric(&self, metric_id: &str) -> Option<&MetricDefinitionSpec> {
        self.metrics.iter().find(|metric| metric.id == metric_id)
    }

    /// Primary ranking metric declared by this challenge version.
    pub fn primary_metric(&self) -> Option<&MetricDefinitionSpec> {
        self.metric(&self.ranking.primary_metric_id)
    }
}

impl Default for MetricSchemaSpec {
    fn default() -> Self {
        Self {
            metrics: vec![MetricDefinitionSpec {
                id: "score".to_string(),
                label: "Score".to_string(),
                unit: None,
                direction: MetricDirection::Maximize,
                visibility: MetricVisibility::Public,
                description: Some("Normalized compatibility score in [0, 1].".to_string()),
            }],
            ranking: RankingSpec {
                primary_metric_id: "score".to_string(),
                tie_breaker_metric_ids: vec![],
            },
        }
    }
}

/// One row in the public challenge catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeListItemDto {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub current_version: CurrentVersionDto,
}

/// Public challenge catalog response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeListResponse {
    pub items: Vec<ChallengeListItemDto>,
}

/// Public challenge detail response with spec and Markdown statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeDetailResponse {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub current_version: CurrentVersionDto,
    pub spec: ChallengeBundleSpec,
    pub statement_markdown: String,
}

/// Admin-facing challenge metadata response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeAdminResponse {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// One row in the admin challenge list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminChallengeListItemDto {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<CurrentVersionDto>,
    pub created_at: String,
    pub updated_at: String,
}

/// Admin challenge list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminChallengeListResponse {
    pub items: Vec<AdminChallengeListItemDto>,
}

/// Admin response returned after publishing a bundle version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChallengeVersionResponse {
    pub challenge_id: String,
    pub slug: String,
    pub title: String,
    pub version_id: String,
    pub version: String,
    pub bundle_path: String,
    pub statement_path: String,
}
