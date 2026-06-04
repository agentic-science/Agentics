use sqlx::Row;
use uuid::Uuid;

use agentics_domain::models::ids::{
    AdminServiceTokenId, AgentId, AgentTokenId, ChallengePrivateAssetId, ChallengeReviewRecordId,
    ChallengeReviewValidationRecordId, ChallengeShortlistRevisionId, CreatorApiTokenId,
    EvaluationJobId, HumanId, PioneerCodeId, SolutionSubmissionId,
};
use agentics_domain::models::names::{AssetName, ChallengeName, TargetName};
use agentics_error::{Result, ServiceError};

/// Reads challenge name from a database row and validates its domain shape.
pub(in crate::db) fn challenge_name_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeName> {
    let raw: String = row.try_get(column)?;
    ChallengeName::try_new(raw).map_err(|e| {
        ServiceError::Internal(format!(
            "stored invalid challenge name in column `{column}`: {e}"
        ))
    })
}

/// Reads target from a database row and validates its domain shape.
pub(in crate::db) fn target_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<TargetName> {
    let raw: String = row.try_get(column)?;
    TargetName::try_new(raw).map_err(|e| {
        ServiceError::Internal(format!(
            "stored invalid target name in column `{column}`: {e}"
        ))
    })
}

/// Reads asset name from a database row and validates its domain shape.
pub(in crate::db) fn asset_name_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<AssetName> {
    let raw: String = row.try_get(column)?;
    AssetName::try_new(raw).map_err(|e| {
        ServiceError::Internal(format!(
            "stored invalid asset name in column `{column}`: {e}"
        ))
    })
}

/// Reads solution submission id from a database row and validates its domain shape.
pub(in crate::db) fn solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<SolutionSubmissionId> {
    let raw = uuid_or_string_from_row(row, column)?;
    SolutionSubmissionId::try_new(raw).map_err(|e| {
        ServiceError::Internal(format!(
            "stored invalid solution submission id in column `{column}`: {e}"
        ))
    })
}

/// Reads agent id from a database row and validates its domain shape.
pub(in crate::db) fn agent_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<AgentId> {
    parse_uuid_id_from_row(row, column, AgentId::try_new, "agent id")
}

/// Reads agent token id from a database row and validates its domain shape.
pub(in crate::db) fn agent_token_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<AgentTokenId> {
    parse_uuid_id_from_row(row, column, AgentTokenId::try_new, "agent token id")
}

/// Reads human id from a database row and validates its domain shape.
pub(in crate::db) fn human_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<HumanId> {
    parse_uuid_id_from_row(row, column, HumanId::try_new, "human id")
}

/// Reads admin service-token id from a database row and validates its domain shape.
pub(in crate::db) fn admin_service_token_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<AdminServiceTokenId> {
    parse_uuid_id_from_row(
        row,
        column,
        AdminServiceTokenId::try_new,
        "admin service-token id",
    )
}

/// Reads creator API-token id from a database row and validates its domain shape.
pub(in crate::db) fn creator_api_token_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<CreatorApiTokenId> {
    parse_uuid_id_from_row(
        row,
        column,
        CreatorApiTokenId::try_new,
        "creator API-token id",
    )
}

/// Reads pioneer-code id from a database row and validates its domain shape.
pub(in crate::db) fn pioneer_code_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<PioneerCodeId> {
    parse_uuid_id_from_row(row, column, PioneerCodeId::try_new, "pioneer-code id")
}

/// Reads challenge review record id from a database row and validates its domain shape.
pub(in crate::db) fn challenge_review_record_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeReviewRecordId> {
    parse_uuid_id_from_row(
        row,
        column,
        ChallengeReviewRecordId::try_new,
        "challenge review record id",
    )
}

/// Reads challenge private asset id from a database row and validates its domain shape.
pub(in crate::db) fn challenge_private_asset_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengePrivateAssetId> {
    parse_uuid_id_from_row(
        row,
        column,
        ChallengePrivateAssetId::try_new,
        "challenge private asset id",
    )
}

/// Reads challenge review record validation record id from a database row and validates its domain shape.
pub(in crate::db) fn challenge_review_validation_record_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeReviewValidationRecordId> {
    parse_uuid_id_from_row(
        row,
        column,
        ChallengeReviewValidationRecordId::try_new,
        "challenge review record validation record id",
    )
}

/// Reads challenge shortlist revision id from a database row and validates its domain shape.
pub(in crate::db) fn challenge_shortlist_revision_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeShortlistRevisionId> {
    parse_uuid_id_from_row(
        row,
        column,
        ChallengeShortlistRevisionId::try_new,
        "challenge shortlist revision id",
    )
}

/// Reads evaluation job id from a database row and validates its domain shape.
pub(in crate::db) fn evaluation_job_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<EvaluationJobId> {
    parse_uuid_id_from_row(row, column, EvaluationJobId::try_new, "evaluation job id")
}

/// Reads optional challenge name from a database row and validates its domain shape.
pub(in crate::db) fn optional_challenge_name_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<ChallengeName>> {
    row.try_get::<Option<String>, _>(column)?
        .map(ChallengeName::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!(
                "stored invalid challenge name in column `{column}`: {e}"
            ))
        })
}

/// Reads optional solution submission id from a database row and validates its domain shape.
pub(in crate::db) fn optional_solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<SolutionSubmissionId>> {
    optional_uuid_or_string_from_row(row, column)?
        .map(SolutionSubmissionId::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!(
                "stored invalid solution submission id in column `{column}`: {e}"
            ))
        })
}

/// Reads uuid string from a database row and validates its domain shape.
pub(in crate::db) fn uuid_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<String> {
    uuid_or_string_from_row(row, column)
}

/// Reads optional uuid string from a database row and validates its domain shape.
pub(in crate::db) fn optional_uuid_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>> {
    optional_uuid_or_string_from_row(row, column)
}

/// Reads uuid or string from a database row and validates its domain shape.
fn uuid_or_string_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<String> {
    if let Ok(value) = row.try_get::<Uuid, _>(column) {
        return Ok(value.to_string());
    }
    Ok(row.try_get(column)?)
}

/// Reads parse uuid id from a database row and validates its domain shape.
fn parse_uuid_id_from_row<T>(
    row: &sqlx::postgres::PgRow,
    column: &str,
    parser: impl FnOnce(String) -> std::result::Result<T, agentics_domain::models::ids::UuidIdError>,
    label: &str,
) -> Result<T> {
    let raw = uuid_or_string_from_row(row, column)?;
    parser(raw).map_err(|e| {
        ServiceError::Internal(format!("stored invalid {label} in column `{column}`: {e}"))
    })
}

/// Reads optional uuid or string from a database row and validates its domain shape.
fn optional_uuid_or_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>> {
    if let Ok(value) = row.try_get::<Option<Uuid>, _>(column) {
        return Ok(value.map(|value| value.to_string()));
    }
    Ok(row.try_get(column)?)
}
