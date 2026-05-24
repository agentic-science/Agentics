use sqlx::{PgPool, Postgres, Row, Transaction};

use super::rows::{draft_status_from_row, row_to_private_asset_response};
use super::{
    CreateChallengeDraftAuditEventInput, clear_stale_active_validation_tx,
    create_challenge_draft_audit_event_tx, lock_quota_scope,
};
use crate::db::ids::agent_id_from_row;
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge_creation::{
    ChallengeDraftStatus, ChallengePrivateAssetResponse,
};
use agentics_domain::models::ids::{AgentId, ChallengeDraftAuditEventId, ChallengePrivateAssetId};
use agentics_domain::storage::StorageKey;

use super::CreateChallengePrivateAssetInput;

/// Reserve a pending private benchmark asset row before storage writes begin.
pub async fn reserve_challenge_private_asset(
    pool: &PgPool,
    input: &CreateChallengePrivateAssetInput,
    max_bytes_per_draft: u64,
    validation_timeout_minutes: i32,
    pending_timeout_minutes: i32,
) -> Result<ChallengePrivateAssetResponse> {
    let max_bytes_per_draft = i64::try_from(max_bytes_per_draft).map_err(|_| {
        ServiceError::Internal("private asset quota limit exceeds supported range".to_string())
    })?;
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-draft:{}:private-assets", input.draft_id);
    lock_quota_scope(&mut tx, &scope).await?;
    auto_fail_stale_pending_private_assets_tx(
        &mut tx,
        input.draft_id.as_str(),
        pending_timeout_minutes,
    )
    .await?;
    ensure_private_asset_upload_allowed_tx(&mut tx, input, validation_timeout_minutes).await?;

    let existing_bytes =
        sum_private_asset_bytes_for_draft_tx(&mut tx, input.draft_id.as_str()).await?;
    let next_total = existing_bytes
        .checked_add(input.size_bytes)
        .ok_or_else(|| ServiceError::BadRequest("private asset size overflow".to_string()))?;
    if next_total > max_bytes_per_draft {
        return Err(ServiceError::TooManyRequests(format!(
            "private asset quota exceeded for draft `{}`: {} of {} bytes would be used",
            input.draft_id, next_total, max_bytes_per_draft
        )));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO challenge_private_assets (
            id,
            draft_id,
            asset_name,
            kind,
            required,
            size_bytes,
            sha256,
            storage_key,
            temporary_storage_key,
            status,
            uploader_agent_id
        )
        VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, 'pending', $10::uuid)
        RETURNING *
        "#,
    )
    .bind(input.asset_row_id.as_str())
    .bind(input.draft_id.as_str())
    .bind(input.asset_name.as_str())
    .bind(input.kind.as_str())
    .bind(input.required)
    .bind(input.size_bytes)
    .bind(input.sha256.to_string())
    .bind(input.storage_key.as_str())
    .bind(input.temporary_storage_key.as_str())
    .bind(input.uploader_agent_id.as_str())
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = CASE WHEN status = 'validated' THEN 'draft' ELSE status END,
            validation_message = CASE WHEN status = 'validated' THEN NULL ELSE validation_message END,
            validation_repository_path = CASE WHEN status = 'validated' THEN NULL ELSE validation_repository_path END,
            validation_bundle_sha256 = CASE WHEN status = 'validated' THEN NULL ELSE validation_bundle_sha256 END,
            approved_bundle_sha256 = CASE WHEN status = 'validated' THEN NULL ELSE approved_bundle_sha256 END,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('draft', 'validated')
        "#,
    )
    .bind(input.draft_id.as_str())
    .execute(&mut *tx)
    .await?;

    let response = row_to_private_asset_response(row)?;
    tx.commit().await?;
    Ok(response)
}

/// Mark a pending private asset active after its object was promoted.
pub async fn activate_challenge_private_asset(
    pool: &PgPool,
    asset_row_id: &ChallengePrivateAssetId,
) -> Result<ChallengePrivateAssetResponse> {
    let mut tx = pool.begin().await?;
    let response = activate_challenge_private_asset_tx(&mut tx, asset_row_id).await?;
    tx.commit().await?;
    Ok(response)
}

