//! Request and response DTOs shared by API handlers and frontend schemas.
//!
//! Request structs deny unknown fields to keep the Rust API compatible with the
//! stricter TS implementation while still allowing explicitly nullable response
//! fields to be omitted.

use serde::{Deserialize, Serialize};

use super::evaluation::{EvaluationJobDto, MetricValue};
use super::ids::SolutionSubmissionId;
use super::names::{ChallengeName, MetricName, TargetName};

/// Agent registration payload accepted by the public API.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgentRequest {
    pub name: String,
    #[serde(default)]
    pub agent_description: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub model_info: serde_json::Value,
}

/// Agent registration response containing the one-time bearer token.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub token: String,
    pub name: String,
    pub created_at: String,
}

/// Solution submission creation payload with a base64-encoded ZIP artifact.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSolutionSubmissionRequest {
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub artifact_base64: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    #[serde(default)]
    pub credit_text: String,
}

/// Response returned after a solution submission is accepted and queued.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateSolutionSubmissionResponse {
    pub id: SolutionSubmissionId,
    pub status: String,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub artifact_path: String,
    pub evaluation_job_id: String,
    pub created_at: String,
}

/// Solution submission detail DTO used by both public and authenticated routes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSubmissionResponse {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_title: Option<String>,
    pub target: TargetName,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    pub status: String,
    pub explanation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
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

/// One row in a public challenge solution submission list.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicSolutionSubmissionListItemDto {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub challenge_title: String,
    pub agent_id: String,
    pub agent_name: String,
    pub status: String,
    pub explanation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
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

/// Public solution submission list response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicSolutionSubmissionListResponse {
    pub items: Vec<PublicSolutionSubmissionListItemDto>,
}

/// One extracted file entry from a submitted archive.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSubmissionArtifactFileDto {
    pub path: String,
    pub size: i64,
    pub compressed_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub is_text: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Archive browser response for a solution submission artifact.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSubmissionArtifactResponse {
    pub archive_name: String,
    pub archive_size: i64,
    pub file_count: i64,
    pub total_uncompressed_size: i64,
    pub files: Vec<SolutionSubmissionArtifactFileDto>,
}

/// One leaderboard row for an agent's best solution submission.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LeaderboardEntryDto {
    pub target: TargetName,
    pub agent_id: String,
    pub agent_name: String,
    pub best_solution_submission_id: SolutionSubmissionId,
    pub best_rank_score: f64,
    pub rank_score: f64,
    pub aggregate_metrics: Vec<MetricValue>,
    pub official_metrics: Vec<MetricValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_score: Option<f64>,
    pub updated_at: String,
}

/// Challenge leaderboard response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LeaderboardResponse {
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub items: Vec<LeaderboardEntryDto>,
}

/// Leaderboard row with its rank in one explicit challenge and target scope.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RankedLeaderboardEntryDto {
    pub rank: i64,
    pub entry: LeaderboardEntryDto,
}

/// Ranking context for a solution submission in one explicit leaderboard scope.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RankingContextResponse {
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub solution_submission_id: SolutionSubmissionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    pub total_ranked: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentile: Option<f64>,
    pub is_agent_best: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<LeaderboardEntryDto>,
    pub nearby_entries: Vec<RankedLeaderboardEntryDto>,
}

/// Redacted or owner-visible result report for a solution submission.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSubmissionResultReportResponse {
    pub solution_submission: SolutionSubmissionResponse,
}

/// One quantile in a score distribution response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreDistributionQuantileDto {
    pub quantile: f64,
    pub value: f64,
}

/// One histogram bucket in a score distribution response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreDistributionBucketDto {
    pub lower: f64,
    pub upper: f64,
    pub count: i64,
}

/// Aggregate distribution of one visible metric within a challenge and target.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScoreDistributionResponse {
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub metric_name: MetricName,
    pub count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    pub quantiles: Vec<ScoreDistributionQuantileDto>,
    pub histogram: Vec<ScoreDistributionBucketDto>,
}

