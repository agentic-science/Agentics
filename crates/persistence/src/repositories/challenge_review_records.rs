use sqlx::PgPool;

use crate::db;
use crate::repositories::{
    AdminChallengePrivateAssetRecord, BeginChallengeReviewRecordValidationInput,
    ChallengePrivateAssetPurgeRecord, ChallengePrivateAssetRecord, ChallengeReviewRecordRecord,
    ChallengeReviewValidationRecord, ClaimedChallengeReviewRecordForPublish,
    CreateChallengePrivateAssetInput, CreateChallengeReviewRecordAuditEventInput,
    CreateChallengeReviewRecordInput, FinishChallengeReviewRecordValidationInput,
    PublishArchiveChallengeReviewRecordInput, PublishNewChallengeReviewRecordInput,
};
use agentics_domain::models::ids::{
    ChallengePrivateAssetId, ChallengeReviewPublishClaimId, ChallengeReviewRecordId, HumanId,
};
use agentics_domain::storage::StorageKey;
use agentics_error::Result;

#[derive(Debug, Clone, Copy)]
pub struct ChallengeReviewRecordsRepository<'a> {
    pub(super) pool: &'a PgPool,
}

impl ChallengeReviewRecordsRepository<'_> {
    pub async fn create(
        &self,
        input: &CreateChallengeReviewRecordInput,
        audit_event: &CreateChallengeReviewRecordAuditEventInput,
    ) -> Result<ChallengeReviewRecordRecord> {
        db::challenge_creation::create_challenge_review_record(self.pool, input, audit_event).await
    }

    pub async fn get(
        &self,
        review_record_id: &ChallengeReviewRecordId,
    ) -> Result<Option<ChallengeReviewRecordRecord>> {
        db::challenge_creation::get_challenge_review_record(self.pool, review_record_id).await
    }

    pub async fn list(&self, limit: i64) -> Result<Vec<ChallengeReviewRecordRecord>> {
        db::challenge_creation::list_challenge_review_records(self.pool, limit).await
    }

    pub async fn list_private_asset_states(
        &self,
        review_record_id: &ChallengeReviewRecordId,
    ) -> Result<Vec<AdminChallengePrivateAssetRecord>> {
        db::challenge_creation::list_challenge_private_asset_states(self.pool, review_record_id)
            .await
    }

    pub async fn reserve_private_asset(
        &self,
        input: &CreateChallengePrivateAssetInput,
        max_bytes_per_review_record: u64,
        validation_timeout_minutes: i32,
        pending_timeout_minutes: i32,
    ) -> Result<ChallengePrivateAssetRecord> {
        db::challenge_creation::reserve_challenge_private_asset(
            self.pool,
            input,
            max_bytes_per_review_record,
            validation_timeout_minutes,
            pending_timeout_minutes,
        )
        .await
    }

    pub async fn activate_private_asset_with_audit(
        &self,
        asset_row_id: &agentics_domain::models::ids::ChallengePrivateAssetId,
        audit_event_id: agentics_domain::models::ids::ChallengeReviewAuditEventId,
        uploader_human_id: &HumanId,
    ) -> Result<ChallengePrivateAssetRecord> {
        db::challenge_creation::activate_challenge_private_asset_with_audit(
            self.pool,
            asset_row_id,
            audit_event_id,
            uploader_human_id,
        )
        .await
    }

    pub async fn activate_private_asset(
        &self,
        asset_row_id: &agentics_domain::models::ids::ChallengePrivateAssetId,
    ) -> Result<ChallengePrivateAssetRecord> {
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
        input: &BeginChallengeReviewRecordValidationInput,
        window_seconds: i64,
        validation_limit: i64,
        validation_timeout_minutes: i32,
    ) -> Result<ChallengeReviewValidationRecord> {
        db::challenge_creation::begin_challenge_review_record_validation(
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
        input: &FinishChallengeReviewRecordValidationInput,
        audit_event: &CreateChallengeReviewRecordAuditEventInput,
    ) -> Result<ChallengeReviewValidationRecord> {
        db::challenge_creation::finish_challenge_review_record_validation(
            self.pool,
            input,
            audit_event,
        )
        .await
    }

    pub async fn abandon_with_audit(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        message: Option<&str>,
        audit_event: &CreateChallengeReviewRecordAuditEventInput,
    ) -> Result<()> {
        db::challenge_creation::abandon_challenge_review_record_with_audit(
            self.pool,
            review_record_id,
            message,
            audit_event,
        )
        .await
    }

    pub async fn abandon_stale(&self, ttl_days: i64) -> Result<i64> {
        db::challenge_creation::abandon_stale_challenge_review_records(self.pool, ttl_days).await
    }

    pub async fn list_unpublished_private_assets_for_purge(
        &self,
        grace_days: i64,
    ) -> Result<Vec<ChallengePrivateAssetPurgeRecord>> {
        db::challenge_creation::list_unpublished_private_assets_for_purge(self.pool, grace_days)
            .await
    }

    pub async fn delete_private_asset(&self, asset_row_id: &ChallengePrivateAssetId) -> Result<()> {
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
        review_record_id: &ChallengeReviewRecordId,
        expected_validation_bundle_sha256: &agentics_domain::models::hashes::Sha256Digest,
        message: Option<&str>,
        audit_event: &CreateChallengeReviewRecordAuditEventInput,
    ) -> Result<()> {
        db::challenge_creation::approve_validated_challenge_review_record_with_audit(
            self.pool,
            review_record_id,
            expected_validation_bundle_sha256,
            message,
            audit_event,
        )
        .await
    }

    pub async fn update_status_with_audit(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        status: agentics_domain::models::challenge_creation::ChallengeReviewRecordStatus,
        message: Option<&str>,
        audit_event: &CreateChallengeReviewRecordAuditEventInput,
    ) -> Result<()> {
        db::challenge_creation::update_challenge_review_record_status_with_audit(
            self.pool,
            review_record_id,
            status,
            message,
            audit_event,
        )
        .await
    }

    pub async fn claim_for_publish(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        publish_timeout_minutes: i32,
    ) -> Result<ClaimedChallengeReviewRecordForPublish> {
        db::challenge_creation::claim_challenge_review_record_for_publish(
            self.pool,
            review_record_id,
            publish_timeout_minutes,
        )
        .await
    }

    pub async fn fail_publish(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        publish_claim_id: &ChallengeReviewPublishClaimId,
        message: &str,
    ) -> Result<()> {
        db::challenge_creation::fail_challenge_review_record_publish(
            self.pool,
            review_record_id,
            publish_claim_id,
            message,
        )
        .await
    }

    pub async fn publish_archive(
        &self,
        input: &PublishArchiveChallengeReviewRecordInput,
    ) -> Result<()> {
        db::challenge_creation::publish_archive_challenge_review_record(self.pool, input).await
    }

    pub async fn publish_new(&self, input: &PublishNewChallengeReviewRecordInput) -> Result<()> {
        db::challenge_creation::publish_new_challenge_review_record(self.pool, input).await
    }

    pub async fn mark_published(
        &self,
        review_record_id: &ChallengeReviewRecordId,
        publish_claim_id: &ChallengeReviewPublishClaimId,
        published_challenge_name: Option<&agentics_domain::models::names::ChallengeName>,
    ) -> Result<()> {
        db::challenge_creation::mark_challenge_review_record_published(
            self.pool,
            review_record_id,
            publish_claim_id,
            published_challenge_name,
        )
        .await
    }
}
