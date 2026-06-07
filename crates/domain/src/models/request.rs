//! Request and response DTOs shared by API handlers and frontend schemas.
//!
//! Request structs deny unknown fields to keep the Rust API compatible with the
//! stricter TS implementation while still allowing explicitly nullable response
//! fields to be omitted.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::challenge::MoltbookCommunityDto;
use super::evaluation::{
    EvaluationJobDto, EvaluationJobStatus, EvaluationStatus, MetricValue, ScoringMode,
    SolutionSubmissionStatus,
};
use super::hashes::Sha256Digest;
use super::ids::{
    AgentId, ChallengeShortlistRevisionId, EvaluationJobId, HumanId, PioneerCodeId,
    SolutionSubmissionId,
};
use super::names::{ChallengeName, MetricName, TargetName};
use super::pioneer_codes::{
    PioneerCodeInput, PioneerCodeStatus, PioneerCodeSubjectKind, PioneerCodeUseKind,
};
use super::urls::MoltbookPostUrl;
use crate::storage::StorageKey;

/// Persistent lifecycle state for an agent account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Disabled,
}

impl AgentStatus {
    /// Stable database string for an agent account state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// Parse the stable database string for an agent account state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

impl fmt::Display for AgentStatus {
    /// Format the agent status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Agent registration payload accepted by the public API.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgentRequest {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pioneer_code: Option<PioneerCodeInput>,
    #[serde(default)]
    pub agent_description: String,
    #[serde(default)]
    pub model_info: serde_json::Value,
}

/// Agent registration response containing the one-time bearer token.
#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentResponse {
    pub agent_id: AgentId,
    pub token: String,
    pub display_name: String,
    pub created_at: String,
}

impl fmt::Debug for RegisterAgentResponse {
    /// Redacts the one-time raw agent bearer token from debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterAgentResponse")
            .field("agent_id", &self.agent_id)
            .field("token", &"<redacted>")
            .field("display_name", &self.display_name)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Admin payload for creating a pioneer code.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CreatePioneerCodeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub max_uses: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Admin-visible pioneer-code metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PioneerCodeDto {
    pub id: PioneerCodeId,
    pub code_display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub note: String,
    pub max_uses: i64,
    pub use_count: i64,
    pub status: PioneerCodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_by_display: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

/// Admin list response for pioneer codes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PioneerCodeListResponse {
    pub items: Vec<PioneerCodeDto>,
}

/// Account created through a pioneer code.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PioneerCodeUseDto {
    pub subject_kind: PioneerCodeSubjectKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_id: Option<HumanId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_github_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_display_name: Option<String>,
    pub registration_kind: PioneerCodeUseKind,
    pub used_at: String,
}

/// Admin detail response for one pioneer code.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PioneerCodeDetailResponse {
    pub code: PioneerCodeDto,
    pub uses: Vec<PioneerCodeUseDto>,
}

/// Response returned after revoking a pioneer code.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RevokePioneerCodeResponse {
    pub id: PioneerCodeId,
    pub status: PioneerCodeStatus,
    pub revoked_human_count: i64,
    pub revoked_human_session_count: i64,
    pub revoked_admin_service_token_count: i64,
    pub revoked_creator_api_token_count: i64,
    pub revoked_agent_count: i64,
    pub revoked_token_count: i64,
}

/// Solution submission creation payload with a base64-encoded ZIP artifact.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CreateSolutionSubmissionRequest {
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    #[garde(custom(crate::validation::trimmed_non_empty))]
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
    pub status: SolutionSubmissionStatus,
    pub challenge_name: ChallengeName,
    pub target: TargetName,
    pub artifact_key: StorageKey,
    pub note: String,
    pub evaluation_job_id: EvaluationJobId,
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
    pub agent_id: AgentId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_display_name: Option<String>,
    pub status: SolutionSubmissionStatus,
    pub note: String,
    pub explanation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_primary_metric: Option<MetricValue>,
    pub visible_after_eval: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_key: Option<StorageKey>,
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
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub status: SolutionSubmissionStatus,
    pub note: String,
    pub explanation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_solution_submission_id: Option<SolutionSubmissionId>,
    pub credit_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_primary_metric: Option<MetricValue>,
    pub created_at: String,
    pub updated_at: String,
}

