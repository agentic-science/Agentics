//! Challenge draft, GitHub identity, private asset, and review lifecycle queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeCreationRequestKind, ChallengeDraftResponse,
    ChallengeDraftStatus, ChallengeDraftValidationRecordResponse, ChallengeDraftValidationStatus,
    ChallengePrivateAssetKind, ChallengePrivateAssetResponse, GithubIdentityResponse,
};

/// Input for linking an agent to a manually verified GitHub account.
#[derive(Debug, Clone)]
pub struct LinkGithubIdentityInput {
    pub agent_id: String,
    pub github_user_id: i64,
    pub github_login: String,
}

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
    pub asset_id_row: String,
    pub draft_id: String,
    pub asset_id: String,
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

/// Persist a GitHub identity link on an existing agent.
pub async fn link_agent_github_identity(
    pool: &PgPool,
    input: &LinkGithubIdentityInput,
) -> Result<GithubIdentityResponse> {
    let row = sqlx::query(
        r#"
        UPDATE agents
        SET github_user_id = $2,
            github_login = $3
        WHERE id = $1
        RETURNING id, github_user_id, github_login
        "#,
    )
    .bind(&input.agent_id)
    .bind(input.github_user_id)
    .bind(input.github_login.trim())
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(AppError::NotFound)?;
    Ok(GithubIdentityResponse {
        agent_id: row.try_get("id")?,
        github_user_id: row.try_get("github_user_id")?,
        github_login: row.try_get("github_login")?,
    })
}

/// Fetch a linked GitHub identity for one agent.
pub async fn get_agent_github_identity(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Option<GithubIdentityResponse>> {
    let row = sqlx::query(
        r#"
        SELECT id, github_user_id, github_login
        FROM agents
        WHERE id = $1
          AND github_user_id IS NOT NULL
        "#,
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(GithubIdentityResponse {
            agent_id: row.try_get("id")?,
            github_user_id: row.try_get("github_user_id")?,
            github_login: row
                .try_get::<Option<String>, _>("github_login")?
                .unwrap_or_default(),
        })
    })
    .transpose()
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
            challenge_id,
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
        VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING *
        "#,
    )
    .bind(&input.draft_id)
    .bind(&input.manifest.challenge_id)
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
    let row = sqlx::query("SELECT * FROM challenge_drafts WHERE id = $1")
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
        let draft_id: String = row.try_get("id")?;
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
        WHERE creator_agent_id = $1
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
        WHERE draft_id = $1
          AND created_at >= NOW() - ($2::TEXT || ' seconds')::INTERVAL
        "#,
    )
    .bind(draft_id)
    .bind(window_seconds)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Sum private asset bytes already attached to a draft.
pub async fn sum_private_asset_bytes_for_draft(pool: &PgPool, draft_id: &str) -> Result<i64> {
    let bytes = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(SUM(size_bytes), 0)::BIGINT
        FROM challenge_private_assets
        WHERE draft_id = $1
        "#,
    )
    .bind(draft_id)
    .fetch_one(pool)
    .await?;

    Ok(bytes)
}

