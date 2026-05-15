//! Challenge draft, GitHub identity, private asset, and review lifecycle queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;
use crate::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeCreationRequestKind, ChallengeDraftResponse,
    ChallengeDraftStatus, ChallengeDraftValidationRecordResponse, ChallengeDraftValidationStatus,
    ChallengePrivateAssetKind, ChallengePrivateAssetResponse,
};
use crate::models::hashes::GitCommitSha;
use crate::models::hashes::Sha256Digest;
use crate::models::ids::{
    AgentId, ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftValidationRecordId,
    ChallengePrivateAssetId,
};
use crate::models::names::{AssetName, ChallengeName};
use crate::models::paths::{ManagedBundlePath, ManagedStatementPath, RepoRelativePath};
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};
use crate::storage::StorageKey;

use super::challenges::{add_challenge_owner_tx, publish_challenge_tx};
use super::ids::{
    agent_id_from_row, asset_name_from_row, challenge_draft_id_from_row,
    challenge_draft_validation_record_id_from_row, challenge_name_from_row,
    challenge_private_asset_id_from_row, optional_challenge_name_from_row,
};

/// Input for inserting one GitHub PR-backed challenge draft.
#[derive(Debug, Clone)]
pub struct CreateChallengeDraftInput {
    pub draft_id: ChallengeDraftId,
    pub creator_agent_id: AgentId,
    pub creator_github_user_id: i64,
    pub creator_github_login: String,
    pub repo_url: GithubRepoRemote,
    pub pr_number: i32,
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
    pub challenge_name: ChallengeName,
    pub bundle_path: ManagedBundlePath,
    pub statement_path: ManagedStatementPath,
    pub spec: ChallengeBundleSpec,
    pub title: String,
    pub summary: String,
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
    pub challenge_name: ChallengeName,
    pub audit_event_id: ChallengeDraftAuditEventId,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Input for recording one admin validation attempt against a draft.
#[derive(Debug, Clone)]
pub struct RecordChallengeDraftValidationInput {
    pub validation_record_id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub repository_path: String,
    pub manifest_sha256: Sha256Digest,
    pub bundle_sha256: Option<Sha256Digest>,
}

/// Insert a new challenge draft bound to a GitHub PR.
pub async fn create_challenge_draft(
    pool: &PgPool,
    input: &CreateChallengeDraftInput,
) -> Result<ChallengeDraftResponse> {
    let manifest_json =
        serde_json::to_value(&input.manifest).map_err(|e| AppError::Internal(e.to_string()))?;
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
    .bind(input.pr_number)
    .bind(input.pr_url.as_str())
    .bind(input.commit_sha.to_string())
    .bind(input.challenge_path.as_str())
    .bind(input.manifest_sha256.to_string())
    .bind(&manifest_json)
    .fetch_one(pool)
    .await?;

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
pub async fn count_active_challenge_drafts_for_agent(pool: &PgPool, agent_id: &str) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM challenge_drafts
        WHERE creator_agent_id = $1::uuid
          AND status IN ('draft', 'validated', 'approved')
        "#,
    )
    .bind(agent_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Count validation attempts for one draft inside a rolling window.
pub async fn count_recent_challenge_draft_validations(
    pool: &PgPool,
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
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Insert a private benchmark asset record for a draft.
pub async fn create_challenge_private_asset(
    pool: &PgPool,
    input: &CreateChallengePrivateAssetInput,
    max_bytes_per_draft: u64,
) -> Result<ChallengePrivateAssetResponse> {
    let max_bytes_per_draft = i64::try_from(max_bytes_per_draft).map_err(|_| {
        AppError::Internal("private asset quota limit exceeds supported range".to_string())
    })?;
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-draft:{}:private-assets", input.draft_id);
    lock_quota_scope(&mut tx, &scope).await?;

    let existing_bytes =
        sum_private_asset_bytes_for_draft_tx(&mut tx, input.draft_id.as_str()).await?;
    let next_total = existing_bytes
        .checked_add(input.size_bytes)
        .ok_or_else(|| AppError::BadRequest("private asset size overflow".to_string()))?;
    if next_total > max_bytes_per_draft {
        return Err(AppError::TooManyRequests(format!(
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
            uploader_agent_id
        )
        VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9::uuid)
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
    .bind(input.uploader_agent_id.as_str())
    .fetch_one(&mut *tx)
    .await?;

    let response = row_to_private_asset_response(row)?;
    tx.commit().await?;
    Ok(response)
}

async fn sum_private_asset_bytes_for_draft_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
) -> Result<i64> {
    let bytes = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(SUM(size_bytes), 0)::BIGINT
        FROM challenge_private_assets
        WHERE draft_id = $1::uuid
        "#,
    )
    .bind(draft_id)
    .fetch_one(&mut **tx)
    .await?;

    Ok(bytes)
}

async fn lock_quota_scope(tx: &mut Transaction<'_, Postgres>, scope: &str) -> Result<()> {
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

/// Record a validation outcome and move draft status accordingly.
pub async fn record_challenge_draft_validation(
    pool: &PgPool,
    input: &RecordChallengeDraftValidationInput,
) -> Result<ChallengeDraftValidationRecordResponse> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO challenge_draft_validation_records (
            id, draft_id, status, message, repository_path, manifest_sha256, bundle_sha256
        )
        VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.draft_id.as_str())
    .bind(input.status.as_str())
    .bind(&input.message)
    .bind(&input.repository_path)
    .bind(input.manifest_sha256.to_string())
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .fetch_one(&mut *tx)
    .await?;

    let next_status = match input.status {
        ChallengeDraftValidationStatus::Passed => ChallengeDraftStatus::Validated,
        ChallengeDraftValidationStatus::Failed => ChallengeDraftStatus::Draft,
    };
    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = $2,
            validation_message = $3,
            validation_repository_path = $4,
            validation_bundle_sha256 = $5,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status IN ('draft', 'validated')
        "#,
    )
    .bind(input.draft_id.as_str())
    .bind(next_status.as_str())
    .bind(&input.message)
    .bind(&input.repository_path)
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .execute(&mut *tx)
    .await?;

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
        "#,
    )
    .bind(draft_id)
    .bind(status.as_str())
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
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
          AND status <> 'published'
        "#,
    )
    .bind(draft_id)
    .bind(message)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Mark inactive unpublished drafts abandoned after the configured TTL.
