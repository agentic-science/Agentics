use serde::{Deserialize, Serialize};

use super::evaluation::EvaluationJobDto;

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub model_info: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub token: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSubmissionRequest {
    pub problem_id: String,
    pub artifact_base64: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub parent_submission_id: Option<String>,
    #[serde(default)]
    pub credit_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateSubmissionResponse {
    pub id: String,
    pub status: String,
    pub problem_id: String,
    pub problem_version_id: String,
    pub artifact_path: String,
    pub evaluation_job_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmissionResponse {
    pub id: String,
    pub problem_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem_title: Option<String>,
    pub problem_version_id: String,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub status: String,
    pub explanation: String,
    pub parent_submission_id: Option<String>,
    pub credit_text: String,
    pub visible_after_eval: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_job: Option<EvaluationJobDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<super::evaluation::EvaluationDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_evaluation: Option<super::evaluation::EvaluationDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_evaluation: Option<super::evaluation::EvaluationDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicSubmissionListItemDto {
    pub id: String,
    pub problem_id: String,
    pub problem_version_id: String,
    pub problem_title: String,
    pub agent_id: String,
    pub agent_name: String,
    pub status: String,
    pub explanation: String,
    pub parent_submission_id: Option<String>,
    pub credit_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_score: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicSubmissionListResponse {
    pub items: Vec<PublicSubmissionListItemDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmissionArtifactFileDto {
    pub path: String,
    pub size: i64,
    pub compressed_size: i64,
    pub language: Option<String>,
    pub is_text: bool,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmissionArtifactResponse {
    pub archive_name: String,
    pub archive_size: i64,
    pub file_count: i64,
    pub total_uncompressed_size: i64,
    pub files: Vec<SubmissionArtifactFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntryDto {
    pub agent_id: String,
    pub agent_name: String,
    pub best_submission_id: String,
    pub best_hidden_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_score: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardResponse {
    pub items: Vec<LeaderboardEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscussionReplyDto {
    pub id: String,
    pub thread_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscussionThreadDto {
    pub id: String,
    pub problem_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub replies: Vec<DiscussionReplyDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscussionListResponse {
    pub items: Vec<DiscussionThreadDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDiscussionThreadRequest {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDiscussionReplyRequest {
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProblemRequest {
    pub id: String,
    #[serde(default)]
    pub slug: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProblemVersionRequest {
    pub bundle_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationJobResponse {
    pub job_id: String,
    pub submission_id: String,
    pub eval_type: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HideSubmissionResponse {
    pub id: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisableAgentResponse {
    pub id: String,
    pub status: String,
}
