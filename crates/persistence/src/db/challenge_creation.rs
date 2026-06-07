//! Challenge review record, GitHub identity, private asset, and review lifecycle queries.

use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::auth::GithubUserId;
use agentics_domain::models::challenge_creation::{
    ChallengeCreationManifest, ChallengePrivateAssetKind, ChallengeReviewRecordStatus,
};
use agentics_domain::models::github::GithubPullRequestNumber;
use agentics_domain::models::hashes::GitCommitSha;
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{ChallengePrivateAssetId, ChallengeReviewRecordId, HumanId};
use agentics_domain::models::names::AssetName;
use agentics_domain::models::paths::RepoRelativePath;
use agentics_domain::models::urls::{GithubPullRequestUrl, GithubRepoRemote};
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};

mod assets;
mod audit;
mod publishing;
mod rows;
mod validation;

pub use assets::{
    activate_challenge_private_asset, activate_challenge_private_asset_with_audit,
    fail_challenge_private_asset, private_asset_storage_key_has_active_reference,
    reserve_challenge_private_asset,
};
pub use audit::CreateChallengeReviewRecordAuditEventInput;
use audit::create_challenge_review_audit_event_tx;
pub use publishing::{
    ClaimedChallengeReviewRecordForPublish, PublishArchiveChallengeReviewRecordInput,
    PublishNewChallengeReviewRecordInput, claim_challenge_review_record_for_publish,
    fail_challenge_review_record_publish, mark_challenge_review_record_published,
    publish_archive_challenge_review_record, publish_new_challenge_review_record,
};
pub use rows::{
    AdminChallengePrivateAssetRecord, ChallengePrivateAssetRecord, ChallengeReviewRecordRecord,
    ChallengeReviewValidationRecord, list_challenge_private_asset_states,
};
use rows::{
    list_private_assets_for_review_record, list_validation_records_for_review_record,
    optional_storage_key_from_row, row_to_review_record, storage_key_from_row,
};
pub use validation::{
    BeginChallengeReviewRecordValidationInput, FinishChallengeReviewRecordValidationInput,
    begin_challenge_review_record_validation, finish_challenge_review_record_validation,
};

use super::ids::{challenge_private_asset_id_from_row, challenge_review_record_id_from_row};

/// Input for inserting one GitHub PR-backed challenge review record.
#[derive(Debug, Clone)]
pub struct CreateChallengeReviewRecordInput {
    pub review_record_id: ChallengeReviewRecordId,
    pub creator_human_id: HumanId,
    pub max_active_review_records: i64,
    pub creator_github_user_id: GithubUserId,
    pub creator_github_login: String,
    pub repo_url: GithubRepoRemote,
    pub pr_number: GithubPullRequestNumber,
    pub pr_url: GithubPullRequestUrl,
    pub commit_sha: GitCommitSha,
    pub challenge_path: RepoRelativePath,
    pub manifest_sha256: Sha256Digest,
    pub manifest: ChallengeCreationManifest,
}

/// Input for persisting one private benchmark asset.
#[derive(Debug, Clone)]
pub struct CreateChallengePrivateAssetInput {
    pub asset_row_id: ChallengePrivateAssetId,
    pub review_record_id: ChallengeReviewRecordId,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub size_bytes: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    pub temporary_storage_key: StorageKey,
    pub uploader_human_id: HumanId,
}

/// Internal private asset cleanup candidate.
#[derive(Debug, Clone)]
pub struct ChallengePrivateAssetPurgeRecord {
    pub id: ChallengePrivateAssetId,
    pub storage_key: StorageKey,
    pub temporary_storage_key: Option<StorageKey>,
}

