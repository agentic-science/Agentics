use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;

use agentics_domain::models::evaluation::{
    EvaluationDto, EvaluationJobStatus, EvaluationStatus, MetricValue, PublicCaseResult,
    RunMetricResult, ScoringMode, SolutionArtifactMetadata, SolutionSubmissionStatus,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{EvaluationId, EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::names::TargetName;
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};

use super::super::ids::optional_uuid_string_from_row;
use super::super::json::decode_optional_json;

pub(super) fn count_to_u64(label: &str, value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| ServiceError::Internal(format!("{label} count was negative")))
}

/// Reads parse eval from a database row and validates its domain shape.
pub(super) fn parse_eval_from_row(
    row: &sqlx::postgres::PgRow,
    prefix: &str,
) -> Result<Option<EvaluationDto>> {
    let id_col = format!("{}_id", prefix);
    let id = optional_evaluation_id_from_row(row, id_col.as_str())?;
    let id = match id {
        Some(i) => i,
        _ => return Ok(None),
    };
    let status_str: String = row.try_get(format!("{}_status", prefix).as_str())?;
    let target_col = format!("{}_target", prefix);
    let target = target_from_row(row, target_col.as_str())?;
    let eval_type_str: String = row.try_get(format!("{}_eval_type", prefix).as_str())?;
    let aggregate_json: Option<Value> =
        row.try_get(format!("{}_aggregate_metrics", prefix).as_str())?;
    let run_metrics_json: Option<Value> =
        row.try_get(format!("{}_run_metrics", prefix).as_str())?;
    let public_results_json: Option<Value> =
        row.try_get(format!("{}_public_results", prefix).as_str())?;
    let validation_summary_json: Option<Value> =
        row.try_get(format!("{}_validation_summary", prefix).as_str())?;
    let official_json: Option<Value> =
        row.try_get(format!("{}_official_summary", prefix).as_str())?;
    let runner_log_storage_key =
        optional_storage_key_from_row(row, format!("{prefix}_runner_log_storage_key").as_str())?;
    let started_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_started_at", prefix).as_str())?;
    let finished_at: Option<DateTime<Utc>> =
        row.try_get(format!("{}_finished_at", prefix).as_str())?;

    let status = EvaluationStatus::from_storage_value(&status_str).ok_or_else(|| {
        ServiceError::Internal(format!("unexpected evaluation status `{status_str}`"))
    })?;
    let eval_type = ScoringMode::from_storage_value(&eval_type_str).ok_or_else(|| {
        ServiceError::Internal(format!("unexpected evaluation type `{eval_type_str}`"))
    })?;
    let public_results: Vec<PublicCaseResult> =
        decode_optional_json(public_results_json, &format!("{prefix} public results"))?
            .unwrap_or_default();
    let aggregate_metrics: Vec<MetricValue> =
        decode_optional_json(aggregate_json, &format!("{prefix} aggregate metrics"))?
            .unwrap_or_default();
    let run_metrics: Vec<RunMetricResult> =
        decode_optional_json(run_metrics_json, &format!("{prefix} run metrics"))?
            .unwrap_or_default();
    let validation_summary = decode_optional_json(
        validation_summary_json,
        &format!("{prefix} validation summary"),
    )?;
    let official_summary =
        decode_optional_json(official_json, &format!("{prefix} official summary"))?;

    Ok(Some(EvaluationDto {
        id,
        target,
        status,
        eval_type,
        aggregate_metrics,
        run_metrics,
        public_results,
        validation_summary,
        official_summary,
        runner_log_storage_key,
        started_at: started_at.map(|d| d.to_rfc3339()),
        finished_at: finished_at.map(|d| d.to_rfc3339()),
    }))
}

/// Reads a solution-submission status and validates its persisted value.
pub(super) fn solution_submission_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<SolutionSubmissionStatus> {
    let value: String = row.try_get(column)?;
    SolutionSubmissionStatus::from_storage_value(&value).ok_or_else(|| {
        ServiceError::Internal(format!("unexpected solution submission status `{value}`"))
    })
}

/// Reads an optional evaluation job status and validates its persisted value.
pub(super) fn optional_evaluation_job_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<EvaluationJobStatus>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(|value| {
            EvaluationJobStatus::from_storage_value(&value).ok_or_else(|| {
                ServiceError::Internal(format!("unexpected evaluation job status `{value}`"))
            })
        })
        .transpose()
}

