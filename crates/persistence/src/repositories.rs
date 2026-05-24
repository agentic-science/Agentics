//! Repository facades over SQLx persistence primitives.
//!
//! These types keep SQL ownership inside `agentics-persistence` while giving
//! services and transport crates explicit boundaries instead of importing a
//! broad bag of free functions.

use sqlx::PgPool;

use crate::db;
use agentics_config::WorkerAccelerators;
use agentics_domain::error::Result;
use agentics_domain::models::challenge::AdminChallengeListItemDto;
use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetResponse, ChallengeDraftResponse,
    ChallengeDraftValidationRecordResponse, ChallengePrivateAssetResponse,
};
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{
    AgentId, AgentPioneerCodeId, ChallengeDraftId, ChallengeDraftPublishClaimId, ChallengeId,
    EvaluationJobId, SolutionSubmissionId,
};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreatorChallengeParticipantsResponse, CreatorChallengeStatsResponse,
};
use agentics_domain::models::urls::MoltbookPostUrl;
use agentics_domain::storage::StorageKey;
use secrecy::SecretString;

pub use db::agents::{AgentRecord, AuthenticatedAgent, RegisterAgentInput};
pub use db::challenge_creation::{
    BeginChallengeDraftValidationInput, ChallengePrivateAssetPurgeRecord,
    ClaimedChallengeDraftForPublish, CreateChallengeDraftAuditEventInput,
    CreateChallengeDraftInput, CreateChallengePrivateAssetInput,
    FinishChallengeDraftValidationInput, PublishArchiveChallengeDraftInput,
    PublishNewChallengeDraftInput,
};
pub use db::challenges::{
    ChallengeCatalogFilters, ChallengeMoltbookDiscussionRecord, ChallengeRecord,
    CreateChallengeShortlistRevisionInput, PublishChallengeInput, PublishedChallengeList,
};
pub use db::evaluation_jobs::{EvaluationJobRecord, QueueEvaluationJobInput};
pub use db::evaluation_policy::PublishedChallengeAdmission;
pub use db::evaluations::{MarkEvaluationStartedInput, PersistedEvaluationResult};
pub use db::leaderboard::LeaderboardMetricEntry;
pub use db::maintenance::{HeartbeatPayload, StaleJobReapResult};
pub use db::pioneer_codes::{
    CreatePioneerCodeInput, PioneerCodeRecord, PioneerCodeRegistrationKind, PioneerCodeUseRecord,
    RevokePioneerCodeOutcome,
};
pub use db::sessions::{
    AuthenticatedAdminSession, AuthenticatedCreatorSession, ConsumedGithubOauthState,
    CreateAdminSessionInput, CreateCreatorSessionInput, CreateGithubOauthStateInput,
};

/// Root persistence facade for one database pool.
#[derive(Debug, Clone)]
pub struct Repositories {
    pool: PgPool,
}

impl Repositories {
    /// Build repository facades over a shared pool.
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub fn agents(&self) -> AgentsRepository<'_> {
        AgentsRepository { pool: &self.pool }
    }

    pub fn challenges(&self) -> ChallengesRepository<'_> {
        ChallengesRepository { pool: &self.pool }
    }

    pub fn challenge_drafts(&self) -> ChallengeDraftsRepository<'_> {
        ChallengeDraftsRepository { pool: &self.pool }
    }

    pub fn solution_submissions(&self) -> SolutionSubmissionsRepository<'_> {
        SolutionSubmissionsRepository { pool: &self.pool }
    }

    pub fn evaluation_jobs(&self) -> EvaluationJobsRepository<'_> {
        EvaluationJobsRepository { pool: &self.pool }
    }

    pub fn leaderboard(&self) -> LeaderboardRepository<'_> {
        LeaderboardRepository { pool: &self.pool }
    }

    pub fn pioneer_codes(&self) -> PioneerCodesRepository<'_> {
        PioneerCodesRepository { pool: &self.pool }
    }

    pub fn sessions(&self) -> SessionsRepository<'_> {
        SessionsRepository { pool: &self.pool }
    }

    pub fn maintenance(&self) -> MaintenanceRepository<'_> {
        MaintenanceRepository { pool: &self.pool }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AgentsRepository<'a> {
    pool: &'a PgPool,
}