pub async fn abandon_stale_challenge_drafts(pool: &PgPool, ttl_days: i64) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'abandoned',
            validation_message = COALESCE(validation_message, 'draft expired due to inactivity'),
            updated_at = NOW()
        WHERE status IN ('draft', 'validated', 'approved', 'rejected')
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
) -> Result<Vec<ChallengePrivateAssetResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT a.*
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
        .map(row_to_private_asset_response)
        .collect()
}

/// Delete a private asset record after its object has been removed.
pub async fn delete_challenge_private_asset(pool: &PgPool, asset_row_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM challenge_private_assets WHERE id = $1::uuid")
        .bind(asset_row_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Mark a draft published and bind it to the published challenge row.
pub async fn mark_challenge_draft_published(
    pool: &PgPool,
    draft_id: &str,
    published_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_name = $2,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'approved'
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_name.map(ChallengeName::as_str))
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
    archive_challenge_tx(&mut tx, &input.challenge_name).await?;
    mark_challenge_draft_published_tx(&mut tx, input.draft_id.as_str(), None).await?;
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

async fn mark_challenge_draft_published_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    published_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_name = $2,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'approved'
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_name.map(ChallengeName::as_str))
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

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

async fn list_private_assets_for_draft(
    pool: &PgPool,
    draft_id: &str,
) -> Result<Vec<ChallengePrivateAssetResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM challenge_private_assets
        WHERE draft_id = $1::uuid
        ORDER BY created_at ASC
        "#,
    )
    .bind(draft_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(row_to_private_asset_response)
        .collect()
}

async fn list_validation_records_for_draft(
    pool: &PgPool,
    draft_id: &str,
) -> Result<Vec<ChallengeDraftValidationRecordResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM challenge_draft_validation_records
        WHERE draft_id = $1::uuid
        ORDER BY created_at DESC
        "#,
    )
    .bind(draft_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(row_to_validation_record_response)
        .collect()
}

