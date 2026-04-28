use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMode {
    Public,
    Official,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreVisibility {
    Full,
    ScoreOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorerCaseStatus {
    Passed,
    Failed,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorerRunStatus {
    Passed,
    Failed,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSummary {
    pub score: f64,
    pub passed: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShownCaseResult {
    pub case_id: String,
    pub status: ScorerCaseStatus,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationDto {
    pub id: String,
    pub status: EvaluationStatus,
    pub eval_type: ScoringMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_score: Option<f64>,
    pub shown_results: Vec<ShownCaseResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_summary: Option<ScoreSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_summary: Option<ScoreSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerRunResult {
    pub status: ScorerRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ScoringMode>,
    pub primary_score: f64,
    #[serde(default)]
    pub shown_results: Vec<ShownCaseResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_summary: Option<ScoreSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_summary: Option<ScoreSummary>,
    #[serde(default)]
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationJobDto {
    pub id: String,
    pub status: EvaluationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationJobPayload {
    pub artifact_path: String,
    pub bundle_path: String,
    pub problem_id: String,
    pub problem_version_id: String,
}
