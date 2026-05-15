use sqlx::Row;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::ids::{
    AgentId, ChallengeDraftId, ChallengeDraftValidationRecordId, ChallengePrivateAssetId,
    ChallengeShortlistRevisionId, EvaluationJobId, SolutionSubmissionId,
};
use crate::models::names::{AssetName, ChallengeName, TargetName};

pub(in crate::db) fn challenge_name_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeName> {
    let raw: String = row.try_get(column)?;
    ChallengeName::try_new(raw).map_err(|e| {
        AppError::Internal(format!(
            "stored invalid challenge name in column `{column}`: {e}"
        ))
    })
}

pub(in crate::db) fn target_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<TargetName> {
    let raw: String = row.try_get(column)?;
    TargetName::try_new(raw).map_err(|e| {
        AppError::Internal(format!(
            "stored invalid target name in column `{column}`: {e}"
        ))
    })
}

pub(in crate::db) fn asset_name_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<AssetName> {
    let raw: String = row.try_get(column)?;
    AssetName::try_new(raw).map_err(|e| {
        AppError::Internal(format!(
            "stored invalid asset name in column `{column}`: {e}"
        ))
    })
}

pub(in crate::db) fn solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<SolutionSubmissionId> {
    let raw = uuid_or_string_from_row(row, column)?;
    SolutionSubmissionId::try_new(raw).map_err(|e| {
        AppError::Internal(format!(
            "stored invalid solution submission id in column `{column}`: {e}"
        ))
    })
}

pub(in crate::db) fn agent_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<AgentId> {
    parse_uuid_id_from_row(row, column, AgentId::try_new, "agent id")
}

pub(in crate::db) fn challenge_draft_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeDraftId> {
    parse_uuid_id_from_row(row, column, ChallengeDraftId::try_new, "challenge draft id")
}

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

pub(in crate::db) fn challenge_draft_validation_record_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeDraftValidationRecordId> {
    parse_uuid_id_from_row(
        row,
        column,
        ChallengeDraftValidationRecordId::try_new,
        "challenge draft validation record id",
    )
}

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

pub(in crate::db) fn evaluation_job_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<EvaluationJobId> {
    parse_uuid_id_from_row(row, column, EvaluationJobId::try_new, "evaluation job id")
}

pub(in crate::db) fn optional_challenge_name_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<ChallengeName>> {
    row.try_get::<Option<String>, _>(column)?
        .map(ChallengeName::try_new)
        .transpose()
        .map_err(|e| {
            AppError::Internal(format!(
                "stored invalid challenge name in column `{column}`: {e}"
            ))
        })
}

pub(in crate::db) fn optional_solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<SolutionSubmissionId>> {
    optional_uuid_or_string_from_row(row, column)?
        .map(SolutionSubmissionId::try_new)
        .transpose()
        .map_err(|e| {
            AppError::Internal(format!(
                "stored invalid solution submission id in column `{column}`: {e}"
            ))
        })
}

pub(in crate::db) fn uuid_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<String> {
    uuid_or_string_from_row(row, column)
}

pub(in crate::db) fn optional_uuid_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>> {
    optional_uuid_or_string_from_row(row, column)
}

fn uuid_or_string_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<String> {
    if let Ok(value) = row.try_get::<Uuid, _>(column) {
        return Ok(value.to_string());
    }
    Ok(row.try_get(column)?)
}

fn parse_uuid_id_from_row<T>(
    row: &sqlx::postgres::PgRow,
    column: &str,
    parser: impl FnOnce(String) -> std::result::Result<T, crate::models::ids::UuidIdError>,
    label: &str,
) -> Result<T> {
    let raw = uuid_or_string_from_row(row, column)?;
    parser(raw).map_err(|e| {
        AppError::Internal(format!("stored invalid {label} in column `{column}`: {e}"))
    })
}

fn optional_uuid_or_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>> {
    if let Ok(value) = row.try_get::<Option<Uuid>, _>(column) {
        return Ok(value.map(|value| value.to_string()));
    }
    Ok(row.try_get(column)?)
}