impl AgentsRepository<'_> {
    pub async fn register_agent(
        &self,
        input: &RegisterAgentInput,
        max_active_agents: i64,
    ) -> Result<AgentRecord> {
        db::agents::register_agent(self.pool, input, max_active_agents).await
    }

    pub async fn register_agent_with_pioneer_code(
        &self,
        input: &RegisterAgentInput,
        code_hash: &str,
        max_active_agents: i64,
        kind: PioneerCodeRegistrationKind,
    ) -> Result<AgentRecord> {
        db::agents::register_agent_with_pioneer_code(
            self.pool,
            input,
            code_hash,
            kind,
            max_active_agents,
        )
        .await
    }

    pub async fn count_active(&self) -> Result<i64> {
        db::agents::count_active_agents(self.pool).await
    }

    pub async fn authenticate_token(
        &self,
        token: &SecretString,
    ) -> Result<Option<AuthenticatedAgent>> {
        db::agents::authenticate_agent_token(self.pool, token).await
    }

    pub async fn disable(&self, agent_id: &str) -> Result<()> {
        db::agents::disable_agent(self.pool, agent_id).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChallengesRepository<'a> {
    pool: &'a PgPool,
}

impl ChallengesRepository<'_> {
    pub async fn list_admin(&self) -> Result<Vec<AdminChallengeListItemDto>> {
        db::challenges::list_admin_challenges(self.pool).await
    }

    pub async fn set_moltbook_discussion(
        &self,
        challenge_id: &ChallengeId,
        discussion_url: &MoltbookPostUrl,
    ) -> Result<ChallengeMoltbookDiscussionRecord> {
        db::challenges::set_challenge_moltbook_discussion(self.pool, challenge_id, discussion_url)
            .await
    }

    pub async fn clear_moltbook_discussion(
        &self,
        challenge_id: &ChallengeId,
    ) -> Result<ChallengeMoltbookDiscussionRecord> {
        db::challenges::clear_challenge_moltbook_discussion(self.pool, challenge_id).await
    }

    pub async fn publish(
        &self,
        input: &PublishChallengeInput<'_>,
    ) -> Result<agentics_domain::models::challenge::PublishChallengeResponse> {
        db::challenges::publish_challenge(self.pool, input).await
    }

    pub async fn archive(&self, challenge_id: &ChallengeId) -> Result<()> {
        db::challenges::archive_challenge(self.pool, challenge_id).await
    }

    pub async fn add_owner(&self, challenge_id: &ChallengeId, agent_id: &AgentId) -> Result<()> {
        db::challenges::add_challenge_owner(self.pool, challenge_id, agent_id).await
    }

    pub async fn agent_owns(&self, challenge_id: &ChallengeId, agent_id: &AgentId) -> Result<bool> {
        db::challenges::agent_owns_challenge(self.pool, challenge_id, agent_id).await
    }

    pub async fn has_shortlist(&self, challenge_id: &ChallengeId) -> Result<bool> {
        db::challenges::challenge_has_shortlist(self.pool, challenge_id).await
    }

    pub async fn agent_is_shortlisted(
        &self,
        challenge_id: &ChallengeId,
        agent_id: &AgentId,
    ) -> Result<bool> {
        db::challenges::agent_is_shortlisted(self.pool, challenge_id, agent_id).await
    }

    pub async fn create_shortlist_revision(
        &self,
        input: &CreateChallengeShortlistRevisionInput,
    ) -> Result<ChallengeShortlistRevisionResponse> {
        db::challenges::create_challenge_shortlist_revision(self.pool, input).await
    }

    pub async fn list_shortlist(
        &self,
        challenge_id: &ChallengeId,
    ) -> Result<ChallengeShortlistResponse> {
        db::challenges::list_challenge_shortlist(self.pool, challenge_id).await
    }

    pub async fn creator_stats(
        &self,
        challenge_id: &ChallengeId,
        target: Option<&TargetName>,
    ) -> Result<CreatorChallengeStatsResponse> {
        db::challenges::get_creator_challenge_stats(self.pool, challenge_id, target).await
    }

    pub async fn creator_participants(
        &self,
        challenge_id: &ChallengeId,
        target: Option<&TargetName>,
    ) -> Result<CreatorChallengeParticipantsResponse> {
        db::challenges::list_creator_challenge_participants(self.pool, challenge_id, target).await
    }

    pub async fn list_published(
        &self,
        limit: i64,
        offset: i64,
        filters: &ChallengeCatalogFilters,
    ) -> Result<PublishedChallengeList> {
        db::challenges::list_published_challenges(self.pool, limit, offset, filters).await
    }

    pub async fn get_published(
        &self,
        challenge_id: &ChallengeId,
    ) -> Result<Option<ChallengeRecord>> {
        db::challenges::get_published_challenge(self.pool, challenge_id).await
    }

    pub async fn get_published_by_name(
        &self,
        challenge_name: &ChallengeName,
    ) -> Result<Option<ChallengeRecord>> {
        db::challenges::get_published_challenge_by_name(self.pool, challenge_name).await
    }

    pub async fn get_public(&self, challenge_id: &ChallengeId) -> Result<Option<ChallengeRecord>> {
        db::challenges::get_public_challenge(self.pool, challenge_id).await
    }

    pub async fn ensure_supports_eval_type(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
        eval_type: ScoringMode,
        agent_id: &AgentId,
    ) -> Result<PublishedChallengeAdmission> {
        db::evaluation_policy::ensure_published_challenge_supports_eval_type(
            self.pool,
            challenge_id,
            target,
            eval_type,
            agent_id,
        )
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChallengeDraftsRepository<'a> {
    pool: &'a PgPool,
}

