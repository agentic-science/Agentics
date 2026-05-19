//! Challenge draft, GitHub identity, private asset, and review lifecycle queries.

use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeDraftResponse, ChallengeDraftStatus,
    ChallengeDraftValidationRecordResponse, ChallengeDraftValidationStatus,
    ChallengePrivateAssetKind,
};
use crate::models::github::GithubPullRequestNumber;
use crate::models::hashes::GitCommitSha;
use crate::models::hashes::Sha256Digest;
use crate::models::ids::{
    AgentId, ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftPublishClaimId,
    ChallengeDraftValidationRecordId, ChallengePrivateAssetId,
};
use crate::models::localization::LocalizedText;
use crate::models::names::{AssetName, ChallengeName};
use crate::models::paths::{ManagedBundlePath, ManagedStatementPath, RepoRelativePath};
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};
use crate::storage::StorageKey;

use super::challenges::{add_challenge_owner_tx, publish_challenge_tx};
mod assets;
mod rows;

pub use assets::{
    activate_challenge_private_asset, fail_challenge_private_asset,
    private_asset_storage_key_has_active_reference, reserve_challenge_private_asset,
};
pub use rows::list_challenge_private_asset_states;
use rows::{
    list_private_assets_for_draft, list_validation_records_for_draft,
    optional_storage_key_from_row, row_to_draft_response, row_to_validation_record_response,
    storage_key_from_row,
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

/// Input for appending a draft audit event.
#[derive(Debug, Clone)]
pub struct CreateChallengeDraftAuditEventInput {
    pub event_id: ChallengeDraftAuditEventId,
    pub draft_id: ChallengeDraftId,
    pub actor_agent_id: Option<AgentId>,
    pub actor_admin_username: Option<String>,
    pub action: String,
    pub message: String,
    pub metadata: Value,
}

/// Input for atomically publishing one approved new-challenge draft.
#[derive(Debug, Clone)]
pub struct PublishNewChallengeDraftInput {
    pub draft_id: ChallengeDraftId,
    pub publish_claim_id: ChallengeDraftPublishClaimId,
    pub challenge_name: ChallengeName,
    pub bundle_path: ManagedBundlePath,
    pub statement_path: ManagedStatementPath,
    pub spec: ChallengeBundleSpec,
    pub title: String,
    pub summary: LocalizedText,
    pub owner_agent_id: AgentId,
    pub audit_event_id: ChallengeDraftAuditEventId,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Input for atomically publishing one approved archive draft.
#[derive(Debug, Clone)]
pub struct PublishArchiveChallengeDraftInput {
    pub draft_id: ChallengeDraftId,
    pub publish_claim_id: ChallengeDraftPublishClaimId,
    pub challenge_name: ChallengeName,
    pub owner_agent_id: AgentId,
    pub audit_event_id: ChallengeDraftAuditEventId,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Draft record claimed for a single publish attempt.
#[derive(Debug, Clone)]
pub struct ClaimedChallengeDraftForPublish {
    pub draft: ChallengeDraftResponse,
    pub publish_claim_id: Option<ChallengeDraftPublishClaimId>,
}

/// Input for reserving one draft validation admission slot before expensive work starts.
#[derive(Debug, Clone)]
pub struct BeginChallengeDraftValidationInput {
    pub validation_record_id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub repository_path: String,
    pub manifest_sha256: Sha256Digest,
}

/// Input for completing a previously reserved draft validation record.
#[derive(Debug, Clone)]
pub struct FinishChallengeDraftValidationInput {
    pub validation_record_id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub bundle_sha256: Option<Sha256Digest>,
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
) -> Result<ChallengeDraftResponse> {
    let manifest_json =
        serde_json::to_value(&input.manifest).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-drafts:agent:{}", input.creator_agent_id);
    lock_quota_scope(&mut tx, &scope).await?;
    let active_drafts =
        count_active_challenge_drafts_for_agent_tx(&mut tx, &input.creator_agent_id).await?;
    if active_drafts >= input.max_active_drafts {
        return Err(AppError::TooManyRequests(format!(
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
        AppError::Internal(format!(
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

/// Reserve one validation quota slot and record a running validation attempt.
pub async fn begin_challenge_draft_validation(
    pool: &PgPool,
    input: &BeginChallengeDraftValidationInput,
    window_seconds: i64,
    validation_limit: i64,
    validation_timeout_minutes: i32,
) -> Result<ChallengeDraftValidationRecordResponse> {
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-draft:{}:validations", input.draft_id);
    lock_quota_scope(&mut tx, &scope).await?;

    let status: Option<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT status, active_validation_record_id::text AS active_validation_record_id
        FROM challenge_drafts
        WHERE id = $1::uuid
        FOR UPDATE
        "#,
    )
    .bind(input.draft_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, active_validation_record_id)) = status else {
        return Err(AppError::NotFound);
    };
    let status = ChallengeDraftStatus::from_storage_value(&status)
        .ok_or_else(|| AppError::Internal(format!("unknown challenge draft status `{status}`")))?;
    if !matches!(
        status,
        ChallengeDraftStatus::Draft | ChallengeDraftStatus::Validated
    ) {
        return Err(AppError::Conflict);
    }
    let active_validation_record_id = if active_validation_record_id.is_some() {
        clear_stale_active_validation_tx(
            &mut tx,
            input.draft_id.as_str(),
            validation_timeout_minutes,
        )
        .await?;
        let refreshed_active: Option<String> = sqlx::query_scalar(
            "SELECT active_validation_record_id::text FROM challenge_drafts WHERE id = $1::uuid",
        )
        .bind(input.draft_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        refreshed_active
    } else {
        active_validation_record_id
    };
    if active_validation_record_id.is_some() {
        return Err(AppError::Conflict);
    }

    let recent_validations = count_recent_challenge_draft_validations_tx(
        &mut tx,
        input.draft_id.as_str(),
        window_seconds,
    )
    .await?;
    if recent_validations >= validation_limit {
        return Err(AppError::TooManyRequests(format!(
            "challenge draft validation quota exceeded for `{}`: {} of {} validations used in the last 24 hours",
            input.draft_id, recent_validations, validation_limit
        )));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO challenge_draft_validation_records (
            id, draft_id, status, message, repository_path, manifest_sha256, bundle_sha256
        )
        VALUES ($1::uuid, $2::uuid, 'running', $3, $4, $5, NULL)
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.draft_id.as_str())
    .bind("challenge draft validation is running")
    .bind(&input.repository_path)
    .bind(input.manifest_sha256.to_string())
    .fetch_one(&mut *tx)
    .await?;

    let claim = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET active_validation_record_id = $2::uuid,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(input.draft_id.as_str())
    .bind(input.validation_record_id.as_str())
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() != 1 {
        return Err(AppError::Conflict);
    }

    tx.commit().await?;
    row_to_validation_record_response(row)
}

/// Count validation attempts for one draft inside a rolling window under a quota lock.
async fn count_recent_challenge_draft_validations_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM challenge_draft_validation_records
        WHERE draft_id = $1::uuid
          AND created_at >= NOW() - ($2::TEXT || ' seconds')::INTERVAL
        "#,
    )
    .bind(draft_id)
    .bind(window_seconds)
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

/// Complete a reserved draft validation record and transition the draft status.
pub async fn finish_challenge_draft_validation(
    pool: &PgPool,
    input: &FinishChallengeDraftValidationInput,
) -> Result<ChallengeDraftValidationRecordResponse> {
    let mut tx = pool.begin().await?;
    let next_status = match input.status {
        ChallengeDraftValidationStatus::Passed => ChallengeDraftStatus::Validated,
        ChallengeDraftValidationStatus::Failed => ChallengeDraftStatus::Draft,
        ChallengeDraftValidationStatus::Running => {
            return Err(AppError::Internal(
                "running draft validation cannot finish as running".to_string(),
            ));
        }
    };

    let row = sqlx::query(
        r#"
        UPDATE challenge_draft_validation_records
        SET status = $3,
            message = $4,
            bundle_sha256 = $5
        WHERE id = $1::uuid
          AND draft_id = $2::uuid
          AND status = 'running'
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.draft_id.as_str())
    .bind(input.status.as_str())
    .bind(&input.message)
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(AppError::Conflict);
    };

    let update = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = $2,
            validation_message = $3,
            validation_repository_path = (
                SELECT repository_path
                FROM challenge_draft_validation_records
                WHERE id = $1::uuid
            ),
            validation_bundle_sha256 = $4,
            active_validation_record_id = NULL,
            updated_at = NOW()
        WHERE id = $5::uuid
          AND active_validation_record_id = $1::uuid
          AND status IN ('draft', 'validated')
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(next_status.as_str())
    .bind(&input.message)
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .bind(input.draft_id.as_str())
    .execute(&mut *tx)
    .await?;
    if update.rows_affected() == 0 {
        tx.commit().await?;
        return row_to_validation_record_response(row);
    }

    tx.commit().await?;
    row_to_validation_record_response(row)
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
        return Err(AppError::Conflict);
    }
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
        "#,
    )
    .bind(draft_id)
    .bind(status.as_str())
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict);
    }
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
        "#,
    )
    .bind(draft_id)
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict);
    }
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
        AppError::Internal("abandoned draft count exceeds supported range".to_string())
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

/// Claim an approved draft for publishing before filesystem work starts.
pub async fn claim_challenge_draft_for_publish(
    pool: &PgPool,
    draft_id: &str,
    publish_timeout_minutes: i32,
) -> Result<ClaimedChallengeDraftForPublish> {
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-draft:{draft_id}:publish");
    lock_quota_scope(&mut tx, &scope).await?;
    reset_stale_publishing_draft_tx(&mut tx, draft_id, publish_timeout_minutes).await?;

    let current: Option<String> =
        sqlx::query_scalar("SELECT status FROM challenge_drafts WHERE id = $1::uuid FOR UPDATE")
            .bind(draft_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(current) = current else {
        return Err(AppError::NotFound);
    };
    let current = ChallengeDraftStatus::from_storage_value(&current)
        .ok_or_else(|| AppError::Internal(format!("unknown challenge draft status `{current}`")))?;
    match current {
        ChallengeDraftStatus::Published => {
            tx.commit().await?;
            let draft = get_challenge_draft(pool, draft_id)
                .await?
                .ok_or(AppError::NotFound)?;
            return Ok(ClaimedChallengeDraftForPublish {
                draft,
                publish_claim_id: None,
            });
        }
        ChallengeDraftStatus::Approved => {}
        _ => return Err(AppError::Conflict),
    }

    let publish_claim_id = ChallengeDraftPublishClaimId::generate();
    let claim = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'publishing',
            publish_claim_id = $2::uuid,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'approved'
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(publish_claim_id.as_str())
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() != 1 {
        return Err(AppError::Conflict);
    }
    tx.commit().await?;

    let draft = get_challenge_draft(pool, draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(ClaimedChallengeDraftForPublish {
        draft,
        publish_claim_id: Some(publish_claim_id),
    })
}

/// Reset a stale publishing claim back to approved so a reviewer can retry.
async fn reset_stale_publishing_draft_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    timeout_minutes: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'approved',
            publish_claim_id = NULL,
            validation_message = 'previous publish attempt expired',
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND updated_at < NOW() - INTERVAL '1 minute' * $2
        "#,
    )
    .bind(draft_id)
    .bind(timeout_minutes.max(1))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Release a publishing claim after filesystem or DB publication fails.
pub async fn fail_challenge_draft_publish(
    pool: &PgPool,
    draft_id: &str,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    message: &str,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'approved',
            publish_claim_id = NULL,
            validation_message = $2,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
        "#,
    )
    .bind(draft_id)
    .bind(message)
    .bind(publish_claim_id.as_str())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

/// Mark a draft published and bind it to the published challenge row.
pub async fn mark_challenge_draft_published(
    pool: &PgPool,
    draft_id: &str,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    published_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_name = $2,
            publish_claim_id = NULL,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_name.map(ChallengeName::as_str))
    .bind(publish_claim_id.as_str())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

/// Publish an approved new-challenge draft as one retry-safe database unit.
pub async fn publish_new_challenge_draft(
    pool: &PgPool,
    input: &PublishNewChallengeDraftInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let published = publish_challenge_tx(
        &mut tx,
        &input.challenge_name,
        &input.bundle_path,
        &input.statement_path,
        &input.spec,
        &input.title,
        &input.summary,
    )
    .await?;
    add_challenge_owner_tx(&mut tx, &published.challenge_name, &input.owner_agent_id).await?;
    mark_challenge_draft_published_tx(
        &mut tx,
        input.draft_id.as_str(),
        &input.publish_claim_id,
        Some(&published.challenge_name),
    )
    .await?;
    create_challenge_draft_audit_event_tx(
        &mut tx,
        &CreateChallengeDraftAuditEventInput {
            event_id: input.audit_event_id.clone(),
            draft_id: input.draft_id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(input.admin_username.clone()),
            action: "draft_published".to_string(),
            message: "challenge draft published".to_string(),
            metadata: serde_json::json!({
                "challenge_name": &input.challenge_name,
                "published_challenge_name": &published.challenge_name,
                "repository_path": &input.repository_path,
                "bundle_sha256": input.bundle_sha256
            }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Publish an approved archive draft as one retry-safe database unit.
pub async fn publish_archive_challenge_draft(
    pool: &PgPool,
    input: &PublishArchiveChallengeDraftInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    ensure_agent_owns_challenge_tx(&mut tx, &input.challenge_name, &input.owner_agent_id).await?;
    archive_challenge_tx(&mut tx, &input.challenge_name).await?;
    mark_challenge_draft_published_tx(
        &mut tx,
        input.draft_id.as_str(),
        &input.publish_claim_id,
        None,
    )
    .await?;
    create_challenge_draft_audit_event_tx(
        &mut tx,
        &CreateChallengeDraftAuditEventInput {
            event_id: input.audit_event_id.clone(),
            draft_id: input.draft_id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(input.admin_username.clone()),
            action: "draft_published".to_string(),
            message: "challenge draft published".to_string(),
            metadata: serde_json::json!({
                "challenge_name": &input.challenge_name,
                "published_challenge_name": Value::Null,
                "repository_path": &input.repository_path,
                "bundle_sha256": input.bundle_sha256
            }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Require that an archive draft creator currently owns the target challenge.
async fn ensure_agent_owns_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
    agent_id: &AgentId,
) -> Result<()> {
    let owns_challenge = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_owners
            WHERE challenge_name = $1 AND agent_id = $2::uuid
        )
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(agent_id.as_str())
    .fetch_one(&mut **tx)
    .await?;
    if !owns_challenge {
        return Err(AppError::Forbidden(
            "only a challenge owner can publish an archive draft for this challenge".to_string(),
        ));
    }

    Ok(())
}

/// Marks challenge draft published tx in persistent state.
async fn mark_challenge_draft_published_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    published_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_name = $2,
            publish_claim_id = NULL,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_name.map(ChallengeName::as_str))
    .bind(publish_claim_id.as_str())
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

/// Handles archive challenge tx for this module.
async fn archive_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE name = $1
        "#,
    )
    .bind(challenge_name.as_str())
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Append a draft audit event.
pub async fn create_challenge_draft_audit_event(
    pool: &PgPool,
    input: &CreateChallengeDraftAuditEventInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    create_challenge_draft_audit_event_tx(&mut tx, input).await?;
    tx.commit().await?;
    Ok(())
}

/// Creates challenge draft audit event tx after validating caller inputs.
async fn create_challenge_draft_audit_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    input: &CreateChallengeDraftAuditEventInput,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO challenge_draft_audit_events (
            id, draft_id, actor_agent_id, actor_admin_username, action, message, metadata_json
        )
        VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7)
        "#,
    )
    .bind(input.event_id.as_str())
    .bind(input.draft_id.as_str())
    .bind(input.actor_agent_id.as_ref().map(AgentId::as_str))
    .bind(&input.actor_admin_username)
    .bind(&input.action)
    .bind(&input.message)
    .bind(&input.metadata)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
