use serde_json::Value;
use sqlx::Row;

use agentics_domain::models::challenge::ChallengeLifecycleStatus;
use agentics_domain::models::evaluation::SolutionSubmissionStatus;
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::localization::LocalizedText;
use agentics_domain::models::urls::MoltbookPostUrl;
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};

/// Reads localized text from a JSONB database column.
pub(in crate::db) fn localized_text_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<LocalizedText> {
    let value: Value = row.try_get(column)?;
    serde_json::from_value(value)
        .map_err(|e| ServiceError::Internal(format!("stored {column} is invalid: {e}")))
}

/// Serialize localized text for JSONB binding.
pub(super) fn localized_text_to_json(value: &LocalizedText) -> Result<Value> {
    serde_json::to_value(value).map_err(|e| ServiceError::Internal(e.to_string()))
}

/// Read an optional Moltbook post URL from a database row.
pub(super) fn optional_moltbook_post_url_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<MoltbookPostUrl>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(MoltbookPostUrl::try_new)
        .transpose()
        .map_err(|e| ServiceError::Internal(format!("stored invalid {column}: {e}")))
}

/// Reads storage key from a database row and validates its domain shape.
pub(super) fn storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value)
        .map_err(|e| ServiceError::Internal(format!("invalid stored {column}: {e}")))
}

/// Reads a challenge lifecycle status and validates its stored value.
pub(super) fn challenge_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeLifecycleStatus> {
    let value: String = row.try_get(column)?;
    ChallengeLifecycleStatus::from_storage_value(&value)
        .ok_or_else(|| ServiceError::Internal(format!("unexpected challenge status `{value}`")))
}

/// Reads an optional solution-submission status for creator participant rows.
pub(super) fn optional_solution_submission_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<SolutionSubmissionStatus>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(|value| {
            SolutionSubmissionStatus::from_storage_value(&value).ok_or_else(|| {
                ServiceError::Internal(format!("unexpected solution submission status `{value}`"))
            })
        })
        .transpose()
}

/// Reads sha256 digest from a database row and validates its domain shape.
pub(super) fn sha256_digest_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Sha256Digest> {
    let value: String = row.try_get(column)?;
    Sha256Digest::try_new(&value)
        .map_err(|e| ServiceError::Internal(format!("invalid stored {column}: {e}")))
}
