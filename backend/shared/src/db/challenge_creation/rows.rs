use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};
use crate::models::challenge_creation::{
    AdminChallengePrivateAssetResponse, ChallengeCreationManifest, ChallengeCreationRequestKind,
    ChallengeDraftResponse, ChallengeDraftStatus, ChallengeDraftValidationRecordResponse,
    ChallengeDraftValidationStatus, ChallengePrivateAssetKind, ChallengePrivateAssetResponse,
    ChallengePrivateAssetStatus,
};
use crate::models::github::GithubPullRequestNumber;
use crate::models::hashes::{GitCommitSha, Sha256Digest};
use crate::models::paths::RepoRelativePath;
use crate::models::urls::{GithubPullRequestUrl, GithubRepoRemote};
use crate::storage::StorageKey;

use super::super::ids::{
    agent_id_from_row, asset_name_from_row, challenge_draft_id_from_row,
    challenge_draft_validation_record_id_from_row, challenge_name_from_row,
    challenge_private_asset_id_from_row, optional_challenge_id_from_row,
};

/// List all private asset lifecycle records for an admin draft review.
pub async fn list_challenge_private_asset_states(
    pool: &PgPool,
    draft_id: &str,
) -> Result<Vec<AdminChallengePrivateAssetResponse>> {
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
        .map(row_to_admin_private_asset_response)
        .collect()
}

/// Lists active private assets for draft using the configured query scope.
pub(super) async fn list_private_assets_for_draft(
    pool: &PgPool,
    draft_id: &str,
) -> Result<Vec<ChallengePrivateAssetResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT *
        FROM challenge_private_assets
        WHERE draft_id = $1::uuid
          AND status = 'active'
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

/// Lists validation records for draft using the configured query scope.
pub(super) async fn list_validation_records_for_draft(
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

/// Converts a database row into the draft response model.
pub(super) fn row_to_draft_response(
    row: sqlx::postgres::PgRow,
    private_assets: Vec<ChallengePrivateAssetResponse>,
    validation_records: Vec<ChallengeDraftValidationRecordResponse>,
) -> Result<ChallengeDraftResponse> {
    let manifest_json: Value = row.try_get("manifest_json")?;
    let manifest: ChallengeCreationManifest =
        serde_json::from_value(manifest_json).map_err(|e| AppError::Internal(e.to_string()))?;
    let published_challenge_id = optional_challenge_id_from_row(&row, "published_challenge_id")?;
    let published_challenge_name = if published_challenge_id.is_some() {
        Some(challenge_name_from_row(&row, "challenge_name")?)
    } else {
        None
    };

    Ok(ChallengeDraftResponse {
        id: challenge_draft_id_from_row(&row, "id")?,
        challenge_name: challenge_name_from_row(&row, "challenge_name")?,
        request: request_kind_from_row(&row, "request_kind")?,
        status: draft_status_from_row(&row, "status")?,
        creator_agent_id: agent_id_from_row(&row, "creator_agent_id")?,
        creator_github_user_id: row.try_get("creator_github_user_id")?,
        creator_github_login: row.try_get("creator_github_login")?,
        repo_url: github_repo_remote_from_row(&row, "repo_url")?,
        pr_number: github_pull_request_number_from_row(&row, "pr_number")?,
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
        published_challenge_id,
        published_challenge_name,
        private_assets,
        validation_records,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
    })
}

/// Converts a database row into the private asset response model.
pub(super) fn row_to_private_asset_response(
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

/// Converts a database row into the admin private asset lifecycle response model.
fn row_to_admin_private_asset_response(
    row: sqlx::postgres::PgRow,
) -> Result<AdminChallengePrivateAssetResponse> {
    let activated_at = row
        .try_get::<Option<DateTime<Utc>>, _>("activated_at")?
        .map(|value| value.to_rfc3339());
    let failed_at = row
        .try_get::<Option<DateTime<Utc>>, _>("failed_at")?
        .map(|value| value.to_rfc3339());
    Ok(AdminChallengePrivateAssetResponse {
        id: challenge_private_asset_id_from_row(&row, "id")?,
        draft_id: challenge_draft_id_from_row(&row, "draft_id")?,
        asset_name: asset_name_from_row(&row, "asset_name")?,
        kind: private_asset_kind_from_row(&row, "kind")?,
        required: row.try_get("required")?,
        status: private_asset_status_from_row(&row, "status")?,
        size_bytes: row.try_get("size_bytes")?,
        sha256: sha256_digest_from_row(&row, "sha256")?,
        storage_key: storage_key_from_row(&row, "storage_key")?,
        temporary_storage_key: optional_storage_key_from_row(&row, "temporary_storage_key")?,
        uploader_agent_id: agent_id_from_row(&row, "uploader_agent_id")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        activated_at,
        failed_at,
        failure_message: row.try_get("failure_message")?,
    })
}

/// Reads github repo remote from a database row and validates its domain shape.
fn github_repo_remote_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<GithubRepoRemote> {
    let value: String = row.try_get(column)?;
    GithubRepoRemote::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads github pull request url from a database row and validates its domain shape.
fn github_pull_request_url_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<GithubPullRequestUrl> {
    let value: String = row.try_get(column)?;
    GithubPullRequestUrl::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads github pull request number from a database row and validates its domain shape.
fn github_pull_request_number_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<GithubPullRequestNumber> {
    let value: i32 = row.try_get(column)?;
    GithubPullRequestNumber::try_new(value.to_string())
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads git commit sha from a database row and validates its domain shape.
fn git_commit_sha_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<GitCommitSha> {
    let value: String = row.try_get(column)?;
    GitCommitSha::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads sha256 digest from a database row and validates its domain shape.
fn sha256_digest_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<Sha256Digest> {
    let value: String = row.try_get(column)?;
    Sha256Digest::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads optional sha256 digest from a database row and validates its domain shape.
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

/// Reads storage key from a database row and validates its domain shape.
pub(super) fn storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads an optional storage key from a database row and validates its domain shape.
pub(super) fn optional_storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<StorageKey>> {
    let Some(value) = row.try_get::<Option<String>, _>(column)? else {
        return Ok(None);
    };
    StorageKey::try_new(&value)
        .map(Some)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads repo relative path from a database row and validates its domain shape.
fn repo_relative_path_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<RepoRelativePath> {
    let value: String = row.try_get(column)?;
    RepoRelativePath::try_new(&value)
        .map_err(|e| AppError::Internal(format!("invalid stored {column}: {e}")))
}

/// Converts a database row into the validation record response model.
pub(super) fn row_to_validation_record_response(
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

/// Reads request kind from a database row and validates its domain shape.
fn request_kind_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeCreationRequestKind> {
    let value: String = row.try_get(column)?;
    ChallengeCreationRequestKind::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

/// Reads draft status from a database row and validates its domain shape.
pub(super) fn draft_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeDraftStatus> {
    let value: String = row.try_get(column)?;
    ChallengeDraftStatus::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

/// Reads validation status from a database row and validates its domain shape.
fn validation_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeDraftValidationStatus> {
    let value: String = row.try_get(column)?;
    ChallengeDraftValidationStatus::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

/// Reads private asset status from a database row and validates its domain shape.
fn private_asset_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengePrivateAssetStatus> {
    let value: String = row.try_get(column)?;
    ChallengePrivateAssetStatus::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}

/// Reads private asset kind from a database row and validates its domain shape.
fn private_asset_kind_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengePrivateAssetKind> {
    let value: String = row.try_get(column)?;
    ChallengePrivateAssetKind::from_storage_value(&value)
        .ok_or_else(|| AppError::Internal(format!("unknown stored {column} `{value}`")))
}
