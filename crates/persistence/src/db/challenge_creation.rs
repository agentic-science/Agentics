//! Challenge draft, GitHub identity, private asset, and review lifecycle queries.

use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeDraftResponse, ChallengeDraftStatus,
    ChallengePrivateAssetKind,
};
use agentics_domain::models::github::GithubPullRequestNumber;
use agentics_domain::models::hashes::GitCommitSha;
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{
    AgentId, ChallengeDraftAuditEventId, ChallengeDraftId, ChallengePrivateAssetId,
};
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
pub use audit::CreateChallengeDraftAuditEventInput;
use audit::create_challenge_draft_audit_event_tx;
pub use publishing::{
    ClaimedChallengeDraftForPublish, PublishArchiveChallengeDraftInput,
    PublishNewChallengeDraftInput, claim_challenge_draft_for_publish, fail_challenge_draft_publish,
    mark_challenge_draft_published, publish_archive_challenge_draft, publish_new_challenge_draft,
};
pub use rows::list_challenge_private_asset_states;
use rows::{
    list_private_assets_for_draft, list_validation_records_for_draft,
    optional_storage_key_from_row, row_to_draft_response, storage_key_from_row,
};
pub use validation::{
    BeginChallengeDraftValidationInput, FinishChallengeDraftValidationInput,
    begin_challenge_draft_validation, finish_challenge_draft_validation,
};

use super::ids::{challenge_draft_id_from_row, challenge_private_asset_id_from_row};

/// Input for inserting one GitHub PR-backed challenge draft.
#[derive(Debug, Clone)]
pub struct CreateChallengeDraftInput {
    pub draft_id: ChallengeDraftId,
    pub creator_agent_id: AgentId,
    pub max_active_drafts: i64,
    pub creator_github_user_id: i64,
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
    pub draft_id: ChallengeDraftId,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub size_bytes: i64,
    pub sha256: Sha256Digest,
    pub storage_key: StorageKey,
    pub temporary_storage_key: StorageKey,
    pub uploader_agent_id: AgentId,
}

/// Internal private asset cleanup candidate.
#[derive(Debug, Clone)]
pub struct ChallengePrivateAssetPurgeRecord {
    pub id: ChallengePrivateAssetId,
    pub storage_key: StorageKey,
    pub temporary_storage_key: Option<StorageKey>,
}

