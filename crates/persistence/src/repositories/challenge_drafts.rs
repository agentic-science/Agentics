use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    BeginChallengeDraftValidationInput, ChallengePrivateAssetPurgeRecord,
    ClaimedChallengeDraftForPublish, CreateChallengeDraftAuditEventInput,
    CreateChallengeDraftInput, CreateChallengePrivateAssetInput,
    FinishChallengeDraftValidationInput, PublishArchiveChallengeDraftInput,
    PublishNewChallengeDraftInput,
};
use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetResponse, ChallengeDraftResponse,
    ChallengeDraftValidationRecordResponse, ChallengePrivateAssetResponse,
};
use agentics_domain::models::ids::{AgentId, ChallengeDraftId, ChallengeDraftPublishClaimId};
use agentics_domain::storage::StorageKey;
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct ChallengeDraftsRepository<'a> {
    pub(super) pool: &'a PgPool,
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

    pub async fn mark_private_asset_purging(
        &self,
        asset_row_id: &agentics_domain::models::ids::ChallengePrivateAssetId,
    ) -> Result<Option<ChallengePrivateAssetPurgeRecord>> {
        db::challenge_creation::mark_challenge_private_asset_purging(self.pool, asset_row_id).await
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
        published_challenge_name: Option<&agentics_domain::models::names::ChallengeName>,
    ) -> Result<()> {
        db::challenge_creation::mark_challenge_draft_published(
            self.pool,
            draft_id,
            publish_claim_id,
            published_challenge_name,
        )
        .await
    }
}