impl ChallengeDraftsRepository<'_> {
    pub async fn create(
        &self,
        input: &CreateChallengeDraftInput,
        audit_event: &CreateChallengeDraftAuditEventInput,
    ) -> Result<ChallengeDraftResponse> {
        db::challenge_creation::create_challenge_draft(self.pool, input, audit_event).await
    }

    pub async fn get(&self, draft_id: &str) -> Result<Option<ChallengeDraftResponse>> {
        db::challenge_creation::get_challenge_draft(self.pool, draft_id).await
    }

    pub async fn list(&self, limit: i64) -> Result<Vec<ChallengeDraftResponse>> {
        db::challenge_creation::list_challenge_drafts(self.pool, limit).await
    }

    pub async fn list_private_asset_states(
        &self,
        draft_id: &str,
    ) -> Result<Vec<AdminChallengePrivateAssetResponse>> {
        db::challenge_creation::list_challenge_private_asset_states(self.pool, draft_id).await
    }

    pub async fn reserve_private_asset(
        &self,
        input: &CreateChallengePrivateAssetInput,
        max_bytes_per_draft: u64,
        validation_timeout_minutes: i32,
        pending_timeout_minutes: i32,
    ) -> Result<ChallengePrivateAssetResponse> {
        db::challenge_creation::reserve_challenge_private_asset(
            self.pool,
            input,
            max_bytes_per_draft,
            validation_timeout_minutes,
            pending_timeout_minutes,
        )
        .await
    }

    pub async fn activate_private_asset_with_audit(
        &self,
        asset_row_id: &agentics_domain::models::ids::ChallengePrivateAssetId,
        audit_event_id: agentics_domain::models::ids::ChallengeDraftAuditEventId,
        uploader_agent_id: &AgentId,
    ) -> Result<ChallengePrivateAssetResponse> {
        db::challenge_creation::activate_challenge_private_asset_with_audit(
            self.pool,
            asset_row_id,
            audit_event_id,
            uploader_agent_id,
        )
        .await
    }

    pub async fn activate_private_asset(
        &self,
        asset_row_id: &agentics_domain::models::ids::ChallengePrivateAssetId,
    ) -> Result<ChallengePrivateAssetResponse> {
        db::challenge_creation::activate_challenge_private_asset(self.pool, asset_row_id).await
    }

    pub async fn fail_private_asset(
        &self,
        asset_row_id: &agentics_domain::models::ids::ChallengePrivateAssetId,
        message: &str,
    ) -> Result<()> {
        db::challenge_creation::fail_challenge_private_asset(self.pool, asset_row_id, message).await
    }

    pub async fn private_asset_storage_key_has_active_reference(
        &self,
        storage_key: &StorageKey,
    ) -> Result<bool> {
        db::challenge_creation::private_asset_storage_key_has_active_reference(
            self.pool,
            storage_key,
        )
        .await
    }

    pub async fn begin_validation(
        &self,
        input: &BeginChallengeDraftValidationInput,
        window_seconds: i64,
        validation_limit: i64,
        validation_timeout_minutes: i32,
    ) -> Result<ChallengeDraftValidationRecordResponse> {
        db::challenge_creation::begin_challenge_draft_validation(
            self.pool,
            input,
            window_seconds,
            validation_limit,
            validation_timeout_minutes,
        )
        .await
    }

    pub async fn finish_validation(
        &self,
        input: &FinishChallengeDraftValidationInput,
        audit_event: &CreateChallengeDraftAuditEventInput,
    ) -> Result<ChallengeDraftValidationRecordResponse> {
        db::challenge_creation::finish_challenge_draft_validation(self.pool, input, audit_event)
            .await
    }

    pub async fn abandon_with_audit(
        &self,
        draft_id: &ChallengeDraftId,
        message: Option<&str>,
        audit_event: &CreateChallengeDraftAuditEventInput,
    ) -> Result<()> {
        db::challenge_creation::abandon_challenge_draft_with_audit(
            self.pool,
            draft_id,
            message,
            audit_event,
        )
        .await
    }

    pub async fn abandon_stale(&self, ttl_days: i64) -> Result<i64> {
        db::challenge_creation::abandon_stale_challenge_drafts(self.pool, ttl_days).await
    }

    pub async fn list_unpublished_private_assets_for_purge(
        &self,
        grace_days: i64,
    ) -> Result<Vec<ChallengePrivateAssetPurgeRecord>> {
        db::challenge_creation::list_unpublished_private_assets_for_purge(self.pool, grace_days)
            .await
    }

    pub async fn delete_private_asset(&self, asset_row_id: &str) -> Result<()> {
        db::challenge_creation::delete_challenge_private_asset(self.pool, asset_row_id).await
    }

    pub async fn approve_validated_with_audit(
        &self,
        draft_id: &ChallengeDraftId,
        expected_validation_bundle_sha256: &agentics_domain::models::hashes::Sha256Digest,
        message: Option<&str>,
        admin_username: String,
        audit_event_id: agentics_domain::models::ids::ChallengeDraftAuditEventId,
    ) -> Result<()> {
        db::challenge_creation::approve_validated_challenge_draft_with_audit(
            self.pool,
            draft_id,
            expected_validation_bundle_sha256,
            message,
            admin_username,
            audit_event_id,
        )
        .await
    }

    pub async fn update_status_with_audit(
        &self,
        draft_id: &ChallengeDraftId,
        status: agentics_domain::models::challenge_creation::ChallengeDraftStatus,
        message: Option<&str>,
        audit_event: &CreateChallengeDraftAuditEventInput,
    ) -> Result<()> {
        db::challenge_creation::update_challenge_draft_status_with_audit(
            self.pool,
            draft_id,
            status,
            message,
            audit_event,
        )
        .await
    }

    pub async fn claim_for_publish(
        &self,
        draft_id: &str,
        publish_timeout_minutes: i32,
    ) -> Result<ClaimedChallengeDraftForPublish> {
        db::challenge_creation::claim_challenge_draft_for_publish(
            self.pool,
            draft_id,
            publish_timeout_minutes,
        )
        .await
    }

    pub async fn fail_publish(
        &self,
        draft_id: &str,
        publish_claim_id: &ChallengeDraftPublishClaimId,
        message: &str,
    ) -> Result<()> {
        db::challenge_creation::fail_challenge_draft_publish(
            self.pool,
            draft_id,
            publish_claim_id,
            message,
        )
        .await
    }

    pub async fn publish_archive(&self, input: &PublishArchiveChallengeDraftInput) -> Result<()> {
        db::challenge_creation::publish_archive_challenge_draft(self.pool, input).await
    }

    pub async fn publish_new(&self, input: &PublishNewChallengeDraftInput) -> Result<()> {
        db::challenge_creation::publish_new_challenge_draft(self.pool, input).await
    }

    pub async fn mark_published(
        &self,
        draft_id: &str,
        publish_claim_id: &ChallengeDraftPublishClaimId,
        published_challenge_id: Option<&ChallengeId>,
    ) -> Result<()> {
        db::challenge_creation::mark_challenge_draft_published(
            self.pool,
            draft_id,
            publish_claim_id,
            published_challenge_id,
        )
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SolutionSubmissionsRepository<'a> {
    pool: &'a PgPool,
}