/// Insert a private benchmark asset record for a draft.
pub async fn create_challenge_private_asset(
    pool: &PgPool,
    input: &CreateChallengePrivateAssetInput,
) -> Result<ChallengePrivateAssetResponse> {
    let row = sqlx::query(
        r#"
        INSERT INTO challenge_private_assets (
            id,
            draft_id,
            asset_id,
            kind,
            required,
            size_bytes,
            sha256,
            storage_uri,
            uploader_agent_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
    .bind(&input.asset_id_row)
    .bind(&input.draft_id)
    .bind(&input.asset_id)
    .bind(input.kind.as_str())
    .bind(input.required)
    .bind(input.size_bytes)
    .bind(&input.sha256)
    .bind(&input.storage_uri)
    .bind(&input.uploader_agent_id)
    .fetch_one(pool)
    .await?;

    row_to_private_asset_response(row)
}

/// Record a validation outcome and move draft status accordingly.
pub async fn record_challenge_draft_validation(
    pool: &PgPool,
    validation_record_id: &str,
    draft_id: &str,
    status: ChallengeDraftValidationStatus,
    message: &str,
    repository_path: &str,
    manifest_sha256: &str,
) -> Result<ChallengeDraftValidationRecordResponse> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO challenge_draft_validation_records (
            id, draft_id, status, message, repository_path, manifest_sha256
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(validation_record_id)
    .bind(draft_id)
    .bind(status.as_str())
    .bind(message)
    .bind(repository_path)
    .bind(manifest_sha256)
    .fetch_one(&mut *tx)
    .await?;

    let next_status = match status {
        ChallengeDraftValidationStatus::Passed => ChallengeDraftStatus::Validated,
        ChallengeDraftValidationStatus::Failed => ChallengeDraftStatus::Draft,
    };
    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = $2,
            validation_message = $3,
            validation_repository_path = $4,
            updated_at = NOW()
        WHERE id = $1
          AND status IN ('draft', 'validated', 'approved')
        "#,
    )
    .bind(draft_id)
    .bind(next_status.as_str())
    .bind(message)
    .bind(repository_path)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    row_to_validation_record_response(row)
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
        WHERE id = $1
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
        WHERE id = $1
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

    Ok(result.rows_affected() as i64)
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
    sqlx::query("DELETE FROM challenge_private_assets WHERE id = $1")
        .bind(asset_row_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Mark a draft published and bind it to the immutable challenge version row.
pub async fn mark_challenge_draft_published(
    pool: &PgPool,
    draft_id: &str,
    published_challenge_version_id: Option<&str>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_version_id = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_version_id)
    .execute(pool)
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
    sqlx::query(
        r#"
        INSERT INTO challenge_draft_audit_events (
            id, draft_id, actor_agent_id, actor_admin_username, action, message, metadata_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(&input.event_id)
    .bind(&input.draft_id)
    .bind(&input.actor_agent_id)
    .bind(&input.actor_admin_username)
    .bind(&input.action)
    .bind(&input.message)
    .bind(&input.metadata)
    .execute(pool)
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
        WHERE draft_id = $1
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
        WHERE draft_id = $1
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
        id: row.try_get("id")?,
        challenge_id: row.try_get("challenge_id")?,
        request: parse_request_kind(&row.try_get::<String, _>("request_kind")?)?,
        status: parse_draft_status(&row.try_get::<String, _>("status")?)?,
        creator_agent_id: row.try_get("creator_agent_id")?,
        creator_github_user_id: row.try_get("creator_github_user_id")?,
        creator_github_login: row.try_get("creator_github_login")?,
        repo_url: row.try_get("repo_url")?,
        pr_number: row.try_get("pr_number")?,
        pr_url: row.try_get("pr_url")?,
        commit_sha: row.try_get("commit_sha")?,
        challenge_path: row.try_get("challenge_path")?,
        manifest_sha256: row.try_get("manifest_sha256")?,
        manifest,
        validation_message: row.try_get("validation_message")?,
        validation_repository_path: row.try_get("validation_repository_path")?,
        published_challenge_version_id: row.try_get("published_challenge_version_id")?,
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
        id: row.try_get("id")?,
        draft_id: row.try_get("draft_id")?,
        asset_id: row.try_get("asset_id")?,
        kind: parse_private_asset_kind(&row.try_get::<String, _>("kind")?)?,
        required: row.try_get("required")?,
        size_bytes: row.try_get("size_bytes")?,
        sha256: row.try_get("sha256")?,
        storage_uri: row.try_get("storage_uri")?,
        uploader_agent_id: row.try_get("uploader_agent_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn row_to_validation_record_response(
    row: sqlx::postgres::PgRow,
) -> Result<ChallengeDraftValidationRecordResponse> {
    Ok(ChallengeDraftValidationRecordResponse {
        id: row.try_get("id")?,
        draft_id: row.try_get("draft_id")?,
        status: parse_validation_status(&row.try_get::<String, _>("status")?)?,
        message: row.try_get("message")?,
        repository_path: row.try_get("repository_path")?,
        manifest_sha256: row.try_get("manifest_sha256")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
    })
}

fn parse_request_kind(value: &str) -> Result<ChallengeCreationRequestKind> {
    match value {
        "new_challenge" => Ok(ChallengeCreationRequestKind::NewChallenge),
        "new_version" => Ok(ChallengeCreationRequestKind::NewVersion),
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
            Self::NewVersion => "new_version",
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
