//! Problem bundle and problem-facing DTOs.

use serde::{Deserialize, Serialize};

use super::CurrentVersionDto;

/// Parsed `spec.json` contract for a problem bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemBundleSpec {
    pub schema_version: i32,
    pub problem_id: String,
    pub problem_title: String,
    pub problem_version: String,
    pub submission: SubmissionSpec,
    pub scorer: ScorerSpec,
    pub limits: LimitsSpec,
    pub datasets: DatasetsSpec,
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
    pub heldout_enabled: bool,
}

/// One row in the public problem catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemListItemDto {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub current_version: CurrentVersionDto,
}

/// Public problem catalog response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemListResponse {
    pub items: Vec<ProblemListItemDto>,
}

/// Public problem detail response with spec and Markdown statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetailResponse {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub current_version: CurrentVersionDto,
    pub spec: ProblemBundleSpec,
    pub statement_markdown: String,
}

/// Admin-facing problem metadata response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemAdminResponse {
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
pub struct CreateProblemVersionResponse {
    pub problem_id: String,
    pub slug: String,
    pub title: String,
    pub version_id: String,
    pub version: String,
    pub bundle_path: String,
    pub statement_path: String,
}