impl SolutionSubmissionsRepository<'_> {
    pub async fn create_with_job(
        &self,
        input: &CreateSolutionSubmissionInput,
    ) -> Result<SolutionSubmissionRecord> {
        db::solution_submissions::create_solution_submission_with_job(self.pool, input).await
    }

    pub async fn ensure_parent_matches_scope(
        &self,
        parent_solution_submission_id: Option<&SolutionSubmissionId>,
        agent_id: &AgentId,
        challenge_id: &ChallengeId,
        target: &TargetName,
    ) -> Result<()> {
        db::solution_submissions::ensure_parent_solution_submission_matches_scope(
            self.pool,
            parent_solution_submission_id,
            agent_id,
            challenge_id,
            target,
        )
        .await
    }

    pub async fn delete(&self, solution_submission_id: &SolutionSubmissionId) -> Result<()> {
        db::solution_submissions::delete_solution_submission(self.pool, solution_submission_id)
            .await
    }

    pub async fn get_by_id(
        &self,
        id: &SolutionSubmissionId,
    ) -> Result<Option<SolutionSubmissionRecord>> {
        db::solution_submissions::get_solution_submission_by_id(self.pool, id).await
    }

    pub async fn get_public_by_id(
        &self,
        id: &SolutionSubmissionId,
    ) -> Result<Option<SolutionSubmissionRecord>> {
        db::solution_submissions::get_public_solution_submission_by_id(self.pool, id).await
    }

    pub async fn list_admin(
        &self,
        limit: i64,
    ) -> Result<Vec<agentics_domain::models::request::AdminSolutionSubmissionListItemDto>> {
        db::solution_submissions::list_admin_solution_submissions(self.pool, limit).await
    }

    pub async fn list_public_for_challenge(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<agentics_domain::models::request::PublicSolutionSubmissionListItemDto>> {
        db::solution_submissions::list_public_solution_submissions_for_challenge(
            self.pool,
            challenge_id,
            target,
            limit,
        )
        .await
    }

    pub async fn count_public_for_challenge(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
    ) -> Result<i64> {
        db::solution_submissions::count_public_solution_submissions_for_challenge(
            self.pool,
            challenge_id,
            target,
        )
        .await
    }

    pub async fn observer_stats(&self) -> Result<(i64, i64, i64)> {
        db::solution_submissions::public_observer_stats(self.pool).await
    }

    pub async fn count_recent_runs_for_agent_challenge(
        &self,
        agent_id: &AgentId,
        challenge_id: &ChallengeId,
        target: &TargetName,
        eval_type: ScoringMode,
        window_seconds: i64,
    ) -> Result<i64> {
        db::validation_quotas::count_recent_runs_for_agent_challenge(
            self.pool,
            agent_id,
            challenge_id,
            target,
            eval_type,
            window_seconds,
        )
        .await
    }

    pub async fn count_lifetime_runs_for_agent_challenge(
        &self,
        agent_id: &AgentId,
        challenge_id: &ChallengeId,
        target: &TargetName,
        eval_type: ScoringMode,
    ) -> Result<i64> {
        db::validation_quotas::count_lifetime_runs_for_agent_challenge(
            self.pool,
            agent_id,
            challenge_id,
            target,
            eval_type,
        )
        .await
    }
}

