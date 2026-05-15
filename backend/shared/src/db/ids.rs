use sqlx::Row;

use crate::error::{AppError, Result};
use crate::models::ids::{ChallengeId, SolutionSubmissionId, TargetName};

pub(in crate::db) fn challenge_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<ChallengeId> {
    let raw: String = row.try_get(column)?;
    ChallengeId::try_new(raw).map_err(|e| {
        AppError::Internal(format!(
            "stored invalid challenge id in column `{column}`: {e}"
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

pub(in crate::db) fn solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<SolutionSubmissionId> {
    let raw: String = row.try_get(column)?;
    SolutionSubmissionId::try_new(raw).map_err(|e| {
        AppError::Internal(format!(
            "stored invalid solution submission id in column `{column}`: {e}"
        ))
    })
}

pub(in crate::db) fn optional_challenge_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<ChallengeId>> {
    row.try_get::<Option<String>, _>(column)?
        .map(ChallengeId::try_new)
        .transpose()
        .map_err(|e| {
            AppError::Internal(format!(
                "stored invalid challenge id in column `{column}`: {e}"
            ))
        })
}

pub(in crate::db) fn optional_solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<SolutionSubmissionId>> {
    row.try_get::<Option<String>, _>(column)?
        .map(SolutionSubmissionId::try_new)
        .transpose()
        .map_err(|e| {
            AppError::Internal(format!(
                "stored invalid solution submission id in column `{column}`: {e}"
            ))
        })
}
