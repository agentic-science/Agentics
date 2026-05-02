//! Challenge bundle and challenge-facing DTOs.

use serde::{Deserialize, Serialize};

use super::CurrentVersionDto;

/// Parsed `spec.json` contract for a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_id: String,
    pub challenge_title: String,
    pub challenge_version: String,
    pub submission: SubmissionSpec,
    pub scorer: ScorerSpec,
    pub limits: LimitsSpec,
    pub datasets: DatasetsSpec,
    /// Metric definitions and ranking metadata used to interpret scorer output.
    #[serde(default)]
    pub metric_schema: MetricSchemaSpec,
}

/// Submission format constraints declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionSpec {
    pub format: String,
    pub language: String,
    pub entrypoint: String,
}

/// Scorer entrypoint and output-file contract for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerSpec {
    pub entrypoint: String,
    pub result_file: String,
}

/// Runtime limits declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsSpec {
    pub time_limit_sec: f64,
    pub memory_limit_mb: i64,
}

/// Dataset layout and visibility policy declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetsSpec {
    pub shown_dir: String,
    pub hidden_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heldout_dir: Option<String>,
    pub shown_policy: super::evaluation::ScoreVisibility,
    pub hidden_policy: String,
    /// Whether agents may request private validation runs for this version.
    #[serde(default)]
    pub validation_enabled: bool,
    pub heldout_enabled: bool,
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
    pub description: String,
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
    pub description: String,
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
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
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
