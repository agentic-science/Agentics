//! Private asset ZIP validation and extraction helpers.

use std::path::Path;

use tracing::warn;
use uuid::Uuid;

use agentics_config::Config;
use agentics_contracts::challenge_creation;
use agentics_contracts::validation::archive::{
    ArchiveEnvelopePolicy, extract_zip_bytes_to_dir, inspect_zip_bytes,
};
use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetResponse, ChallengeReviewRecordStatus,
};
use agentics_domain::models::ids::{ChallengePrivateAssetId, ChallengeReviewAuditEventId};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};
use agentics_storage::{Storage, StorageKey, StorageWriteIntent};

use super::presentation::{private_asset_response, review_record_response};
use super::types::UploadChallengePrivateAssetServiceRequest;
use super::utils::{base64_decode, cleanup_storage_key};
use crate::storage_errors::storage_error_to_service_error;

const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;

/// Upload a private benchmark asset for a review_record owned by the authenticated human.
pub async fn upload_challenge_private_asset(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: UploadChallengePrivateAssetServiceRequest,
) -> Result<ChallengePrivateAssetResponse> {
    let UploadChallengePrivateAssetServiceRequest {
        creator_human_id,
        review_record_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let review_record = repos
        .challenge_review_records()
        .get(&review_record_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let review_record = review_record_response(review_record);
    if review_record.creator_human_id != creator_human_id {
        return Err(ServiceError::NotFound);
    }
    if matches!(
        review_record.status,
        ChallengeReviewRecordStatus::Rejected
            | ChallengeReviewRecordStatus::Approved
            | ChallengeReviewRecordStatus::Publishing
            | ChallengeReviewRecordStatus::Published
            | ChallengeReviewRecordStatus::Abandoned
    ) {
        return Err(ServiceError::Conflict);
    }

    let requirement = review_record
        .manifest
        .private_assets
        .iter()
        .find(|asset| asset.asset_name == body.asset_name)
        .ok_or_else(|| {
            ServiceError::BadRequest(format!(
                "private asset `{}` is not declared in the challenge manifest",
                body.asset_name
            ))
        })?;
    if requirement.kind != body.kind {
        return Err(ServiceError::BadRequest(format!(
            "private asset `{}` kind mismatch",
            body.asset_name
        )));
    }

    let asset_bytes = base64_decode(&body.asset_base64).ok_or(ServiceError::Base64)?;
    let asset_size_bytes = u64::try_from(asset_bytes.len()).map_err(|_| {
        ServiceError::BadRequest("private asset size exceeds supported range".to_string())
    })?;
    if asset_size_bytes
        > config
            .quotas
            .challenge_private_asset_bytes_per_review_record
    {
        return Err(ServiceError::BadRequest(format!(
            "private asset must be at most {} bytes",
            config
                .quotas
                .challenge_private_asset_bytes_per_review_record
        )));
    }
    let asset_size_bytes_i64 = i64::try_from(asset_size_bytes).map_err(|_| {
        ServiceError::BadRequest("private asset size exceeds supported database range".to_string())
    })?;
    validate_private_asset_zip_upload(
        &asset_bytes,
        body.asset_name.as_str(),
        config
            .quotas
            .challenge_private_asset_bytes_per_review_record,
    )
    .await?;
    let sha256 = challenge_creation::sha256_digest(&asset_bytes);
    let storage_key = StorageKey::try_new(format!(
        "challenge-review-records/{}/private-assets/{}-{}.bin",
        review_record.id, body.asset_name, sha256
    ))?;
    let temporary_asset_key = StorageKey::try_new(format!(
        "_tmp/challenge-private-assets/{}-{}-{}.bin",
        review_record.id,
        body.asset_name,
        Uuid::new_v4()
    ))?;
    let asset_row_id = ChallengePrivateAssetId::generate();
    repos
        .challenge_review_records()
        .reserve_private_asset(
            &persistence::CreateChallengePrivateAssetInput {
                asset_row_id: asset_row_id.clone(),
                review_record_id: review_record.id.clone(),
                asset_name: body.asset_name.clone(),
                kind: body.kind,
                required: requirement.required,
                size_bytes: asset_size_bytes_i64,
                sha256,
                storage_key: storage_key.clone(),
                temporary_storage_key: temporary_asset_key.clone(),
                uploader_human_id: creator_human_id.clone(),
            },
            config
                .quotas
                .challenge_private_asset_bytes_per_review_record,
            config
                .quotas
                .challenge_review_record_validation_timeout_minutes,
            config
                .quotas
                .challenge_private_asset_pending_timeout_minutes,
        )
        .await
        .map_err(ServiceError::unique_violation_as_conflict)?;

    let temporary_storage_key = match storage
        .put(
            &temporary_asset_key,
            &asset_bytes,
            StorageWriteIntent::new(
                "challenge private asset ZIP",
                config
                    .quotas
                    .challenge_private_asset_bytes_per_review_record,
            ),
        )
        .await
    {
        Ok(key) => key,
        Err(error) => {
            fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
            cleanup_storage_key(storage, &temporary_asset_key).await;
            return Err(storage_error_to_service_error(error));
        }
    };

    if let Err(error) = cleanup_unreferenced_private_asset_object(pool, storage, &storage_key).await
    {
        fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
        cleanup_storage_key(storage, &temporary_storage_key).await;
        return Err(error);
    }

    if let Err(error) = storage.promote(&temporary_storage_key, &storage_key).await {
        fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
        cleanup_storage_key(storage, &temporary_storage_key).await;
        return Err(storage_error_to_service_error(error));
    }
    let asset = match Repositories::new(pool)
        .challenge_review_records()
        .activate_private_asset_with_audit(
            &asset_row_id,
            ChallengeReviewAuditEventId::generate(),
            &creator_human_id,
        )
        .await
    {
        Ok(asset) => private_asset_response(asset),
        Err(error) => {
            cleanup_storage_key(storage, &storage_key).await;
            return Err(error);
        }
    };

    Ok(asset)
}

/// Marks the pending private asset failed when storage writes cannot complete.
async fn fail_challenge_private_asset_record(
    pool: &sqlx::PgPool,
    asset_row_id: &ChallengePrivateAssetId,
    message: &str,
) {
    if let Err(error) = Repositories::new(pool)
        .challenge_review_records()
        .fail_private_asset(asset_row_id, message)
        .await
    {
        warn!(
            asset_row_id = %asset_row_id,
            error = %error,
            "failed to mark private asset upload failed after storage error"
        );
    }
}

/// Remove an unreferenced durable object left by a stale pending private asset.
async fn cleanup_unreferenced_private_asset_object(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    storage_key: &StorageKey,
) -> Result<()> {
    if !storage
        .exists(storage_key)
        .await
        .map_err(storage_error_to_service_error)?
    {
        return Ok(());
    }
    if Repositories::new(pool)
        .challenge_review_records()
        .private_asset_storage_key_has_active_reference(storage_key)
        .await?
    {
        return Err(ServiceError::Conflict);
    }
    cleanup_storage_key(storage, storage_key).await;
    Ok(())
}

/// Validate a private asset ZIP before the bytes become durable storage state.
pub(super) async fn validate_private_asset_zip_upload(
    bytes: &[u8],
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        validate_private_asset_zip_upload_blocking(&bytes, &asset_name, max_uncompressed_bytes)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("private asset validation task failed: {e}")))?
}

/// Inspect a private asset ZIP for envelope safety without extracting it.
fn validate_private_asset_zip_upload_blocking(
    bytes: &[u8],
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let policy = ArchiveEnvelopePolicy::new(
        format!("private asset `{asset_name}`"),
        max_uncompressed_bytes,
        MAX_PRIVATE_ASSET_FILE_COUNT,
        max_uncompressed_bytes,
    );
    inspect_zip_bytes(bytes, &policy)?;
    Ok(())
}

/// Extracts one private asset ZIP overlay on a blocking worker thread.
pub(super) async fn extract_private_asset_overlay(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        extract_private_asset_overlay_blocking(
            &bytes,
            &target_dir,
            &asset_name,
            max_uncompressed_bytes,
        )
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("private asset extraction task failed: {e}")))?
}

/// Expands a private asset ZIP while enforcing containment, size, and no-overwrite rules.
pub(super) fn extract_private_asset_overlay_blocking(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let policy = ArchiveEnvelopePolicy::new(
        format!("private asset `{asset_name}`"),
        max_uncompressed_bytes,
        MAX_PRIVATE_ASSET_FILE_COUNT,
        max_uncompressed_bytes,
    );
    extract_zip_bytes_to_dir(bytes, target_dir, &policy)
}
