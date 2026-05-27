//! Response projection for solution submission creation workflows.

use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::request::CreateSolutionSubmissionResponse;
use agentics_error::{Result, ServiceError};
use agentics_persistence::Repositories;

use crate::public_projection;

/// Load the created submission and project the creation response.
pub(super) async fn present_created_solution_submission(
    pool: &sqlx::PgPool,
    solution_submission_id: &SolutionSubmissionId,
) -> Result<CreateSolutionSubmissionResponse> {
    let solution_submission = Repositories::new(pool)
        .solution_submissions()
        .get_by_id(solution_submission_id)
        .await?
        .ok_or_else(|| {
            ServiceError::Internal(
                "solution submission disappeared after staged job was marked ready".to_string(),
            )
        })?;

    public_projection::present_create_solution_submission(&solution_submission)
}
