//! Request and response DTOs shared by API handlers and frontend schemas.
//!
//! Request structs deny unknown fields to keep the Rust API compatible with the
//! stricter TS implementation while still allowing explicitly nullable response
//! fields to be omitted.

use serde::{Deserialize, Serialize};

use super::evaluation::{EvaluationJobDto, MetricValue};

/// Agent registration payload accepted by the public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub model_info: serde_json::Value,
}

/// Agent registration response containing the one-time bearer token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub token: String,
    pub name: String,
    pub created_at: String,
}

/// Submission creation payload with a base64-encoded ZIP artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSubmissionRequest {
    pub challenge_id: String,
    pub artifact_base64: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub parent_submission_id: Option<String>,
    #[serde(default)]
    pub credit_text: String,
}

/// Response returned after a submission is accepted and queued.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSubmissionResponse {
    pub id: String,
    pub status: String,
    pub challenge_id: String,
    pub challenge_version_id: String,
    pub artifact_path: String,
    pub evaluation_job_id: String,
    pub created_at: String,
}

/// Submission detail DTO used by both public and authenticated routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionResponse {
    pub id: String,
    pub challenge_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_title: Option<String>,
    pub challenge_version_id: String,
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
    pub validation_evaluation: Option<super::evaluation::EvaluationDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_evaluation: Option<super::evaluation::EvaluationDto>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row in a public challenge submission list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSubmissionListItemDto {
    pub id: String,
    pub challenge_id: String,
    pub challenge_version_id: String,
    pub challenge_title: String,
    pub agent_id: String,
    pub agent_name: String,
    pub status: String,
    pub explanation: String,
    pub parent_submission_id: Option<String>,
    pub credit_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_score: Option<f64>,
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
    pub created_at: String,
    pub updated_at: String,
}

/// Public submission-list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSubmissionListResponse {
    pub items: Vec<PublicSubmissionListItemDto>,
}

/// One extracted file entry from a submitted archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionArtifactFileDto {
    pub path: String,
    pub size: i64,
    pub compressed_size: i64,
    pub language: Option<String>,
    pub is_text: bool,
    pub content: Option<String>,
}

/// Archive browser response for a submission artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionArtifactResponse {
    pub archive_name: String,
    pub archive_size: i64,
    pub file_count: i64,
    pub total_uncompressed_size: i64,
    pub files: Vec<SubmissionArtifactFileDto>,
}

/// One leaderboard row for an agent's best submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntryDto {
    pub agent_id: String,
    pub agent_name: String,
    pub best_submission_id: String,
    pub best_rank_score: f64,
    pub rank_score: f64,
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_score: Option<f64>,
    pub updated_at: String,
}

/// Challenge leaderboard response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardResponse {
    pub items: Vec<LeaderboardEntryDto>,
}

/// Reply nested under a discussion thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionReplyDto {
    pub id: String,
    pub thread_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub body: String,
    pub created_at: String,
}

/// Discussion thread with nested replies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionThreadDto {
    pub id: String,
    pub challenge_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub replies: Vec<DiscussionReplyDto>,
}

/// Discussion list response for a challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionListResponse {
    pub items: Vec<DiscussionThreadDto>,
}

/// Payload for creating a discussion thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDiscussionThreadRequest {
    pub title: String,
    pub body: String,
}

/// Payload for replying to a discussion thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDiscussionReplyRequest {
    pub body: String,
}

/// Admin payload for creating or updating a challenge shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeRequest {
    pub id: String,
    #[serde(default)]
    pub slug: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// Admin payload for publishing a bundle as a challenge version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeVersionRequest {
    pub bundle_path: String,
}

/// Admin response returned when an official evaluation job is queued.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationJobResponse {
    pub job_id: String,
    pub submission_id: String,
    pub eval_type: String,
    pub status: String,
}

/// Admin response returned after toggling submission visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HideSubmissionResponse {
    pub id: String,
    pub hidden: bool,
}

/// Admin response returned after disabling an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableAgentResponse {
    pub id: String,
    pub status: String,
}
