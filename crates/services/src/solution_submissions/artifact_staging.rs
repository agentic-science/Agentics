use uuid::Uuid;

use agentics_contracts::zip_project::MAX_ZIP_PROJECT_ARTIFACT_BYTES;
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_error::{Result, ServiceError};
use agentics_storage::{Storage, StorageKey, StorageWriteIntent};

use crate::storage_errors::storage_error_to_service_error;

/// Durable and temporary object-storage keys for a submitted solution ZIP.
pub(super) struct SolutionArtifactKeys {
    pub(super) durable: StorageKey,
    pub(super) temporary: StorageKey,
}

/// Decodes user-provided base64 payloads after trimming transport whitespace.
pub(super) fn decode_solution_artifact(input: &str) -> Result<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD
        .decode(input.trim())
        .map_err(|_| ServiceError::Base64)
}

/// Build no-overwrite durable and temporary artifact keys for one submission.
pub(super) fn solution_artifact_keys(
    solution_submission_id: &SolutionSubmissionId,
) -> Result<SolutionArtifactKeys> {
    let durable =
        StorageKey::try_new(format!("solution-submissions/{solution_submission_id}.zip"))?;
    let temporary = StorageKey::try_new(format!(
        "_tmp/solution-submissions/{}-{}.zip",
        solution_submission_id,
        Uuid::new_v4()
    ))?;
    Ok(SolutionArtifactKeys { durable, temporary })
}

/// Write the temporary artifact object before the DB row is promoted to ready.
pub(super) async fn stage_temporary_solution_artifact(
    storage: &dyn Storage,
    key: &StorageKey,
    artifact_bytes: &[u8],
) -> Result<StorageKey> {
    storage
        .put(
            key,
            artifact_bytes,
            StorageWriteIntent::new("solution artifact ZIP", MAX_ZIP_PROJECT_ARTIFACT_BYTES),
        )
        .await
        .map_err(storage_error_to_service_error)
}