fn row_to_draft_response(
    row: sqlx::postgres::PgRow,
    private_assets: Vec<ChallengePrivateAssetResponse>,
    validation_records: Vec<ChallengeDraftValidationRecordResponse>,
) -> Result<ChallengeDraftResponse> {
    let manifest_json: Value = row.try_get("manifest_json")?;
    let manifest: ChallengeCreationManifest =
        serde_json::from_value(manifest_json).map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ChallengeDraftResponse {
        id: challenge_draft_id_from_row(&row, "id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        request: request_kind_from_row(&row, "request_kind")?,
        status: draft_status_from_row(&row, "status")?,
        creator_agent_id: agent_id_from_row(&row, "creator_agent_id")?,
        creator_github_user_id: row.try_get("creator_github_user_id")?,
        creator_github_login: row.try_get("creator_github_login")?,
        repo_url: github_repo_remote_from_row(&row, "repo_url")?,
        pr_number: row.try_get("pr_number")?,
        pr_url: github_pull_request_url_from_row(&row, "pr_url")?,
        commit_sha: git_commit_sha_from_row(&row, "commit_sha")?,
        challenge_path: repo_relative_path_from_row(&row, "challenge_path")?,
        manifest_sha256: sha256_digest_from_row(&row, "manifest_sha256")?,
        manifest,
        validation_bundle_sha256: optional_sha256_digest_from_row(
            &row,
            "validation_bundle_sha256",
        )?,
        approved_bundle_sha256: optional_sha256_digest_from_row(&row, "approved_bundle_sha256")?,
        validation_message: row.try_get("validation_message")?,
        validation_repository_path: row.try_get("validation_repository_path")?,
        published_challenge_name: optional_challenge_name_from_row(
            &row,
            "published_challenge_name",
        )?,
        private_assets,
        validation_records,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

fn row_to_private_asset_response(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengePrivateAssetResponse> {
    Ok(ChallengePrivateAssetResponse {
        id: challenge_private_asset_id_from_row(&row, "id")?,
        draft_id: challenge_draft_id_from_row(&row, "draft_id")?,
        asset_name: asset_name_from_row(&row, "asset_name")?,
        kind: private_asset_kind_from_row(&row, "kind")?,
        required: row.try_get("required")?,
        size_bytes: row.try_get("size_bytes")?,
        sha256: sha256_digest_from_row(&row, "sha256")?,
        storage_key: storage_key_from_row(&row, "storage_key")?,
        uploader_agent_id: agent_id_from_row(&row, "uploader_agent_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn github_repo_remote_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<GithubRepoRemote> {
    let value: String = row.try_get(column)?;
    GithubRepoRemote::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn github_pull_request_url_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<GithubPullRequestUrl> {
    let value: String = row.try_get(column)?;
    GithubPullRequestUrl::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn git_commit_sha_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<GitCommitSha> {
    let value: String = row.try_get(column)?;
    GitCommitSha::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn sha256_digest_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<Sha256Digest> {
    let value: String = row.try_get(column)?;
    Sha256Digest::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn optional_sha256_digest_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<Sha256Digest>> {
    let Some(value) = row.try_get::<Option<String>, _>(column)? else {
        return Ok(None);
    };
    Sha256Digest::try_new(&value)
        .map(Some)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn storage_key_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn repo_relative_path_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<RepoRelativePath> {
    let value: String = row.try_get(column)?;
    RepoRelativePath::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

fn row_to_validation_record_response(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengeDraftValidationRecordResponse> {
    Ok(ChallengeDraftValidationRecordResponse {
        id: challenge_draft_validation_record_id_from_row(&row, "id")?,
        draft_id: challenge_draft_id_from_row(&row, "draft_id")?,
        status: validation_status_from_row(&row, "status")?,
        message: row.try_get("message")?,
        repository_path: row.try_get("repository_path")?,
        manifest_sha256: sha256_digest_from_row(&row, "manifest_sha256")?,
        bundle_sha256: optional_sha256_digest_from_row(&row, "bundle_sha256")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn request_kind_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeCreationRequestKind> {
    let value: String = row.try_get(column)?;
    ChallengeCreationRequestKind::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

fn draft_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeDraftStatus> {
    let value: String = row.try_get(column)?;
    ChallengeDraftStatus::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

fn validation_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeDraftValidationStatus> {
    let value: String = row.try_get(column)?;
    ChallengeDraftValidationStatus::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

fn private_asset_kind_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengePrivateAssetKind> {
    let value: String = row.try_get(column)?;
    ChallengePrivateAssetKind::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}
