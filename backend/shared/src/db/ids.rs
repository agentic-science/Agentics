use sqlx::Row;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::ids::SolutionSubmissionId;
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

fn optional_uuid_or_string_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<String>> {
    if let Ok(value) = row.try_get::<Option<Uuid>, _>(column) {
        return Ok(value.map(|value| value.to_string()));
    }
    Ok(row.try_get(column)?)
}