/// Insert a new challenge review record bound to a GitHub PR.
pub async fn create_challenge_review_record(
    pool: &PgPool,
    input: &CreateChallengeReviewRecordInput,
    audit_event: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<ChallengeReviewRecordRecord> {
    if audit_event.review_record_id != input.review_record_id {
        return Err(ServiceError::Internal(
            "review record creation audit event targets a different review record".to_string(),
        ));
    }
    let manifest_json =
        serde_json::to_value(&input.manifest).map_err(|e| ServiceError::Internal(e.to_string()))?;
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-review-records:human:{}", input.creator_human_id);
    lock_quota_scope(&mut tx, &scope).await?;
    let active_review_records =
        count_active_challenge_review_records_for_human_tx(&mut tx, &input.creator_human_id)
            .await?;
    if active_review_records >= input.max_active_review_records {
        return Err(ServiceError::TooManyRequests(format!(
            "challenge review record quota exceeded: {active_review_records} of {} active review records are already open",
            input.max_active_review_records
        )));
    }
    let row = sqlx::query(
        r#"
        INSERT INTO challenge_review_records (
            id,
            challenge_name,
            request_kind,
            status,
            creator_human_id,
            creator_github_user_id,
            creator_github_login,
            repo_url,
            repo_key,
            pr_number,
            pr_url,
            commit_sha,
            challenge_path,
            manifest_sha256,
            manifest_json
        )
        VALUES ($1::uuid, $2, $3, 'pending_review', $4::uuid, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING *
        "#,
    )
    .bind(input.review_record_id.as_str())
    .bind(input.manifest.challenge_name.as_str())
    .bind(input.manifest.request.as_str())
    .bind(input.creator_human_id.as_str())
    .bind(input.creator_github_user_id.as_i64())
    .bind(&input.creator_github_login)
    .bind(input.repo_url.as_str())
    .bind(input.repo_url.repository_key().as_str())
    .bind(input.pr_number.as_i32().map_err(|e| {
        ServiceError::Internal(format!(
            "invalid typed pull request number reached database: {e}"
        ))
    })?)
    .bind(input.pr_url.as_str())
    .bind(input.commit_sha.to_string())
    .bind(input.challenge_path.as_str())
    .bind(input.manifest_sha256.to_string())
    .bind(&manifest_json)
    .fetch_one(&mut *tx)
    .await?;

    create_challenge_review_audit_event_tx(&mut tx, audit_event).await?;
    tx.commit().await?;

    row_to_review_record(row, Vec::new(), Vec::new())
}

/// Get one review_record with its private assets and validation records.
pub async fn get_challenge_review_record(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
) -> Result<Option<ChallengeReviewRecordRecord>> {
    let row = sqlx::query("SELECT * FROM challenge_review_records WHERE id = $1::uuid")
        .bind(review_record_id.as_str())
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let assets = list_private_assets_for_review_record(pool, review_record_id).await?;
    let validation_records =
        list_validation_records_for_review_record(pool, review_record_id).await?;
    row_to_review_record(row, assets, validation_records).map(Some)
}

/// List recent review_records for admin review.
pub async fn list_challenge_review_records(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ChallengeReviewRecordRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM challenge_review_records
        ORDER BY updated_at DESC, created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut review_records = Vec::with_capacity(rows.len());
    for row in rows {
        let review_record_id = challenge_review_record_id_from_row(&row, "id")?;
        let assets = list_private_assets_for_review_record(pool, &review_record_id).await?;
        let validation_records =
            list_validation_records_for_review_record(pool, &review_record_id).await?;
        review_records.push(row_to_review_record(row, assets, validation_records)?);
    }
    Ok(review_records)
}

/// Count non-terminal review_records owned by a human for creator quota enforcement.
async fn count_active_challenge_review_records_for_human_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM challenge_review_records
        WHERE creator_human_id = $1::uuid
          AND status IN ('pending_review', 'validated', 'approved', 'publishing')
        "#,
    )
    .bind(human_id.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Fail and clear active validation leases that exceeded the configured timeout.
pub(super) async fn clear_stale_active_validation_tx(
    tx: &mut Transaction<'_, Postgres>,
    review_record_id: &str,
    timeout_minutes: i32,
) -> Result<()> {
    let stale_validation_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT v.id::text
        FROM challenge_review_records d
        JOIN challenge_review_validation_records v ON v.id = d.active_validation_record_id
        WHERE d.id = $1::uuid
          AND (
            v.status <> 'running'
            OR v.created_at < NOW() - INTERVAL '1 minute' * $2
          )
        "#,
    )
    .bind(review_record_id)
    .bind(timeout_minutes.max(1))
    .fetch_optional(&mut **tx)
    .await?;
    let Some(stale_validation_id) = stale_validation_id else {
        return Ok(());
    };

    sqlx::query(
        r#"
        UPDATE challenge_review_validation_records
        SET status = 'failed',
            message = 'challenge review record validation lease expired'
        WHERE id = $1::uuid
          AND status = 'running'
        "#,
    )
    .bind(&stale_validation_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET active_validation_record_id = NULL,
            validation_message = 'challenge review record validation lease expired',
            updated_at = NOW()
        WHERE id = $1::uuid
          AND active_validation_record_id = $2::uuid
        "#,
    )
    .bind(review_record_id)
    .bind(&stale_validation_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Handles lock quota scope for this module.
pub(super) async fn lock_quota_scope(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO quota_admission_locks (scope)
        VALUES ($1)
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .bind(scope)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        SELECT scope
        FROM quota_admission_locks
        WHERE scope = $1
        FOR UPDATE
        "#,
    )
    .bind(scope)
    .fetch_one(&mut **tx)
    .await?;

    Ok(())
}

/// Approve the latest validated review_record content and freeze its review digest.
pub async fn approve_validated_challenge_review_record(
    pool: &PgPool,
    review_record_id: &str,
    message: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'approved',
            validation_message = COALESCE($2, validation_message),
            approved_bundle_sha256 = validation_bundle_sha256,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'validated'
          AND validation_bundle_sha256 IS NOT NULL
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id)
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Approve a validated review_record and audit the exact digest approved in one transaction.
pub async fn approve_validated_challenge_review_record_with_audit(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
    expected_validation_bundle_sha256: &Sha256Digest,
    message: Option<&str>,
    audit_event: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<()> {
    if audit_event.review_record_id != *review_record_id {
        return Err(ServiceError::Internal(
            "review record approval audit event targets a different review record".to_string(),
        ));
    }
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'approved',
            validation_message = COALESCE($2, validation_message),
            approved_bundle_sha256 = validation_bundle_sha256,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'validated'
          AND validation_bundle_sha256 IS NOT NULL
          AND validation_bundle_sha256 = $3
          AND active_validation_record_id IS NULL
        RETURNING approved_bundle_sha256
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(message)
    .bind(expected_validation_bundle_sha256.to_string())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        return Err(ServiceError::Conflict);
    };
    let approved_bundle_sha256: Option<String> = row.try_get("approved_bundle_sha256")?;
    create_challenge_review_audit_event_tx(
        &mut tx,
        &CreateChallengeReviewRecordAuditEventInput {
            event_id: audit_event.event_id.clone(),
            review_record_id: review_record_id.clone(),
            actor_human_id: audit_event.actor_human_id.clone(),
            actor_admin_service_token_id: audit_event.actor_admin_service_token_id.clone(),
            actor_display: audit_event.actor_display.clone(),
            action: "review_record_approved".to_string(),
            message: message.unwrap_or_default().to_string(),
            metadata: serde_json::json!({ "approved_bundle_sha256": approved_bundle_sha256 }),
        },
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Move a review_record to a review status.
pub async fn update_challenge_review_record_status(
    pool: &PgPool,
    review_record_id: &str,
    status: ChallengeReviewRecordStatus,
    message: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = $2,
            validation_message = COALESCE($3, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('pending_review', 'validated', 'approved')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id)
    .bind(status.as_str())
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Move a review_record to a review status and append its audit event atomically.
pub async fn update_challenge_review_record_status_with_audit(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
    status: ChallengeReviewRecordStatus,
    message: Option<&str>,
    audit_event: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = $2,
            validation_message = COALESCE($3, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('pending_review', 'validated', 'approved')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(status.as_str())
    .bind(message)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    create_challenge_review_audit_event_tx(&mut tx, audit_event).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark one review_record abandoned unless it has already been published.
pub async fn abandon_challenge_review_record(
    pool: &PgPool,
    review_record_id: &str,
    message: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'abandoned',
            validation_message = COALESCE($2, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('pending_review', 'validated', 'approved', 'rejected')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id)
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Mark one review_record abandoned and append its audit event atomically.
pub async fn abandon_challenge_review_record_with_audit(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
    message: Option<&str>,
    audit_event: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'abandoned',
            validation_message = COALESCE($2, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('pending_review', 'validated', 'approved', 'rejected')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(message)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    create_challenge_review_audit_event_tx(&mut tx, audit_event).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark inactive unpublished review_records abandoned after the configured TTL.
pub async fn abandon_stale_challenge_review_records(pool: &PgPool, ttl_days: i64) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'abandoned',
            validation_message = COALESCE(validation_message, 'review record abandoned due to inactivity'),
            updated_at = NOW()
        WHERE status IN ('pending_review', 'validated', 'approved')
          AND updated_at < NOW() - ($1::TEXT || ' days')::INTERVAL
        "#,
    )
    .bind(ttl_days)
    .execute(pool)
    .await?;

    i64::try_from(result.rows_affected()).map_err(|_| {
        ServiceError::Internal("abandoned review record count exceeds supported range".to_string())
    })
}

/// List private assets eligible for cleanup because their review_record did not publish.
pub async fn list_unpublished_private_assets_for_purge(
    pool: &PgPool,
    grace_days: i64,
) -> Result<Vec<ChallengePrivateAssetPurgeRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.storage_key, a.temporary_storage_key
        FROM challenge_private_assets a
        JOIN challenge_review_records d ON d.id = a.review_record_id
        WHERE d.status IN ('abandoned', 'rejected')
          AND d.updated_at < NOW() - ($1::TEXT || ' days')::INTERVAL
          AND a.status IN ('pending', 'active', 'failed', 'purging')
        ORDER BY a.created_at ASC
        "#,
    )
    .bind(grace_days)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(ChallengePrivateAssetPurgeRecord {
                id: challenge_private_asset_id_from_row(&row, "id")?,
                storage_key: storage_key_from_row(&row, "storage_key")?,
                temporary_storage_key: optional_storage_key_from_row(
                    &row,
                    "temporary_storage_key",
                )?,
            })
        })
        .collect()
}

/// Mark a purge-eligible private asset as purging before deleting storage.
pub async fn mark_challenge_private_asset_purging(
    pool: &PgPool,
    asset_row_id: &ChallengePrivateAssetId,
) -> Result<Option<ChallengePrivateAssetPurgeRecord>> {
    let row = sqlx::query(
        r#"
        UPDATE challenge_private_assets
        SET status = 'purging'
        WHERE id = $1::uuid
          AND status IN ('pending', 'active', 'failed', 'purging')
        RETURNING id, storage_key, temporary_storage_key
        "#,
    )
    .bind(asset_row_id.as_str())
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(ChallengePrivateAssetPurgeRecord {
            id: challenge_private_asset_id_from_row(&row, "id")?,
            storage_key: storage_key_from_row(&row, "storage_key")?,
            temporary_storage_key: optional_storage_key_from_row(&row, "temporary_storage_key")?,
        })
    })
    .transpose()
}

/// Delete a private asset record after its object has been removed.
pub async fn delete_challenge_private_asset(
    pool: &PgPool,
    asset_row_id: &ChallengePrivateAssetId,
) -> Result<()> {
    sqlx::query(
        r#"
        WITH deleted AS (
            DELETE FROM challenge_private_assets
            WHERE id = $1::uuid
            RETURNING review_record_id
        )
        UPDATE challenge_review_records d
        SET updated_at = NOW()
        WHERE d.id IN (SELECT review_record_id FROM deleted)
        "#,
    )
    .bind(asset_row_id.as_str())
    .execute(pool)
    .await?;

    Ok(())
}
