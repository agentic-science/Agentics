use agentics_contracts::validation::public_api::{self, DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT};
use agentics_domain::models::evaluation::{
    EvaluationDto, EvaluationJobDto, EvaluationJobStatus, MetricValue, ScoringMode,
    SolutionSubmissionStatus,
};
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::request::{
    CreateSolutionSubmissionResponse, PublicSolutionSubmissionListResponse,
    SolutionSubmissionResponse, SolutionSubmissionResultReportResponse,
};
use agentics_error::{Result, ServiceError};
use agentics_persistence::{Repositories, SolutionSubmissionRecord};

use super::visibility::{
    SolutionSubmissionAudience, ensure_public_result_detail_visible,
    ensure_public_result_detail_visible_for_spec, ensure_public_solution_artifact_visible,
    load_challenge_policy, public_visible_solution_submission,
};

/// Present the response returned immediately after solution submission creation.
pub fn present_create_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
) -> Result<CreateSolutionSubmissionResponse> {
    let evaluation_job_id = solution_submission
        .evaluation_job_id
        .clone()
        .ok_or_else(|| {
            ServiceError::Internal(
                "created solution submission is missing its initial evaluation job id".to_string(),
            )
        })?;

    Ok(CreateSolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        status: solution_submission_status_from_storage(&solution_submission.status)?,
        challenge_name: solution_submission.challenge_name.clone(),
        target: solution_submission.target.clone(),
        artifact_key: solution_submission.artifact_key.clone(),
        note: solution_submission.note.clone(),
        evaluation_job_id,
        created_at: solution_submission.created_at.to_rfc3339(),
    })
}

/// Present a solution submission while applying audience and benchmark visibility policy.
pub fn present_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
    audience: SolutionSubmissionAudience,
) -> Result<SolutionSubmissionResponse> {
    let evaluation = present_evaluation(solution_submission.evaluation.as_ref(), audience);
    let validation_evaluation = if audience.includes_validation_details() {
        present_evaluation(solution_submission.validation_evaluation.as_ref(), audience)
    } else {
        None
    };
    let official_primary_metric =
        solution_submission
            .official_evaluation
            .as_ref()
            .and_then(|evaluation| {
                MetricValue::find_by_name(
                    &evaluation.aggregate_metrics,
                    &solution_submission
                        .challenge_spec
                        .metric_schema
                        .ranking
                        .primary_metric_name,
                )
            });
    let official_evaluation =
        present_evaluation(solution_submission.official_evaluation.as_ref(), audience);
    let evaluation_job = if audience.includes_evaluation_job() {
        solution_submission
            .evaluation_job_id
            .as_ref()
            .map(|id| {
                Ok::<_, ServiceError>(EvaluationJobDto {
                    id: id.clone(),
                    target: solution_submission.target.clone(),
                    status: evaluation_job_status_from_storage(
                        solution_submission
                            .evaluation_job_status
                            .as_deref()
                            .unwrap_or("queued"),
                    )?,
                })
            })
            .transpose()?
    } else {
        None
    };

    Ok(SolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        challenge_name: solution_submission.challenge_name.clone(),
        challenge_title: solution_submission.challenge_title.clone(),
        target: solution_submission.target.clone(),
        agent_id: solution_submission.agent_id.clone(),
        agent_display_name: solution_submission.agent_display_name.clone(),
        status: solution_submission_status_from_storage(&solution_submission.status)?,
        note: solution_submission.note.clone(),
        explanation: solution_submission.explanation.clone(),
        parent_solution_submission_id: solution_submission.parent_solution_submission_id.clone(),
        credit_text: solution_submission.credit_text.clone(),
        official_primary_metric,
        visible_after_eval: solution_submission.visible_after_eval,
        artifact_key: if audience.includes_artifact_key() {
            Some(solution_submission.artifact_key.clone())
        } else {
            None
        },
        evaluation_job,
        evaluation,
        validation_evaluation,
        official_evaluation,
        created_at: solution_submission.created_at.to_rfc3339(),
        updated_at: solution_submission.updated_at.to_rfc3339(),
    })
}

