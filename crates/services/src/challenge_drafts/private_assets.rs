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
    ChallengeDraftStatus, ChallengePrivateAssetResponse,
};
use agentics_domain::models::ids::{ChallengeDraftAuditEventId, ChallengePrivateAssetId};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};
use agentics_storage::{Storage, StorageKey, StorageWriteIntent};

use super::types::UploadChallengePrivateAssetServiceRequest;
use super::utils::{base64_decode, cleanup_storage_key};

const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;

/// Upload a private benchmark asset for a draft owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: UploadChallengePrivateAssetServiceRequest,
) -> Result<ChallengePrivateAssetResponse> {
    let UploadChallengePrivateAssetServiceRequest {
        creator_agent_id,
        draft_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let draft = repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    if draft.creator_agent_id != creator_agent_id {
        return Err(ServiceError::NotFound);
    }
    if matches!(
        draft.status,
        ChallengeDraftStatus::Rejected
            | ChallengeDraftStatus::Approved
            | ChallengeDraftStatus::Publishing
            | ChallengeDraftStatus::Published
            | ChallengeDraftStatus::Abandoned
    ) {
        return Err(ServiceError::Conflict);
    }

    let requirement = draft
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
    if asset_size_bytes > config.quotas.challenge_private_asset_bytes_per_draft {
        return Err(ServiceError::BadRequest(format!(
            "private asset must be at most {} bytes",
            config.quotas.challenge_private_asset_bytes_per_draft
        )));
    }
    let asset_size_bytes_i64 = i64::try_from(asset_size_bytes).map_err(|_| {
        ServiceError::BadRequest("private asset size exceeds supported database range".to_string())
    })?;
    validate_private_asset_zip_upload(
        &asset_bytes,
        body.asset_name.as_str(),
        config.quotas.challenge_private_asset_bytes_per_draft,
    )
    .await?;
    let sha256 = challenge_creation::sha256_digest(&asset_bytes);
    let storage_key = StorageKey::try_new(format!(
        "challenge-drafts/{}/private-assets/{}-{}.bin",
        draft.id, body.asset_name, sha256
    ))?;
    let temporary_asset_key = StorageKey::try_new(format!(
        "_tmp/challenge-private-assets/{}-{}-{}.bin",
        draft.id,
        body.asset_name,
        Uuid::new_v4()
    ))?;
    let asset_row_id = ChallengePrivateAssetId::generate();
    repos
        .challenge_drafts()
        .reserve_private_asset(
            &persistence::CreateChallengePrivateAssetInput {
                asset_row_id: asset_row_id.clone(),
                draft_id: draft.id.clone(),
                asset_name: body.asset_name.clone(),
                kind: body.kind,
                required: requirement.required,
                size_bytes: asset_size_bytes_i64,
                sha256,
                storage_key: storage_key.clone(),
                temporary_storage_key: temporary_asset_key.clone(),
                uploader_agent_id: creator_agent_id.clone(),
            },
            config.quotas.challenge_private_asset_bytes_per_draft,
            config.quotas.challenge_draft_validation_timeout_minutes,
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
                config.quotas.challenge_private_asset_bytes_per_draft,
            ),
        )
        .await
    {
        Ok(key) => key,
        Err(error) => {
            fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
            cleanup_storage_key(storage, &temporary_asset_key).await;
            return Err(error.into());
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
        return Err(error.into());
    }
    let asset = match Repositories::new(pool)
        .challenge_drafts()
        .activate_private_asset_with_audit(
            &asset_row_id,
            ChallengeDraftAuditEventId::generate(),
            &creator_agent_id,
        )
        .await
    {
        Ok(asset) => asset,
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
        .challenge_drafts()
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
    if !storage.exists(storage_key).await? {
        return Ok(());
    }
    if Repositories::new(pool)
        .challenge_drafts()
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
