//! Solution submission admission, artifact staging, and initial job creation.

use tracing::warn;
use uuid::Uuid;

use agentics_config::Config;
use agentics_contracts::zip_project::ZipProjectManifest;
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AgentId, EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::names::TargetName;
use agentics_domain::models::request::{
    CreateSolutionSubmissionRequest, CreateSolutionSubmissionResponse,
};
use agentics_persistence::{
    CreateSolutionSubmissionInput, PublishedChallengeAdmission, Repositories,
    SolutionSubmissionQuotaAdmission,
};
use agentics_storage::{Storage, StorageKey};

use crate::evaluation_lifecycle;
use crate::public_projection;

const SUBMISSION_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

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
    let challenge_id = body.challenge_id;
    let target = body.target.clone();
    let admission = repos
        .challenges()
        .ensure_supports_eval_type(&challenge_id, &target, eval_type, &agent_id)
        .await?;
    let canonical_challenge_id = admission.challenge_id.clone();
    let canonical_challenge_name = admission.challenge_name.clone();
    let challenge_lifetime_limit = challenge_lifetime_limit(&admission, eval_type);
    ensure_submission_quota_available(
        pool,
        config,
        &agent_id,
        &canonical_challenge_id,
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
            &canonical_challenge_id,
            &target,
        )
        .await?;

    let artifact_bytes = base64_decode(&body.artifact_base64).ok_or(ServiceError::Base64)?;
    let manifest = ZipProjectManifest::from_zip_bytes(&artifact_bytes)?;

    let solution_submission_id = SolutionSubmissionId::generate();
    let job_id = EvaluationJobId::generate();
    let artifact_key =
        StorageKey::try_new(format!("solution-submissions/{solution_submission_id}.zip"))?;
    let temporary_artifact_key = StorageKey::try_new(format!(
        "_tmp/solution-submissions/{}-{}.zip",
        solution_submission_id,
        Uuid::new_v4()
    ))?;
    let temporary_artifact_key = storage
        .put(&temporary_artifact_key, &artifact_bytes)
        .await?;

    let quota_limit = match eval_type {
        ScoringMode::Validation => i64::from(config.validation_runs_per_agent_challenge_day),
        ScoringMode::Official => i64::from(config.official_runs_per_agent_challenge_day),
    };
    let max_active_official_jobs =
        (eval_type == ScoringMode::Official).then_some(i64::from(config.max_active_official_jobs));

    let solution_submission = repos
        .solution_submissions()
        .create_with_job(&CreateSolutionSubmissionInput {
            solution_submission_id: solution_submission_id.clone(),
            job_id: job_id.clone(),
            agent_id,
            challenge_id: canonical_challenge_id,
            challenge_name: canonical_challenge_name,
            target,
            artifact_key: artifact_key.clone(),
            note: manifest.note,
            eval_type,
            explanation: body.explanation.trim().to_string(),
            parent_solution_submission_id: body.parent_solution_submission_id,
            credit_text: body.credit_text.trim().to_string(),
            quota_admission: SolutionSubmissionQuotaAdmission {
                window_seconds: SUBMISSION_QUOTA_WINDOW_SECONDS,
                per_agent_challenge_limit: quota_limit,
                challenge_lifetime_limit,
                max_active_official_jobs,
            },
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
        .promote(&temporary_artifact_key, &artifact_key)
        .await
    {
        cleanup_solution_submission_record(pool, &solution_submission.id).await;
        cleanup_storage_key(storage, &temporary_artifact_key).await;
        return Err(error.into());
    }

    if let Err(error) = evaluation_lifecycle::mark_staged_evaluation_job_ready(pool, &job_id).await
    {
        cleanup_solution_submission_record(pool, &solution_submission.id).await;
        cleanup_storage_key(storage, &artifact_key).await;
        cleanup_storage_key(storage, &temporary_artifact_key).await;
        return Err(error);
    }
    let solution_submission = repos
        .solution_submissions()
        .get_by_id(&solution_submission.id)
        .await?
        .ok_or_else(|| {
            ServiceError::Internal(
                "solution submission disappeared after staged job was marked ready".to_string(),
            )
        })?;

    public_projection::present_create_solution_submission(&solution_submission)
}

/// Removes a staged submission row after storage or job admission fails.
async fn cleanup_solution_submission_record(
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
async fn cleanup_storage_key(storage: &dyn Storage, storage_key: &StorageKey) {
    if let Err(error) = storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up staged storage object after admission failure"
        );
    }
}

/// Performs pre-upload quota checks so abusive requests fail before artifact decode.
async fn ensure_submission_quota_available(
    pool: &sqlx::PgPool,
    config: &Config,
    agent_id: &AgentId,
    challenge_id: &agentics_domain::models::ids::ChallengeId,
    target: &TargetName,
    eval_type: ScoringMode,
    challenge_lifetime_limit: Option<i64>,
) -> Result<()> {
    let repos = Repositories::new(pool);
    let limit = match eval_type {
        ScoringMode::Validation => i64::from(config.validation_runs_per_agent_challenge_day),
        ScoringMode::Official => i64::from(config.official_runs_per_agent_challenge_day),
    };
    let used = repos
        .solution_submissions()
        .count_recent_runs_for_agent_challenge(
            agent_id,
            challenge_id,
            target,
            eval_type,
            SUBMISSION_QUOTA_WINDOW_SECONDS,
        )
        .await?;

    if used >= limit {
        return Err(ServiceError::TooManyRequests(format!(
            "{} quota exceeded for challenge `{challenge_id}`: {used} of {limit} runs used in the last 24 hours",
            eval_type.as_str()
        )));
    }

    if let Some(limit) = challenge_lifetime_limit {
        let used = repos
            .solution_submissions()
            .count_lifetime_runs_for_agent_challenge(agent_id, challenge_id, target, eval_type)
            .await?;
        if used >= limit {
            return Err(ServiceError::TooManyRequests(format!(
                "{} challenge limit exceeded for challenge `{challenge_id}`: {used} of {limit} lifetime runs used",
                eval_type.as_str()
            )));
        }
    }

    if eval_type == ScoringMode::Official {
        let active = repos
            .evaluation_jobs()
            .count_active(ScoringMode::Official)
            .await?;
        let max_active = i64::from(config.max_active_official_jobs);
        if active >= max_active {
            return Err(ServiceError::TooManyRequests(format!(
                "official evaluation queue is full: {active} of {max_active} official jobs are queued or running"
            )));
        }
    }

    Ok(())
}

/// Selects the challenge-level run limit that applies to the requested scoring mode.
fn challenge_lifetime_limit(
    admission: &PublishedChallengeAdmission,
    eval_type: ScoringMode,
) -> Option<i64> {
    match eval_type {
        ScoringMode::Validation => admission.validation_submission_limit,
        ScoringMode::Official => admission.official_submission_limit,
    }
}

/// Decodes user-provided base64 payloads after trimming transport whitespace.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.decode(input.trim()).ok()
}
