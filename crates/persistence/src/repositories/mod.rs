//! Repository facades over SQLx persistence primitives.
//!
//! These types keep SQL ownership inside `agentics-persistence` while giving
//! services and transport crates explicit boundaries instead of importing a
//! broad bag of free functions.

mod agents;
mod challenge_drafts;
mod challenges;
mod evaluation_jobs;
mod leaderboard;
mod maintenance;
mod pioneer_codes;
mod sessions;
mod solution_submissions;

use sqlx::PgPool;

pub use agents::AgentsRepository;
pub use challenge_drafts::ChallengeDraftsRepository;
pub use challenges::ChallengesRepository;
pub use evaluation_jobs::EvaluationJobsRepository;
pub use leaderboard::LeaderboardRepository;
pub use maintenance::MaintenanceRepository;
pub use pioneer_codes::PioneerCodesRepository;
pub use sessions::SessionsRepository;
pub use solution_submissions::SolutionSubmissionsRepository;

pub use crate::db::agents::{AgentRecord, AuthenticatedAgent, RegisterAgentInput};
pub use crate::db::challenge_creation::{
    AdminChallengePrivateAssetRecord, BeginChallengeDraftValidationInput, ChallengeDraftRecord,
    ChallengeDraftValidationRecord, ChallengePrivateAssetPurgeRecord, ChallengePrivateAssetRecord,
    ClaimedChallengeDraftForPublish, CreateChallengeDraftAuditEventInput,
    CreateChallengeDraftInput, CreateChallengePrivateAssetInput,
    FinishChallengeDraftValidationInput, PublishArchiveChallengeDraftInput,
    PublishNewChallengeDraftInput,
};
pub use crate::db::challenges::{
    AdminChallengeListItemRecord, ChallengeCatalogFilters, ChallengeMoltbookDiscussionRecord,
    ChallengeRecord, ChallengeShortlistRecord, ChallengeShortlistRevisionRecord,
    ChallengeShortlistedAgentRecord, CreateChallengeShortlistRevisionInput,
    CreatorChallengeParticipantRecord, CreatorChallengeParticipantsRecord,
    CreatorChallengeStatsRecord, PublishChallengeInput, PublishChallengeRecord,
    PublishedChallengeList, PublishedChallengeListItemRecord,
};
pub use crate::db::evaluation_jobs::{
    EvaluationJobRecord, QueueEvaluationJobInput, RunnerJobClaimRecord,
};
pub use crate::db::evaluation_policy::PublishedChallengeAdmission;
pub use crate::db::evaluations::{MarkEvaluationStartedInput, PersistedEvaluationResult};
pub use crate::db::leaderboard::{LeaderboardMetricEntry, LeaderboardRecord};
pub use crate::db::maintenance::{HeartbeatPayload, ServiceHeartbeatRecord, StaleJobReapResult};
pub use crate::db::pioneer_codes::{
    CreatePioneerCodeInput, PioneerCodeRecord, PioneerCodeRegistrationKind, PioneerCodeUseRecord,
    RevokePioneerCodeOutcome,
};
pub use crate::db::sessions::{
    AuthenticatedAdminSession, AuthenticatedCreatorSession, ConsumedGithubOauthState,
    CreateAdminSessionInput, CreateCreatorSessionInput, CreateGithubOauthStateInput,
};
pub use crate::db::solution_submissions::{
    AdminSolutionSubmissionListItemRecord, CreateSolutionSubmissionInput,
    PublicSolutionSubmissionListItemRecord, SolutionSubmissionQuotaAdmission,
    SolutionSubmissionRecord,
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