/// Insert a new challenge draft bound to a GitHub PR.
pub async fn create_challenge_draft(
    pool: &PgPool,
    input: &CreateChallengeDraftInput,
    audit_event: &CreateChallengeDraftAuditEventInput,
) -> Result<ChallengeDraftResponse> {
    if audit_event.draft_id != input.draft_id {
        return Err(ServiceError::Internal(
            "draft creation audit event targets a different draft".to_string(),
        ));
    }
    let manifest_json =
        serde_json::to_value(&input.manifest).map_err(|e| ServiceError::Internal(e.to_string()))?;
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-drafts:agent:{}", input.creator_agent_id);
    lock_quota_scope(&mut tx, &scope).await?;
    let active_drafts =
        count_active_challenge_drafts_for_agent_tx(&mut tx, &input.creator_agent_id).await?;
    if active_drafts >= input.max_active_drafts {
        return Err(ServiceError::TooManyRequests(format!(
            "challenge draft quota exceeded: {active_drafts} of {} active drafts are already open",
            input.max_active_drafts
        )));
    }
    let row = sqlx::query(
        r#"
        INSERT INTO challenge_drafts (
            id,
            challenge_name,
            request_kind,
            status,
            creator_agent_id,
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
        VALUES ($1::uuid, $2, $3, 'draft', $4::uuid, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING *
        "#,
    )
    .bind(input.draft_id.as_str())
    .bind(input.manifest.challenge_name.as_str())
    .bind(input.manifest.request.as_str())
    .bind(input.creator_agent_id.as_str())
    .bind(input.creator_github_user_id)
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

    create_challenge_draft_audit_event_tx(&mut tx, audit_event).await?;
    tx.commit().await?;

    row_to_draft_response(row, Vec::new(), Vec::new())
}

/// Get one draft with its private assets and validation records.
pub async fn get_challenge_draft(
    pool: &PgPool,
    draft_id: &str,
) -> Result<Option<ChallengeDraftResponse>> {
    let row = sqlx::query("SELECT * FROM challenge_drafts WHERE id = $1::uuid")
        .bind(draft_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let assets = list_private_assets_for_draft(pool, draft_id).await?;
    let validation_records = list_validation_records_for_draft(pool, draft_id).await?;
    row_to_draft_response(row, assets, validation_records).map(Some)
}

/// List recent drafts for admin review.
pub async fn list_challenge_drafts(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ChallengeDraftResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM challenge_drafts
        ORDER BY updated_at DESC, created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut drafts = Vec::with_capacity(rows.len());
    for row in rows {
        let draft_id = challenge_draft_id_from_row(&row, "id")?;
        let assets = list_private_assets_for_draft(pool, draft_id.as_str()).await?;
        let validation_records = list_validation_records_for_draft(pool, draft_id.as_str()).await?;
        drafts.push(row_to_draft_response(row, assets, validation_records)?);
    }
    Ok(drafts)
}

/// Count non-terminal drafts owned by an agent for creator quota enforcement.
async fn count_active_challenge_drafts_for_agent_tx(
    tx: &mut Transaction<'_, Postgres>,
    agent_id: &AgentId,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM challenge_drafts
        WHERE creator_agent_id = $1::uuid
          AND status IN ('draft', 'validated', 'approved', 'publishing')
        "#,
    )
    .bind(agent_id.as_str())
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Fail and clear active validation leases that exceeded the configured timeout.
pub(super) async fn clear_stale_active_validation_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    timeout_minutes: i32,
) -> Result<()> {
    let stale_validation_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT v.id::text
        FROM challenge_drafts d
        JOIN challenge_draft_validation_records v ON v.id = d.active_validation_record_id
        WHERE d.id = $1::uuid
          AND (
            v.status <> 'running'
            OR v.created_at < NOW() - INTERVAL '1 minute' * $2
          )
        "#,
    )
    .bind(draft_id)
    .bind(timeout_minutes.max(1))
    .fetch_optional(&mut **tx)
    .await?;
    let Some(stale_validation_id) = stale_validation_id else {
        return Ok(());
    };

    sqlx::query(
        r#"
        UPDATE challenge_draft_validation_records
        SET status = 'failed',
            message = 'challenge draft validation lease expired'
        WHERE id = $1::uuid
          AND status = 'running'
        "#,
    )
    .bind(&stale_validation_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET active_validation_record_id = NULL,
            validation_message = 'challenge draft validation lease expired',
            updated_at = NOW()
        WHERE id = $1::uuid
          AND active_validation_record_id = $2::uuid
        "#,
    )
    .bind(draft_id)
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

/// Approve the latest validated draft content and freeze its review digest.
pub async fn approve_validated_challenge_draft(
    pool: &PgPool,
    draft_id: &str,
    message: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
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
    .bind(draft_id)
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Approve a validated draft and audit the exact digest approved in one transaction.
pub async fn approve_validated_challenge_draft_with_audit(
    pool: &PgPool,
    draft_id: &ChallengeDraftId,
    expected_validation_bundle_sha256: &Sha256Digest,
    message: Option<&str>,
    admin_username: String,
    event_id: ChallengeDraftAuditEventId,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        UPDATE challenge_drafts
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
    .bind(draft_id.as_str())
    .bind(message)
    .bind(expected_validation_bundle_sha256.to_string())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        return Err(ServiceError::Conflict);
    };
    let approved_bundle_sha256: Option<String> = row.try_get("approved_bundle_sha256")?;
    create_challenge_draft_audit_event_tx(
        &mut tx,
        &CreateChallengeDraftAuditEventInput {
            event_id,
            draft_id: draft_id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(admin_username),
            action: "draft_approved".to_string(),
            message: message.unwrap_or_default().to_string(),
            metadata: serde_json::json!({ "approved_bundle_sha256": approved_bundle_sha256 }),
        },
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Move a draft to a review status.
pub async fn update_challenge_draft_status(
    pool: &PgPool,
    draft_id: &str,
    status: ChallengeDraftStatus,
    message: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = $2,
            validation_message = COALESCE($3, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('draft', 'validated', 'approved')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(status.as_str())
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Move a draft to a review status and append its audit event atomically.
pub async fn update_challenge_draft_status_with_audit(
    pool: &PgPool,
    draft_id: &ChallengeDraftId,
    status: ChallengeDraftStatus,
    message: Option<&str>,
    audit_event: &CreateChallengeDraftAuditEventInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = $2,
            validation_message = COALESCE($3, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('draft', 'validated', 'approved')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id.as_str())
    .bind(status.as_str())
    .bind(message)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    create_challenge_draft_audit_event_tx(&mut tx, audit_event).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark one draft abandoned unless it has already been published.
pub async fn abandon_challenge_draft(
    pool: &PgPool,
    draft_id: &str,
    message: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'abandoned',
            validation_message = COALESCE($2, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('draft', 'validated', 'approved', 'rejected')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Mark one draft abandoned and append its audit event atomically.
pub async fn abandon_challenge_draft_with_audit(
    pool: &PgPool,
    draft_id: &ChallengeDraftId,
    message: Option<&str>,
    audit_event: &CreateChallengeDraftAuditEventInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'abandoned',
            validation_message = COALESCE($2, validation_message),
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('draft', 'validated', 'approved', 'rejected')
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id.as_str())
    .bind(message)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    create_challenge_draft_audit_event_tx(&mut tx, audit_event).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark inactive unpublished drafts abandoned after the configured TTL.
pub async fn abandon_stale_challenge_drafts(pool: &PgPool, ttl_days: i64) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'abandoned',
            validation_message = COALESCE(validation_message, 'draft abandoned due to inactivity'),
            updated_at = NOW()
        WHERE status IN ('draft', 'validated', 'approved')
          AND updated_at < NOW() - ($1::TEXT || ' days')::INTERVAL
        "#,
    )
    .bind(ttl_days)
    .execute(pool)
    .await?;

    i64::try_from(result.rows_affected()).map_err(|_| {
        ServiceError::Internal("abandoned draft count exceeds supported range".to_string())
    })
}

/// List private assets eligible for cleanup because their draft did not publish.
pub async fn list_unpublished_private_assets_for_purge(
    pool: &PgPool,
    grace_days: i64,
) -> Result<Vec<ChallengePrivateAssetPurgeRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.storage_key, a.temporary_storage_key
        FROM challenge_private_assets a
        JOIN challenge_drafts d ON d.id = a.draft_id
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
pub async fn delete_challenge_private_asset(pool: &PgPool, asset_row_id: &str) -> Result<()> {
    sqlx::query(
        r#"
        WITH deleted AS (
            DELETE FROM challenge_private_assets
            WHERE id = $1::uuid
            RETURNING draft_id
        )
        UPDATE challenge_drafts d
        SET updated_at = NOW()
        WHERE d.id IN (SELECT draft_id FROM deleted)
        "#,
    )
    .bind(asset_row_id)
    .execute(pool)
    .await?;

    Ok(())
}
