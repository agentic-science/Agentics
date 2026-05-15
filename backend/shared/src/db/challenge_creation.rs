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
use crate::models::names::{AssetName, ChallengeName};

use super::challenges::{add_challenge_owner_tx, publish_challenge_tx};
use super::ids::{
    asset_name_from_row, challenge_name_from_row, optional_challenge_name_from_row,
    uuid_string_from_row,
};

/// Input for inserting one GitHub PR-backed challenge draft.
#[derive(Debug, Clone)]
pub struct CreateChallengeDraftInput {
    pub draft_id: String,
    pub creator_agent_id: String,
    pub creator_github_user_id: i64,
    pub creator_github_login: String,
    pub repo_url: String,
    pub pr_number: i32,
    pub pr_url: String,
    pub commit_sha: String,
    pub challenge_path: String,
    pub manifest_sha256: String,
    pub manifest: ChallengeCreationManifest,
}

/// Input for persisting one private benchmark asset.
#[derive(Debug, Clone)]
pub struct CreateChallengePrivateAssetInput {
    pub asset_row_id: String,
    pub draft_id: String,
    pub asset_name: AssetName,
    pub kind: ChallengePrivateAssetKind,
    pub required: bool,
    pub size_bytes: i64,
    pub sha256: String,
    pub storage_uri: String,
    pub uploader_agent_id: String,
}

/// Input for appending a draft audit event.
#[derive(Debug, Clone)]
pub struct CreateChallengeDraftAuditEventInput {
    pub event_id: String,
    pub draft_id: String,
    pub actor_agent_id: Option<String>,
    pub actor_admin_username: Option<String>,
    pub action: String,
    pub message: String,
    pub metadata: Value,
}

/// Input for atomically publishing one approved new-challenge draft.
#[derive(Debug, Clone)]
pub struct PublishNewChallengeDraftInput {
    pub draft_id: String,
    pub challenge_name: ChallengeName,
    pub bundle_path: String,
    pub statement_path: String,
    pub spec: ChallengeBundleSpec,
    pub title: String,
    pub summary: String,
    pub owner_agent_id: String,
    pub audit_event_id: String,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: String,
}

/// Input for atomically publishing one approved archive draft.
#[derive(Debug, Clone)]
pub struct PublishArchiveChallengeDraftInput {
    pub draft_id: String,
    pub challenge_name: ChallengeName,
    pub audit_event_id: String,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: String,
}

