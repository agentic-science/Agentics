//! Solution submission admission, artifact staging, and initial job creation.

use agentics_config::Config;
use agentics_contracts::zip_project::inspect_zip_project_artifact;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AgentId, EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::request::{
    CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse,
};
use agentics_error::Result;
use agentics_persistence::{CreateSolutionSubmissionInput, Repositories};
use agentics_storage::Storage;

use crate::storage_errors::storage_error_to_service_error;

mod admission;
mod artifact_staging;
mod cleanup;
mod job_staging;
mod presentation;

use admission::{challenge_lifetime_limit, ensure_submission_quota_available, quota_admission};
use artifact_staging::{
    decode_solution_artifact, solution_artifact_keys, stage_temporary_solution_artifact,
};
use cleanup::{cleanup_solution_submission_record, cleanup_storage_key};
use job_staging::mark_initial_job_ready_or_cleanup;
use presentation::present_created_solution_submission;

/// Authenticated request to create one official submission or validation run.
#[derive(Debug, Clone)]
pub struct CreateSolutionSubmissionServiceRequest {
    pub agent_id: AgentId,
    pub body: CreateSolutionSubmissionRequest,
    pub eval_type: ScoringMode,
}

/// Create a solution submission, stage its artifact, and queue the first evaluation job.
pub async fn create_solution_submission(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: CreateSolutionSubmissionServiceRequest,
) -> Result<CreateSolutionSubmissionResponse> {
    let CreateSolutionSubmissionServiceRequest {
        agent_id,
        body,
        eval_type,
    } = request;
    let repos = Repositories::new(pool);
    let challenge_name = body.challenge_name;
    let target = body.target.clone();
    let admission = repos
        .challenges()
        .ensure_supports_eval_type(&challenge_name, &target, eval_type, &agent_id)
        .await?;
    let canonical_challenge_name = admission.challenge_name.clone();
    let challenge_lifetime_limit = challenge_lifetime_limit(&admission, eval_type);
    ensure_submission_quota_available(
        pool,
        config,
        &agent_id,
        &canonical_challenge_name,
        &target,
        eval_type,
        challenge_lifetime_limit,
    )
    .await?;
    repos
        .solution_submissions()
        .ensure_parent_matches_scope(
            body.parent_solution_submission_id.as_ref(),
            &agent_id,
            &canonical_challenge_name,
            &target,
        )
        .await?;

    let artifact_bytes = decode_solution_artifact(&body.artifact_base64)?;
    let artifact = inspect_zip_project_artifact(&artifact_bytes)?;

    let solution_submission_id = SolutionSubmissionId::generate();
    let job_id = EvaluationJobId::generate();
    let artifact_keys = solution_artifact_keys(&solution_submission_id)?;
    let temporary_artifact_key =
        stage_temporary_solution_artifact(storage, &artifact_keys.temporary, &artifact_bytes)
            .await?;
    let quota_admission = quota_admission(config, eval_type, challenge_lifetime_limit);

    let solution_submission = repos
        .solution_submissions()
        .create_with_job(&CreateSolutionSubmissionInput {
            solution_submission_id: solution_submission_id.clone(),
            job_id: job_id.clone(),
            agent_id,
            challenge_name: canonical_challenge_name,
            target,
            artifact_key: artifact_keys.durable.clone(),
            artifact_metadata: artifact.metadata,
            note: artifact.manifest.note,
            eval_type,
            explanation: body.explanation.trim().to_string(),
            parent_solution_submission_id: body.parent_solution_submission_id,
            credit_text: body.credit_text.trim().to_string(),
            quota_admission,
        })
        .await;
    let solution_submission = match solution_submission {
        Ok(solution_submission) => solution_submission,
        Err(error) => {
            cleanup_storage_key(storage, &temporary_artifact_key).await;
            return Err(error);
        }
    };

    if let Err(error) = storage
        .promote(&temporary_artifact_key, &artifact_keys.durable)
        .await
    {
        cleanup_solution_submission_record(pool, &solution_submission.id).await;
        cleanup_storage_key(storage, &temporary_artifact_key).await;
        return Err(storage_error_to_service_error(error));
    }

    mark_initial_job_ready_or_cleanup(
        pool,
        storage,
        &solution_submission.id,
        &job_id,
        &artifact_keys.durable,
        &temporary_artifact_key,
    )
    .await?;

    present_created_solution_submission(pool, &solution_submission.id).await
}