/// Mark a pending private asset active and audit the activation atomically.
pub async fn activate_challenge_private_asset_with_audit(
    pool: &PgPool,
    asset_row_id: &ChallengePrivateAssetId,
    audit_event_id: ChallengeDraftAuditEventId,
    actor_agent_id: &AgentId,
) -> Result<ChallengePrivateAssetResponse> {
    let mut tx = pool.begin().await?;
    let response = activate_challenge_private_asset_tx(&mut tx, asset_row_id).await?;
    create_challenge_draft_audit_event_tx(
        &mut tx,
        &CreateChallengeDraftAuditEventInput {
            event_id: audit_event_id,
            draft_id: response.draft_id.clone(),
            actor_agent_id: Some(actor_agent_id.clone()),
            actor_admin_username: None,
            action: "private_asset_uploaded".to_string(),
            message: "private benchmark asset uploaded".to_string(),
            metadata: serde_json::json!({
                "asset_name": &response.asset_name,
                "kind": response.kind,
                "size_bytes": response.size_bytes,
                "sha256": &response.sha256
            }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(response)
}

/// Mark a pending private asset active inside an existing transaction.
async fn activate_challenge_private_asset_tx(
    tx: &mut Transaction<'_, Postgres>,
    asset_row_id: &ChallengePrivateAssetId,
) -> Result<ChallengePrivateAssetResponse> {
    let row = sqlx::query(
        r#"
        UPDATE challenge_private_assets
        SET status = 'active',
            temporary_storage_key = NULL,
            activated_at = NOW(),
            failed_at = NULL,
            failure_message = NULL
        WHERE id = $1::uuid
          AND status = 'pending'
        RETURNING *
    "#,
    )
    .bind(asset_row_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Err(ServiceError::Conflict);
    };
    sqlx::query(
        r#"
        UPDATE challenge_drafts d
        SET updated_at = NOW()
        FROM challenge_private_assets a
        WHERE a.id = $1::uuid
          AND d.id = a.draft_id
    "#,
    )
    .bind(asset_row_id.as_str())
    .execute(&mut **tx)
    .await?;
    row_to_private_asset_response(row)
}

/// Mark a pending private asset failed after storage write or promote failed.
pub async fn fail_challenge_private_asset(
    pool: &PgPool,
    asset_row_id: &ChallengePrivateAssetId,
    message: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        WITH failed AS (
            UPDATE challenge_private_assets
            SET status = 'failed',
                failed_at = NOW(),
                failure_message = $2
            WHERE id = $1::uuid
              AND status = 'pending'
            RETURNING draft_id
        )
        UPDATE challenge_drafts d
        SET updated_at = NOW()
        WHERE d.id IN (SELECT draft_id FROM failed)
        "#,
    )
    .bind(asset_row_id.as_str())
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

/// Return whether a private asset storage key is owned by an active asset row.
pub async fn private_asset_storage_key_has_active_reference(
    pool: &PgPool,
    storage_key: &StorageKey,
) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_private_assets
            WHERE storage_key = $1
              AND status = 'active'
        )
        "#,
    )
    .bind(storage_key.as_str())
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Lock a draft row and confirm private assets may still be attached.
async fn ensure_private_asset_upload_allowed_tx(
    tx: &mut Transaction<'_, Postgres>,
    input: &CreateChallengePrivateAssetInput,
    validation_timeout_minutes: i32,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT status, creator_agent_id, active_validation_record_id::text AS active_validation_record_id
        FROM challenge_drafts
        WHERE id = $1::uuid
        FOR UPDATE
        "#,
    )
    .bind(input.draft_id.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Err(ServiceError::NotFound);
    };

    let creator_agent_id = agent_id_from_row(&row, "creator_agent_id")?;
    if creator_agent_id != input.uploader_agent_id {
        return Err(ServiceError::NotFound);
    }
    if row
        .try_get::<Option<String>, _>("active_validation_record_id")?
        .is_some()
    {
        clear_stale_active_validation_tx(tx, input.draft_id.as_str(), validation_timeout_minutes)
            .await?;
        let active_validation_record_id: Option<String> = sqlx::query_scalar(
            "SELECT active_validation_record_id::text FROM challenge_drafts WHERE id = $1::uuid",
        )
        .bind(input.draft_id.as_str())
        .fetch_one(&mut **tx)
        .await?;
        if active_validation_record_id.is_some() {
            return Err(ServiceError::Conflict);
        }
    }
    let status = draft_status_from_row(&row, "status")?;
    if !matches!(
        status,
        ChallengeDraftStatus::Draft | ChallengeDraftStatus::Validated
    ) {
        return Err(ServiceError::Conflict);
    }

    Ok(())
}

/// Fail stale pending private assets before retrying the same asset name.
async fn auto_fail_stale_pending_private_assets_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    timeout_minutes: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        WITH failed AS (
            UPDATE challenge_private_assets
            SET status = 'failed',
                failed_at = NOW(),
                failure_message = 'private asset pending lease expired'
            WHERE draft_id = $1::uuid
              AND status = 'pending'
              AND created_at < NOW() - INTERVAL '1 minute' * $2
            RETURNING draft_id
        )
        UPDATE challenge_drafts d
        SET updated_at = NOW()
        WHERE d.id IN (SELECT draft_id FROM failed)
        "#,
    )
    .bind(draft_id)
    .bind(timeout_minutes.max(1))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Handles sum private asset bytes for draft tx for this module.
async fn sum_private_asset_bytes_for_draft_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
) -> Result<i64> {
    let bytes = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(SUM(size_bytes), 0)::BIGINT
        FROM challenge_private_assets
        WHERE draft_id = $1::uuid
          AND status IN ('pending', 'active')
        "#,
    )
    .bind(draft_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(bytes)
}