/// List public solution submissions visible for one challenge and target.
pub async fn list_public_solution_submissions(
    pool: &sqlx::PgPool,
    challenge_name: &ChallengeName,
    target: Option<&str>,
    limit: Option<i64>,
) -> Result<PublicSolutionSubmissionListResponse> {
    let (_challenge, spec) = load_challenge_policy(pool, challenge_name).await?;
    ensure_public_result_detail_visible_for_spec(&spec)?;
    let target = public_api::resolve_required_public_target(&spec, target)?;
    let limit = public_api::bounded_public_limit(
        limit,
        DEFAULT_PUBLIC_SUBMISSION_LIST_LIMIT,
        "solution submission list",
    )?;
    let repos = Repositories::new(pool);
    let items = repos
        .solution_submissions()
        .list_public_for_challenge(challenge_name, &target, limit)
        .await?;
    let total_count = repos
        .solution_submissions()
        .count_public_for_challenge(challenge_name, &target)
        .await?;
    Ok(PublicSolutionSubmissionListResponse { total_count, items })
}

/// Fetch a public solution submission view without private artifact paths or job metadata.
pub async fn get_public_solution_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_result_detail_visible(pool, &solution_submission.challenge_name).await?;
    present_solution_submission(&solution_submission, SolutionSubmissionAudience::Public)
}

/// Fetch a public redacted result report when the challenge visibility allows it.
pub async fn get_public_solution_submission_result_report(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionResultReportResponse> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_result_detail_visible(pool, &solution_submission.challenge_name).await?;
    Ok(SolutionSubmissionResultReportResponse {
        solution_submission: present_solution_submission(
            &solution_submission,
            SolutionSubmissionAudience::Public,
        )?,
    })
}

/// Fetch the public submission record after enforcing visibility for artifact access.
pub async fn get_public_artifact_submission(
    pool: &sqlx::PgPool,
    id: &SolutionSubmissionId,
) -> Result<SolutionSubmissionRecord> {
    let solution_submission = public_visible_solution_submission(pool, id).await?;
    ensure_public_solution_artifact_visible(pool, &solution_submission.challenge_name).await?;
    Ok(solution_submission)
}

/// Parse a persisted solution-submission status for response DTOs.
fn solution_submission_status_from_storage(value: &str) -> Result<SolutionSubmissionStatus> {
    SolutionSubmissionStatus::from_storage_value(value).ok_or_else(|| {
        ServiceError::Internal(format!(
            "stored invalid solution submission status `{value}`"
        ))
    })
}

/// Parse a persisted evaluation job status for response DTOs.
fn evaluation_job_status_from_storage(value: &str) -> Result<EvaluationJobStatus> {
    EvaluationJobStatus::from_storage_value(value).ok_or_else(|| {
        ServiceError::Internal(format!("stored invalid evaluation job status `{value}`"))
    })
}

/// Projects one persisted evaluation according to audience and benchmark privacy policy.
fn present_evaluation(
    evaluation: Option<&EvaluationDto>,
    audience: SolutionSubmissionAudience,
) -> Option<EvaluationDto> {
    let evaluation = evaluation?;
    match evaluation.eval_type {
        ScoringMode::Validation if audience.includes_validation_details() => {
            Some(evaluation.clone())
        }
        ScoringMode::Validation => None,
        ScoringMode::Official => Some(redact_private_benchmark_details(evaluation, audience)),
    }
}

/// Removes official-run fields that could reveal private benchmark cases or logs.
fn redact_private_benchmark_details(
    evaluation: &EvaluationDto,
    audience: SolutionSubmissionAudience,
) -> EvaluationDto {
    let include_aggregate_feedback = audience.includes_official_aggregate_feedback();
    EvaluationDto {
        id: evaluation.id.clone(),
        target: evaluation.target.clone(),
        status: evaluation.status,
        eval_type: evaluation.eval_type,
        rank_score: evaluation.rank_score,
        aggregate_metrics: if include_aggregate_feedback {
            evaluation.aggregate_metrics.clone()
        } else {
            Vec::new()
        },
        run_metrics: Vec::new(),
        public_results: Vec::new(),
        validation_summary: None,
        official_summary: if include_aggregate_feedback {
            evaluation.official_summary.clone()
        } else {
            None
        },
        log_key: None,
        started_at: evaluation.started_at.clone(),
        finished_at: evaluation.finished_at.clone(),
    }
}
