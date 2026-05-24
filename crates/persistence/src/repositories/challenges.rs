use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    ChallengeCatalogFilters, ChallengeMoltbookDiscussionRecord, ChallengeRecord,
    CreateChallengeShortlistRevisionInput, PublishChallengeInput, PublishedChallengeAdmission,
    PublishedChallengeList,
};
use agentics_domain::error::Result;
use agentics_domain::models::challenge::AdminChallengeListItemDto;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AgentId, ChallengeId};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    ChallengeShortlistResponse, ChallengeShortlistRevisionResponse,
    CreatorChallengeParticipantsResponse, CreatorChallengeStatsResponse,
};
use agentics_domain::models::urls::MoltbookPostUrl;

#[derive(Debug, Clone, Copy)]
pub struct ChallengesRepository<'a> {
    pub(super) pool: &'a PgPool,
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