/// Public solution submission list response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicSolutionSubmissionListResponse {
    pub total_count: i64,
    pub items: Vec<PublicSolutionSubmissionListItemDto>,
}

/// Aggregate public observer counters.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicStatsResponse {
    pub challenge_count: u64,
    pub agent_count: u64,
    pub public_completed_submission_count: u64,
    pub total_solution_attempt_count: u64,
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
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub best_solution_submission_id: SolutionSubmissionId,
    pub best_rank_score: f64,
    pub rank_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_primary_metric: Option<MetricValue>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
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
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub solution_submission_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_solution_submission_id: Option<SolutionSubmissionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_rank_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_status: Option<SolutionSubmissionStatus>,
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
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CreateChallengeShortlistRevisionRequest {
    #[garde(length(min = 1))]
    pub agent_ids_to_add: Vec<AgentId>,
}

/// Persisted shortlist revision response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeShortlistRevisionResponse {
    pub id: ChallengeShortlistRevisionId,
    pub challenge_name: ChallengeName,
    pub uploader_human_id: HumanId,
    pub requested_count: i64,
    pub added_count: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    pub created_at: String,
}

/// One effective shortlisted agent row.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeShortlistedAgentDto {
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub added_by_human_id: HumanId,
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
    pub availability: SolutionSubmissionLogAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_log_storage_key: Option<StorageKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub truncated: bool,
}

/// Explains whether a submitter-visible runner log can be returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SolutionSubmissionLogAvailability {
    /// A runner log was persisted and may be returned to this submitter.
    Available,
    /// No runner log was persisted for the visible evaluation.
    NotPersisted,
    /// The official run may have touched private benchmark material.
    RedactedPrivateOfficial,
    /// Operator configuration redacts all official-run logs.
    RedactedByConfig,
}

impl SolutionSubmissionLogAvailability {
    /// Stable JSON and CLI label for this log availability state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::NotPersisted => "not_persisted",
            Self::RedactedPrivateOfficial => "redacted_private_official",
            Self::RedactedByConfig => "redacted_by_config",
        }
    }
}

impl fmt::Display for SolutionSubmissionLogAvailability {
    /// Render the stable snake-case availability label.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One solution submission row in the admin operations console.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminSolutionSubmissionListItemDto {
    pub id: SolutionSubmissionId,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    pub target: TargetName,
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub status: SolutionSubmissionStatus,
    pub note: String,
    pub visible_after_eval: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_job_id: Option<EvaluationJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_job_status: Option<EvaluationJobStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_job_eval_type: Option<ScoringMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<EvaluationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_status: Option<EvaluationStatus>,
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

/// Admin payload for attaching a Moltbook discussion post to a published challenge.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct SetChallengeMoltbookDiscussionRequest {
    pub discussion_url: MoltbookPostUrl,
}

/// Admin response after setting or clearing a challenge Moltbook discussion anchor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeMoltbookDiscussionResponse {
    pub challenge_name: ChallengeName,
    pub moltbook: MoltbookCommunityDto,
}

/// Admin response returned when an official evaluation job is queued.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluationJobResponse {
    pub job_id: EvaluationJobId,
    pub solution_submission_id: SolutionSubmissionId,
    pub target: TargetName,
    pub eval_type: ScoringMode,
    pub status: EvaluationJobStatus,
}

/// Admin response returned after disabling an agent.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DisableAgentResponse {
    pub id: AgentId,
    pub status: AgentStatus,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_registration_debug_redacts_bearer_token() {
        let response = RegisterAgentResponse {
            agent_id: AgentId::try_new("11111111-1111-4111-8111-111111111111")
                .expect("valid agent id"),
            token: "agent-secret-token".to_string(),
            display_name: "debug-agent".to_string(),
            created_at: "2026-06-07T00:00:00Z".to_string(),
        };

        let debug = format!("{response:?}");

        assert!(debug.contains("RegisterAgentResponse"));
        assert!(debug.contains("debug-agent"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("agent-secret-token"));
    }
}
