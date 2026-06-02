use std::time::{Duration, SystemTime};

use agentics_config::Config;
use agentics_domain::models::challenge_creation::ChallengeReviewRecordCleanupResponse;
use agentics_error::{Result, ServiceError};
use agentics_persistence::Repositories;
use agentics_storage::{Storage, StorageKey};

use crate::storage_errors::storage_error_to_service_error;

/// Expire stale review records and purge private assets for rejected or abandoned
/// unpublished review_records after the configured grace period.
pub async fn cleanup_challenge_review_records(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
) -> Result<ChallengeReviewRecordCleanupResponse> {
    let repos = Repositories::new(pool);
    let abandoned = repos
        .challenge_review_records()
        .abandon_stale(config.quotas.challenge_review_record_ttl_days)
        .await?;
    let purge_candidates = repos
        .challenge_review_records()
        .list_unpublished_private_assets_for_purge(
            config.quotas.unpublished_challenge_asset_grace_days,
        )
        .await?;

    let mut purged = 0_i64;
    for asset in purge_candidates {
        let Some(asset) = repos
            .challenge_review_records()
            .mark_private_asset_purging(&asset.id)
            .await?
        else {
            continue;
        };
        storage
            .delete(&asset.storage_key)
            .await
            .map_err(storage_error_to_service_error)?;
        if let Some(temporary_storage_key) = &asset.temporary_storage_key {
            storage
                .delete(temporary_storage_key)
                .await
                .map_err(storage_error_to_service_error)?;
        }
        repos
            .challenge_review_records()
            .delete_private_asset(asset.id.as_str())
            .await?;
        purged = purged.checked_add(1).ok_or_else(|| {
            ServiceError::Internal("private asset purge count overflow".to_string())
        })?;
    }
    let tmp_cutoff = temporary_storage_cleanup_cutoff(config)?;
    let purged_temporary_storage_objects = storage
        .delete_prefix_older_than(&StorageKey::try_new("_tmp")?, tmp_cutoff)
        .await
        .map_err(storage_error_to_service_error)?;
    let purged_temporary_storage_objects = i64::try_from(purged_temporary_storage_objects)
        .map_err(|_| {
            ServiceError::Internal(
                "temporary storage cleanup count exceeds supported range".to_string(),
            )
        })?;

    Ok(ChallengeReviewRecordCleanupResponse {
        abandoned_review_records: abandoned,
        purged_private_assets: purged,
        purged_temporary_storage_objects,
    })
}

fn temporary_storage_cleanup_cutoff(config: &Config) -> Result<SystemTime> {
    let seconds = config
        .storage
        .tmp_object_grace_hours
        .checked_mul(60 * 60)
        .ok_or_else(|| {
            ServiceError::Internal("temporary storage grace window overflow".to_string())
        })?;
    SystemTime::now()
        .checked_sub(Duration::from_secs(seconds))
        .ok_or_else(|| {
            ServiceError::Internal("temporary storage cleanup cutoff underflow".to_string())
        })
}