/// Input for recording one admin validation attempt against a draft.
#[derive(Debug, Clone)]
pub struct RecordChallengeDraftValidationInput {
    pub validation_record_id: String,
    pub draft_id: String,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub repository_path: String,
    pub manifest_sha256: String,
    pub bundle_sha256: Option<String>,
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
            pr_number,
            pr_url,
            commit_sha,
            challenge_path,
            manifest_sha256,
            manifest_json
        )
        VALUES ($1::uuid, $2, $3, 'draft', $4::uuid, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING *
        "#,
    )
    .bind(&input.draft_id)
    .bind(input.manifest.challenge_name.as_str())
    .bind(input.manifest.request.as_str())
    .bind(&input.creator_agent_id)
    .bind(input.creator_github_user_id)
    .bind(&input.creator_github_login)
    .bind(&input.repo_url)
    .bind(input.pr_number)
    .bind(&input.pr_url)
    .bind(&input.commit_sha)
    .bind(&input.challenge_path)
    .bind(&input.manifest_sha256)
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
        let draft_id = uuid_string_from_row(&row, "id")?;
        let assets = list_private_assets_for_draft(pool, &draft_id).await?;
        let validation_records = list_validation_records_for_draft(pool, &draft_id).await?;
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

    let existing_bytes = sum_private_asset_bytes_for_draft_tx(&mut tx, &input.draft_id).await?;
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
            storage_uri,
            uploader_agent_id
        )
        VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9::uuid)
        RETURNING *
        "#,
    )
    .bind(&input.asset_row_id)
    .bind(&input.draft_id)
    .bind(input.asset_name.as_str())
    .bind(input.kind.as_str())
    .bind(input.required)
    .bind(input.size_bytes)
    .bind(&input.sha256)
    .bind(&input.storage_uri)
    .bind(&input.uploader_agent_id)
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
    .bind(&input.validation_record_id)
    .bind(&input.draft_id)
    .bind(input.status.as_str())
    .bind(&input.message)
    .bind(&input.repository_path)
    .bind(&input.manifest_sha256)
    .bind(&input.bundle_sha256)
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
    .bind(&input.draft_id)
    .bind(next_status.as_str())
    .bind(&input.message)
    .bind(&input.repository_path)
    .bind(&input.bundle_sha256)
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
    mark_challenge_draft_published_tx(&mut tx, &input.draft_id, Some(&published.challenge_name))
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
                "bundle_sha256": &input.bundle_sha256
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
    mark_challenge_draft_published_tx(&mut tx, &input.draft_id, None).await?;
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
                "bundle_sha256": &input.bundle_sha256
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
    .bind(&input.event_id)
    .bind(&input.draft_id)
    .bind(&input.actor_agent_id)
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
        id: uuid_string_from_row(&row, "id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        request: parse_request_kind(&row.try_get::<String, _>("request_kind")?)?,
        status: parse_draft_status(&row.try_get::<String, _>("status")?)?,
        creator_agent_id: uuid_string_from_row(&row, "creator_agent_id")?,
        creator_github_user_id: row.try_get("creator_github_user_id")?,
        creator_github_login: row.try_get("creator_github_login")?,
        repo_url: row.try_get("repo_url")?,
        pr_number: row.try_get("pr_number")?,
        pr_url: row.try_get("pr_url")?,
        commit_sha: row.try_get("commit_sha")?,
        challenge_path: row.try_get("challenge_path")?,
        manifest_sha256: row.try_get("manifest_sha256")?,
        manifest,
        validation_bundle_sha256: row.try_get("validation_bundle_sha256")?,
        approved_bundle_sha256: row.try_get("approved_bundle_sha256")?,
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
        id: uuid_string_from_row(&row, "id")?,
        draft_id: uuid_string_from_row(&row, "draft_id")?,
        asset_name: asset_name_from_row(&row, "asset_name")?,
        kind: parse_private_asset_kind(&row.try_get::<String, _>("kind")?)?,
        required: row.try_get("required")?,
        size_bytes: row.try_get("size_bytes")?,
        sha256: row.try_get("sha256")?,
        storage_uri: row.try_get("storage_uri")?,
        uploader_agent_id: uuid_string_from_row(&row, "uploader_agent_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn row_to_validation_record_response(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengeDraftValidationRecordResponse> {
    Ok(ChallengeDraftValidationRecordResponse {
        id: uuid_string_from_row(&row, "id")?,
        draft_id: uuid_string_from_row(&row, "draft_id")?,
        status: parse_validation_status(&row.try_get::<String, _>("status")?)?,
        message: row.try_get("message")?,
        repository_path: row.try_get("repository_path")?,
        manifest_sha256: row.try_get("manifest_sha256")?,
        bundle_sha256: row.try_get("bundle_sha256")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn parse_request_kind(value: &str) -> Result<ChallengeCreationRequestKind> {
    match value {
        "new_challenge" => Ok(ChallengeCreationRequestKind::NewChallenge),
        "archive_challenge" => Ok(ChallengeCreationRequestKind::ArchiveChallenge),
        _ => Err(AppError::Internal(format!(
            "unknown challenge draft request kind `{value}`"
        ))),
    }
}

fn parse_draft_status(value: &str) -> Result<ChallengeDraftStatus> {
    match value {
        "draft" => Ok(ChallengeDraftStatus::Draft),
        "validated" => Ok(ChallengeDraftStatus::Validated),
        "approved" => Ok(ChallengeDraftStatus::Approved),
        "rejected" => Ok(ChallengeDraftStatus::Rejected),
        "published" => Ok(ChallengeDraftStatus::Published),
        "abandoned" => Ok(ChallengeDraftStatus::Abandoned),
        _ => Err(AppError::Internal(format!(
            "unknown challenge draft status `{value}`"
        ))),
    }
}

fn parse_validation_status(value: &str) -> Result<ChallengeDraftValidationStatus> {
    match value {
        "passed" => Ok(ChallengeDraftValidationStatus::Passed),
        "failed" => Ok(ChallengeDraftValidationStatus::Failed),
        _ => Err(AppError::Internal(format!(
            "unknown challenge draft validation status `{value}`"
        ))),
    }
}

fn parse_private_asset_kind(value: &str) -> Result<ChallengePrivateAssetKind> {
    match value {
        "private_benchmark_data" => Ok(ChallengePrivateAssetKind::PrivateBenchmarkData),
        "private_scorer_package" => Ok(ChallengePrivateAssetKind::PrivateScorerPackage),
        "private_seeds" => Ok(ChallengePrivateAssetKind::PrivateSeeds),
        "private_reference_outputs" => Ok(ChallengePrivateAssetKind::PrivateReferenceOutputs),
        _ => Err(AppError::Internal(format!(
            "unknown private asset kind `{value}`"
        ))),
    }
}

impl ChallengeCreationRequestKind {
    /// Stable database string for this creation request.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewChallenge => "new_challenge",
            Self::ArchiveChallenge => "archive_challenge",
        }
    }
}

impl ChallengePrivateAssetKind {
    /// Stable database string for this private asset kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateBenchmarkData => "private_benchmark_data",
            Self::PrivateScorerPackage => "private_scorer_package",
            Self::PrivateSeeds => "private_seeds",
            Self::PrivateReferenceOutputs => "private_reference_outputs",
        }
    }
}
