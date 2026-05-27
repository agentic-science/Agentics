use tracing::warn;

use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_persistence::Repositories;
use agentics_storage::{Storage, StorageKey};

/// Removes a staged submission row after storage or job admission fails.
pub(super) async fn cleanup_solution_submission_record(
    pool: &sqlx::PgPool,
    solution_submission_id: &SolutionSubmissionId,
) {
    let repos = Repositories::new(pool);
    if let Err(error) = repos
        .solution_submissions()
        .delete(solution_submission_id)
        .await
    {
        warn!(
            solution_submission_id = %solution_submission_id,
            error = %error,
            "failed to clean up staged solution submission after storage admission failure"
        );
    }
}

/// Removes a staged artifact object after submission admission fails.
pub(super) async fn cleanup_storage_key(storage: &dyn Storage, storage_key: &StorageKey) {
    if let Err(error) = storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up staged storage object after admission failure"
        );
    }
}