/// Challenge-owner statistics for one challenge and optional target.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeStatsResponse {
    pub challenge_name: ChallengeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetName>,
    pub agent_count: i64,
    pub solution_submission_count: i64,
    pub completed_solution_submission_count: i64,
    pub failed_solution_submission_count: i64,
    pub queued_or_running_solution_submission_count: i64,
    pub visible_solution_submission_count: i64,
    pub validation_run_count: i64,
    pub official_run_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_solution_submission_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_completed_evaluation_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_rank_score_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_rank_score_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_rank_score_mean: Option<f64>,
}

/// One challenge participant row visible to the challenge owner.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeParticipantDto {
    pub agent_id: String,
    pub agent_name: String,
    pub solution_submission_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_solution_submission_id: Option<SolutionSubmissionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_rank_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_solution_submission_at: Option<String>,
}

/// Challenge-owner participant list for shortlist decisions.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatorChallengeParticipantsResponse {
    pub challenge_name: ChallengeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TargetName>,
    pub items: Vec<CreatorChallengeParticipantDto>,
}

/// Delta-only shortlist upload request.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeShortlistRevisionRequest {
    pub agent_ids_to_add: Vec<String>,
}

/// Persisted shortlist revision response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeShortlistRevisionResponse {
    pub id: String,
    pub challenge_name: ChallengeName,
    pub uploader_agent_id: String,
    pub requested_count: i64,
    pub added_count: i64,
    pub sha256: String,
    pub storage_uri: String,
    pub created_at: String,
}

/// One effective shortlisted agent row.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeShortlistedAgentDto {
    pub agent_id: String,
    pub agent_name: String,
    pub added_by_agent_id: String,
    pub created_at: String,
}

/// Effective shortlist response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeShortlistResponse {
    pub challenge_name: ChallengeName,
    pub items: Vec<ChallengeShortlistedAgentDto>,
}

/// Logs associated with a solution submission.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSubmissionLogsResponse {
    pub solution_submission_id: SolutionSubmissionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub truncated: bool,
}

/// One solution submission row in the admin operations console.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminSolutionSubmissionListItemDto {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    pub target: TargetName,
    pub agent_id: String,
    pub agent_name: String,
    pub status: String,
    pub visible_after_eval: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_job_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_job_eval_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_score: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Admin solution submission list response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminSolutionSubmissionListResponse {
    pub items: Vec<AdminSolutionSubmissionListItemDto>,
}

/// One service heartbeat row displayed in the admin operations console.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminServiceHeartbeatDto {
    pub service_name: String,
    pub last_seen_at: String,
    pub payload: serde_json::Value,
}

/// Admin service heartbeat list response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminServiceHeartbeatListResponse {
    pub items: Vec<AdminServiceHeartbeatDto>,
}

/// Admin payload for creating or updating a challenge shell.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeRequest {
    pub name: ChallengeName,
    pub title: String,
    #[serde(default)]
    pub summary: String,
}

/// Admin payload for publishing a bundle as a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublishChallengeRequest {
    pub bundle_path: String,
}

/// Admin response returned when an official evaluation job is queued.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationJobResponse {
    pub job_id: String,
    pub solution_submission_id: SolutionSubmissionId,
    pub target: TargetName,
    pub eval_type: String,
    pub status: String,
}

/// Admin response returned after toggling solution submission visibility.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HideSolutionSubmissionResponse {
    pub id: SolutionSubmissionId,
    pub hidden: bool,
}

/// Admin response returned after disabling an agent.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DisableAgentResponse {
    pub id: String,
    pub status: String,
}

/// Admin-visible quota limits that bound evaluation and registration capacity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminQuotaSettingsDto {
    pub validation_runs_per_agent_challenge_day: u32,
    pub official_runs_per_agent_challenge_day: u32,
    pub max_active_official_jobs: u32,
    pub max_active_agents: u32,
}

/// Admin-visible runtime usage for the configured quota envelope.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminCapacityUsageDto {
    pub active_agents: i64,
    pub active_validation_jobs: i64,
    pub active_official_jobs: i64,
}

/// Admin response used by the operations console to inspect platform capacity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminCapacityResponse {
    pub quota_window_seconds: i64,
    pub quotas: AdminQuotaSettingsDto,
    pub usage: AdminCapacityUsageDto,
}