pub use db::solution_submissions::{
    CreateSolutionSubmissionInput, SolutionSubmissionQuotaAdmission, SolutionSubmissionRecord,
};

#[derive(Debug, Clone, Copy)]
pub struct EvaluationJobsRepository<'a> {
    pool: &'a PgPool,
}

impl EvaluationJobsRepository<'_> {
    pub async fn claim_next(
        &self,
        worker_id: &str,
        accelerators: WorkerAccelerators,
    ) -> Result<Option<EvaluationJobRecord>> {
        db::evaluation_jobs::claim_next_evaluation_job(self.pool, worker_id, accelerators).await
    }

    pub async fn refresh_claim(
        &self,
        job_id: &EvaluationJobId,
        worker_id: &str,
        attempt_count: i32,
    ) -> Result<bool> {
        db::evaluation_jobs::refresh_evaluation_job_claim(
            self.pool,
            job_id,
            worker_id,
            attempt_count,
        )
        .await
    }

    pub async fn requeue_for_capacity(
        &self,
        job_id: &EvaluationJobId,
        worker_id: &str,
        attempt_count: i32,
        last_error: &str,
    ) -> Result<bool> {
        db::evaluation_jobs::requeue_running_evaluation_job_for_capacity(
            self.pool,
            job_id,
            worker_id,
            attempt_count,
            last_error,
        )
        .await
    }

    pub async fn mark_ready(&self, job_id: &EvaluationJobId) -> Result<()> {
        db::evaluation_jobs::mark_evaluation_job_ready(self.pool, job_id).await
    }

    pub async fn queue(&self, input: &QueueEvaluationJobInput) -> Result<EvaluationJobRecord> {
        db::evaluation_jobs::queue_evaluation_job(self.pool, input).await
    }

    pub async fn count_active(&self, eval_type: ScoringMode) -> Result<i64> {
        db::evaluation_jobs::count_active_evaluation_jobs(self.pool, eval_type).await
    }

    pub async fn mark_started(&self, input: &MarkEvaluationStartedInput) -> Result<bool> {
        db::evaluations::mark_evaluation_started(self.pool, input).await
    }

    pub async fn mark_finished(&self, input: &PersistedEvaluationResult) -> Result<bool> {
        db::evaluations::mark_evaluation_finished(self.pool, input).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LeaderboardRepository<'a> {
    pool: &'a PgPool,
}

