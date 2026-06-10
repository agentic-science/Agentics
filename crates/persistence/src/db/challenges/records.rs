use chrono::{DateTime, Utc};
use serde_json::Value;

use agentics_domain::models::challenge::{ChallengeBundleSpec, ChallengeLifecycleStatus};
use agentics_domain::models::evaluation::{MetricValue, SolutionSubmissionStatus};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{
    AgentId, ChallengeShortlistRevisionId, HumanId, SolutionSubmissionId,
};
use agentics_domain::models::localization::LocalizedText;
use agentics_domain::models::names::{ChallengeKeyword, ChallengeName, MetricName, TargetName};
use agentics_domain::models::urls::MoltbookPostUrl;
use agentics_domain::storage::StorageKey;

/// Published challenge list plus the unbounded count for pagination previews.
#[derive(Debug, Clone)]
pub struct PublishedChallengeList {
    pub items: Vec<PublishedChallengeListItemRecord>,
    pub total_count: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

/// Published challenge catalog record before public API projection.
#[derive(Debug, Clone)]
pub struct PublishedChallengeListItemRecord {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    pub spec_json: Value,
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
}

/// Search and keyword filters applied before public challenge pagination.
#[derive(Debug, Clone, Default)]
pub struct ChallengeCatalogFilters {
    pub search: Option<String>,
    pub keywords: Vec<ChallengeKeyword>,
}

/// Published challenge joined with challenge metadata.
#[derive(Debug, Clone)]
pub struct ChallengeRecord {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    pub bundle_key: StorageKey,
    pub public_bundle_key: StorageKey,
    pub statement_key: StorageKey,
    pub spec_json: Value,
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
}

/// Moltbook discussion anchor attached to one published challenge.
#[derive(Debug, Clone)]
pub struct ChallengeMoltbookDiscussionRecord {
    pub challenge_name: ChallengeName,
    pub discussion_url: Option<MoltbookPostUrl>,
}

/// Admin challenge catalog row before DTO projection.
#[derive(Debug, Clone)]
pub struct AdminChallengeListItemRecord {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    pub status: ChallengeLifecycleStatus,
    pub spec_json: Option<Value>,
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Challenge publish inputs.
#[derive(Debug)]
pub struct PublishChallengeInput<'a> {
    pub challenge_name: &'a ChallengeName,
    pub bundle_key: &'a StorageKey,
    pub public_bundle_key: &'a StorageKey,
    pub statement_key: &'a StorageKey,
    pub spec: &'a ChallengeBundleSpec,
    pub title: &'a str,
    pub summary: &'a LocalizedText,
}

/// Published challenge storage row returned by publish primitives.
#[derive(Debug, Clone)]
pub struct PublishChallengeRecord {
    pub challenge_name: ChallengeName,
    pub title: String,
    pub bundle_key: StorageKey,
    pub public_bundle_key: StorageKey,
    pub statement_key: StorageKey,
}

/// Input for one shortlist delta revision.
#[derive(Debug, Clone)]
pub struct CreateChallengeShortlistRevisionInput {
    pub revision_id: ChallengeShortlistRevisionId,
    pub challenge_name: ChallengeName,
    pub uploader_human_id: HumanId,
    pub storage_key: StorageKey,
    pub sha256: Sha256Digest,
    pub requested_count: i64,
    pub agent_ids_to_add: Vec<AgentId>,
}

/// Persisted shortlist revision row before DTO projection.
#[derive(Debug, Clone)]
pub struct ChallengeShortlistRevisionRecord {
    pub id: ChallengeShortlistRevisionId,
    pub challenge_name: ChallengeName,
    pub uploader_human_id: HumanId,
    pub requested_count: i64,
    pub added_count: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    pub created_at: DateTime<Utc>,
}

/// Effective shortlisted agent row before DTO projection.
#[derive(Debug, Clone)]
pub struct ChallengeShortlistedAgentRecord {
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub added_by_human_id: HumanId,
    pub created_at: DateTime<Utc>,
}

/// Effective challenge shortlist before DTO projection.
#[derive(Debug, Clone)]
pub struct ChallengeShortlistRecord {
    pub challenge_name: ChallengeName,
    pub items: Vec<ChallengeShortlistedAgentRecord>,
}

/// Challenge-owner aggregate statistics before DTO projection.
#[derive(Debug, Clone)]
pub struct CreatorChallengeStatsRecord {
    pub challenge_name: ChallengeName,
    pub target: Option<TargetName>,
    pub agent_count: i64,
    pub solution_submission_count: i64,
    pub completed_solution_submission_count: i64,
    pub failed_solution_submission_count: i64,
    pub queued_or_running_solution_submission_count: i64,
    pub visible_solution_submission_count: i64,
    pub validation_run_count: i64,
    pub official_run_count: i64,
    pub latest_solution_submission_at: Option<DateTime<Utc>>,
    pub latest_completed_evaluation_at: Option<DateTime<Utc>>,
    pub primary_metric_name: MetricName,
    pub primary_metric_min: Option<f64>,
    pub primary_metric_max: Option<f64>,
    pub primary_metric_mean: Option<f64>,
}

/// Challenge-owner participant row before DTO projection.
#[derive(Debug, Clone)]
pub struct CreatorChallengeParticipantRecord {
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub solution_submission_count: i64,
    pub best_solution_submission_id: Option<SolutionSubmissionId>,
    pub best_primary_metric: Option<MetricValue>,
    pub best_aggregate_metrics: Option<Vec<MetricValue>>,
    pub best_updated_at: Option<DateTime<Utc>>,
    pub latest_status: Option<SolutionSubmissionStatus>,
    pub latest_solution_submission_at: Option<DateTime<Utc>>,
}

/// Challenge-owner participant list before DTO projection.
#[derive(Debug, Clone)]
pub struct CreatorChallengeParticipantsRecord {
    pub challenge_name: ChallengeName,
    pub target: Option<TargetName>,
    pub items: Vec<CreatorChallengeParticipantRecord>,
}
