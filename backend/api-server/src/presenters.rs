//! Conversion helpers from database records to API DTOs.

use shared::db::{AgentRecord, ChallengeVersionRecord, SolutionSubmissionRecord};
use shared::error::{AppError, Result};
use shared::models::challenge::*;
use shared::models::evaluation::{EvaluationDto, ScoringMode};
use shared::models::request::*;

/// Present a newly registered agent together with its one-time bearer token.
pub fn present_register_agent(agent: &AgentRecord, token: &str) -> RegisterAgentResponse {
    RegisterAgentResponse {
        agent_id: agent.id.clone(),
        token: token.to_string(),
        name: agent.name.clone(),
        created_at: agent.created_at.to_rfc3339(),
    }
}

/// Present public challenge details from a published version record and statement body.
pub fn present_challenge_detail(
    challenge: &ChallengeVersionRecord,
    statement: &str,
) -> Result<ChallengeDetailResponse> {
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| AppError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    Ok(ChallengeDetailResponse {
        id: challenge.challenge_id.clone(),
        slug: challenge.slug.clone(),
        title: challenge.title.clone(),
        summary: challenge.summary.clone(),
        current_version: shared::models::CurrentVersionDto {
            id: challenge.challenge_version_id.clone(),
            version: challenge.version.clone(),
        },
        spec,
        statement_markdown: statement.to_string(),
    })
}

/// Present the response returned immediately after solution submission creation.
pub fn present_create_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
) -> CreateSolutionSubmissionResponse {
    CreateSolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        status: solution_submission.status.clone(),
        challenge_id: solution_submission.challenge_id.clone(),
        challenge_version_id: solution_submission.challenge_version_id.clone(),
        artifact_path: solution_submission.artifact_path.clone(),
        evaluation_job_id: solution_submission
            .evaluation_job_id
            .clone()
            .unwrap_or_default(),
        created_at: solution_submission.created_at.to_rfc3339(),
    }
}

/// Audience-specific projection for solution submission details.
#[derive(Debug, Clone, Copy)]
pub enum SolutionSubmissionAudience {
    /// The submitting agent can see its artifact path, job id, and validation details.
    Owner,
    /// Public viewers can only see ranking-visible data.
    Public,
}

impl SolutionSubmissionAudience {
    fn includes_artifact_path(self) -> bool {
        matches!(self, Self::Owner)
    }

    fn includes_evaluation_job(self) -> bool {
        matches!(self, Self::Owner)
    }

    fn includes_validation_details(self) -> bool {
        matches!(self, Self::Owner)
    }
}

/// Present a solution submission while applying audience and benchmark visibility policy.
pub fn present_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
    audience: SolutionSubmissionAudience,
) -> SolutionSubmissionResponse {
    let evaluation = present_evaluation(solution_submission.evaluation.as_ref(), audience);
    let validation_evaluation = if audience.includes_validation_details() {
        present_evaluation(solution_submission.validation_evaluation.as_ref(), audience)
    } else {
        None
    };
    let official_evaluation =
        present_evaluation(solution_submission.official_evaluation.as_ref(), audience);

    SolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        challenge_id: solution_submission.challenge_id.clone(),
        challenge_title: solution_submission.challenge_title.clone(),
        challenge_version_id: solution_submission.challenge_version_id.clone(),
        agent_id: solution_submission.agent_id.clone(),
        agent_name: solution_submission.agent_name.clone(),
        status: solution_submission.status.clone(),
        explanation: solution_submission.explanation.clone(),
        parent_solution_submission_id: solution_submission.parent_solution_submission_id.clone(),
        credit_text: solution_submission.credit_text.clone(),
        visible_after_eval: solution_submission.visible_after_eval,
        artifact_path: if audience.includes_artifact_path() {
            Some(solution_submission.artifact_path.clone())
        } else {
            None
        },
        evaluation_job: if audience.includes_evaluation_job() {
            solution_submission.evaluation_job_id.as_ref().map(|id| {
                shared::models::evaluation::EvaluationJobDto {
                    id: id.clone(),
                    status: match solution_submission.evaluation_job_status.as_deref() {
                        Some("running") => shared::models::evaluation::EvaluationStatus::Running,
                        Some("completed") => {
                            shared::models::evaluation::EvaluationStatus::Completed
                        }
                        Some("failed") => shared::models::evaluation::EvaluationStatus::Failed,
                        _ => shared::models::evaluation::EvaluationStatus::Queued,
                    },
                }
            })
        } else {
            None
        },
        evaluation,
        validation_evaluation,
        official_evaluation,
        created_at: solution_submission.created_at.to_rfc3339(),
        updated_at: solution_submission.updated_at.to_rfc3339(),
    }
}

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
        ScoringMode::Official => Some(redact_private_benchmark_details(evaluation)),
    }
}

fn redact_private_benchmark_details(evaluation: &EvaluationDto) -> EvaluationDto {
    EvaluationDto {
        id: evaluation.id.clone(),
        status: evaluation.status,
        eval_type: evaluation.eval_type,
        primary_score: evaluation.primary_score,
        rank_score: evaluation.rank_score,
        aggregate_metrics: evaluation.aggregate_metrics.clone(),
        run_metrics: Vec::new(),
        public_results: Vec::new(),
        validation_summary: None,
        official_summary: None,
        log_path: None,
        started_at: evaluation.started_at.clone(),
        finished_at: evaluation.finished_at.clone(),
    }
}
