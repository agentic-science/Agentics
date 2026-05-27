//! Evaluation job readiness transitions for newly staged submissions.

use agentics_domain::models::ids::{EvaluationJobId, SolutionSubmissionId};
use agentics_domain::storage::StorageKey;
use agentics_error::Result;
use agentics_storage::Storage;

use crate::evaluation_lifecycle;

use super::cleanup::{cleanup_solution_submission_record, cleanup_storage_key};

/// Mark the staged evaluation job ready, rolling back durable submission state on failure.
pub(super) async fn mark_initial_job_ready_or_cleanup(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    solution_submission_id: &SolutionSubmissionId,
    job_id: &EvaluationJobId,
    durable_artifact_key: &StorageKey,
    temporary_artifact_key: &StorageKey,
) -> Result<()> {
    if let Err(error) = evaluation_lifecycle::mark_staged_evaluation_job_ready(pool, job_id).await {
        cleanup_solution_submission_record(pool, solution_submission_id).await;
        cleanup_storage_key(storage, durable_artifact_key).await;
        cleanup_storage_key(storage, temporary_artifact_key).await;
        return Err(error);
    }
    Ok(())
}