/// Reads an optional evaluation result status and validates its persisted value.
pub(super) fn optional_evaluation_status_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<EvaluationStatus>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(|value| {
            EvaluationStatus::from_storage_value(&value).ok_or_else(|| {
                ServiceError::Internal(format!("unexpected evaluation status `{value}`"))
            })
        })
        .transpose()
}

/// Reads an optional scoring mode and validates its persisted value.
pub(super) fn optional_scoring_mode_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<ScoringMode>> {
    let value: Option<String> = row.try_get(column)?;
    value
        .map(|value| {
            ScoringMode::from_storage_value(&value).ok_or_else(|| {
                ServiceError::Internal(format!("unexpected evaluation type `{value}`"))
            })
        })
        .transpose()
}

/// Reads storage key from a database row and validates its domain shape.
pub(super) fn storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<StorageKey> {
    let value: String = row.try_get(column)?;
    StorageKey::try_new(&value).map_err(|e| {
        ServiceError::Internal(format!("stored invalid storage key in `{column}`: {e}"))
    })
}

/// Reads optional solution artifact metadata from a database row.
pub(super) fn artifact_metadata_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<SolutionArtifactMetadata>> {
    let artifact_zip_bytes = optional_u64_from_row(row, "artifact_zip_bytes")?;
    let artifact_uncompressed_bytes = optional_u64_from_row(row, "artifact_uncompressed_bytes")?;
    let artifact_file_count = optional_u64_from_row(row, "artifact_file_count")?;
    let artifact_sha256: Option<String> = row.try_get("artifact_sha256")?;
    match (
        artifact_zip_bytes,
        artifact_uncompressed_bytes,
        artifact_file_count,
        artifact_sha256,
    ) {
        (None, None, None, None) => Ok(None),
        (
            Some(artifact_zip_bytes),
            Some(artifact_uncompressed_bytes),
            Some(artifact_file_count),
            Some(artifact_sha256),
        ) => {
            let artifact_sha256 = Sha256Digest::try_new(&artifact_sha256).map_err(|e| {
                ServiceError::Internal(format!("stored invalid artifact_sha256: {e}"))
            })?;
            Ok(Some(SolutionArtifactMetadata {
                artifact_zip_bytes,
                artifact_uncompressed_bytes,
                artifact_file_count,
                artifact_sha256,
            }))
        }
        _ => Err(ServiceError::Internal(
            "stored partial solution artifact metadata".to_string(),
        )),
    }
}

/// Reads an optional non-negative BIGINT as `u64`.
fn optional_u64_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<u64>> {
    let value: Option<i64> = row.try_get(column)?;
    value
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| ServiceError::Internal(format!("stored negative value in `{column}`")))
        })
        .transpose()
}

/// Converts a bounded `u64` to PostgreSQL BIGINT.
pub(super) fn u64_to_i64(value: u64, field: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| ServiceError::Validation(format!("{field} exceeds supported range")))
}

/// Reads optional storage key from a database row and validates its domain shape.
fn optional_storage_key_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<StorageKey>> {
    row.try_get::<Option<String>, _>(column)?
        .map(StorageKey::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!("stored invalid storage key in `{column}`: {e}"))
        })
}

/// Reads optional evaluation job id from a database row and validates its domain shape.
pub(super) fn optional_evaluation_job_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<EvaluationJobId>> {
    optional_uuid_string_from_row(row, column)?
        .map(EvaluationJobId::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!(
                "stored invalid evaluation job id in column `{column}`: {e}"
            ))
        })
}

/// Reads optional solution submission id from a database row and validates its domain shape.
pub(super) fn optional_solution_submission_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<SolutionSubmissionId>> {
    optional_uuid_string_from_row(row, column)?
        .map(SolutionSubmissionId::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!(
                "stored invalid solution submission id in column `{column}`: {e}"
            ))
        })
}

/// Reads a target name from a database row and validates its domain shape.
pub(super) fn target_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<TargetName> {
    let value: String = row.try_get(column)?;
    TargetName::try_new(value).map_err(|error| {
        ServiceError::Internal(format!("stored invalid target in `{column}`: {error}"))
    })
}

/// Reads optional evaluation id from a database row and validates its domain shape.
fn optional_evaluation_id_from_row(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<Option<EvaluationId>> {
    optional_uuid_string_from_row(row, column)?
        .map(EvaluationId::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!(
                "stored invalid evaluation id in column `{column}`: {e}"
            ))
        })
}