impl LeaderboardRepository<'_> {
    pub async fn list_entries(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<agentics_domain::models::request::LeaderboardEntryDto>> {
        db::leaderboard::list_leaderboard_entries(self.pool, challenge_id, target, limit).await
    }

    pub async fn list_entries_with_metric_payloads(
        &self,
        challenge_id: &ChallengeId,
        target: &TargetName,
        limit: i64,
    ) -> Result<Vec<LeaderboardMetricEntry>> {
        db::leaderboard::list_leaderboard_entries_with_metric_payloads(
            self.pool,
            challenge_id,
            target,
            limit,
        )
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PioneerCodesRepository<'a> {
    pool: &'a PgPool,
}

impl PioneerCodesRepository<'_> {
    pub async fn create(&self, input: &CreatePioneerCodeInput) -> Result<PioneerCodeRecord> {
        db::pioneer_codes::create_pioneer_code(self.pool, input).await
    }

    pub async fn list(&self) -> Result<Vec<PioneerCodeRecord>> {
        db::pioneer_codes::list_pioneer_codes(self.pool).await
    }

    pub async fn detail(
        &self,
        id: &AgentPioneerCodeId,
    ) -> Result<(PioneerCodeRecord, Vec<PioneerCodeUseRecord>)> {
        db::pioneer_codes::get_pioneer_code_detail(self.pool, id).await
    }

    pub async fn revoke(&self, id: &AgentPioneerCodeId) -> Result<RevokePioneerCodeOutcome> {
        db::pioneer_codes::revoke_pioneer_code(self.pool, id).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SessionsRepository<'a> {
    pool: &'a PgPool,
}

impl SessionsRepository<'_> {
    pub async fn upsert_github_creator_agent(
        &self,
        agent_id: &AgentId,
        github_user_id: i64,
        github_login: &str,
        max_active_agents: i64,
    ) -> Result<AgentId> {
        db::sessions::upsert_github_creator_agent(
            self.pool,
            agent_id,
            github_user_id,
            github_login,
            max_active_agents,
        )
        .await
    }

    pub async fn upsert_github_creator_agent_with_pioneer_code(
        &self,
        fallback_agent_id: &AgentId,
        github_user_id: i64,
        github_login: &str,
        pioneer_code_hash: Option<&str>,
        require_pioneer_code: bool,
        max_active_agents: i64,
    ) -> Result<AgentId> {
        db::sessions::upsert_github_creator_agent_with_pioneer_code(
            self.pool,
            fallback_agent_id,
            github_user_id,
            github_login,
            pioneer_code_hash,
            require_pioneer_code,
            max_active_agents,
        )
        .await
    }

    pub async fn create_github_oauth_state(
        &self,
        input: &CreateGithubOauthStateInput,
    ) -> Result<()> {
        db::sessions::create_github_oauth_state(self.pool, input).await
    }

    pub async fn consume_github_oauth_state(
        &self,
        state_hash: &str,
        browser_nonce_hash: &str,
    ) -> Result<Option<ConsumedGithubOauthState>> {
        db::sessions::consume_github_oauth_state(self.pool, state_hash, browser_nonce_hash).await
    }

    pub async fn create_creator_session(&self, input: &CreateCreatorSessionInput) -> Result<()> {
        db::sessions::create_creator_session(self.pool, input).await
    }

    pub async fn create_admin_session(&self, input: &CreateAdminSessionInput) -> Result<()> {
        db::sessions::create_admin_session(self.pool, input).await
    }

    pub async fn authenticate_creator(
        &self,
        session_token: &str,
    ) -> Result<Option<AuthenticatedCreatorSession>> {
        db::sessions::authenticate_creator_session(self.pool, session_token).await
    }

    pub async fn authenticate_admin(
        &self,
        session_token: &str,
    ) -> Result<Option<AuthenticatedAdminSession>> {
        db::sessions::authenticate_admin_session(self.pool, session_token).await
    }

    pub async fn delete_web_session_by_token(&self, session_token: &str) -> Result<()> {
        db::sessions::delete_web_session_by_token(self.pool, session_token).await
    }

    pub async fn delete_expired_web_auth_rows(&self) -> Result<()> {
        db::sessions::delete_expired_web_auth_rows(self.pool).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MaintenanceRepository<'a> {
    pool: &'a PgPool,
}

impl MaintenanceRepository<'_> {
    pub async fn upsert_service_heartbeat(
        &self,
        worker_id: &str,
        payload: &HeartbeatPayload,
    ) -> Result<()> {
        db::maintenance::upsert_service_heartbeat(self.pool, worker_id, payload).await
    }

    pub async fn list_service_heartbeats(
        &self,
    ) -> Result<Vec<agentics_domain::models::request::AdminServiceHeartbeatDto>> {
        db::maintenance::list_service_heartbeats(self.pool).await
    }

    pub async fn ensure_challenges_seeded_from_root(
        &self,
        challenges_root: &str,
        storage_root: &str,
    ) -> Result<usize> {
        db::maintenance::ensure_challenges_seeded_from_root(
            self.pool,
            challenges_root,
            storage_root,
        )
        .await
    }

    pub async fn reap_stuck_jobs(&self, timeout_minutes: i32) -> Result<StaleJobReapResult> {
        db::maintenance::reap_stuck_jobs(self.pool, timeout_minutes).await
    }
}
