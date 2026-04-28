use serde::{Deserialize, Serialize};

use super::CurrentVersionDto;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionSpec {
    pub format: String,
    pub language: String,
    pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerSpec {
    pub entrypoint: String,
    pub result_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsSpec {
    pub time_limit_sec: f64,
    pub memory_limit_mb: i64,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemListItemDto {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub current_version: CurrentVersionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemListResponse {
    pub items: Vec<ProblemListItemDto>,
}

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
