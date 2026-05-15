//! Conversion helpers from database records to API DTOs.

use shared::db::{AgentRecord, ChallengeRecord, SolutionSubmissionRecord};
use shared::error::{AppError, Result};
use shared::models::challenge::{ChallengeBundleSpec, ChallengeDetailResponse};
use shared::models::evaluation::{EvaluationDto, ScoringMode};
use shared::models::ids::AgentId;
use shared::models::request::{
    CreateSolutionSubmissionResponse, RegisterAgentResponse, SolutionSubmissionResponse,
};

/// Present a newly registered agent together with its one-time bearer token.
pub fn present_register_agent(agent: &AgentRecord, token: &str) -> Result<RegisterAgentResponse> {
    Ok(RegisterAgentResponse {
        agent_id: AgentId::try_new(&agent.id).map_err(|e| {
            AppError::Internal(format!(
                "database returned invalid registered agent id: {e}"
            ))
        })?,
        token: token.to_string(),
        display_name: agent.display_name.clone(),
        created_at: agent.created_at.to_rfc3339(),
    })
}

/// Present public challenge details from a published challenge record and statement body.
pub fn present_challenge_detail(
    challenge: &ChallengeRecord,
    statement: &str,
) -> Result<ChallengeDetailResponse> {
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .map_err(|e| AppError::Internal(format!("stored challenge spec is invalid: {e}")))?;

    Ok(ChallengeDetailResponse {
        name: challenge.challenge_name.clone(),
        title: challenge.title.clone(),
        summary: challenge.summary.clone(),
        spec,
        statement_markdown: statement.to_string(),
    })
}

/// Present the response returned immediately after solution submission creation.
pub fn present_create_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
) -> Result<CreateSolutionSubmissionResponse> {
    let evaluation_job_id = solution_submission
        .evaluation_job_id
        .clone()
        .ok_or_else(|| {
            AppError::Internal(
                "created solution submission is missing its initial evaluation job id".to_string(),
            )
        })?;

    Ok(CreateSolutionSubmissionResponse {
        id: solution_submission.id.clone(),
        status: solution_submission.status.clone(),
        challenge_name: solution_submission.challenge_name.clone(),
        target: solution_submission.target.clone(),
        artifact_key: solution_submission.artifact_key.clone(),
        evaluation_job_id,
        created_at: solution_submission.created_at.to_rfc3339(),
    })
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
    /// Returns whether this audience may see the stored solution artifact key.
    fn includes_artifact_key(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see the current evaluation job handle.
    fn includes_evaluation_job(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns whether this audience may see validation-mode evaluation details.
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
        challenge_name: solution_submission.challenge_name.clone(),
        challenge_title: solution_submission.challenge_title.clone(),
        target: solution_submission.target.clone(),
        agent_id: solution_submission.agent_id.clone(),
        agent_display_name: solution_submission.agent_display_name.clone(),
        status: solution_submission.status.clone(),
        explanation: solution_submission.explanation.clone(),
        parent_solution_submission_id: solution_submission.parent_solution_submission_id.clone(),
        credit_text: solution_submission.credit_text.clone(),
        visible_after_eval: solution_submission.visible_after_eval,
        artifact_key: if audience.includes_artifact_key() {
            Some(solution_submission.artifact_key.clone())
        } else {
            None
        },
        evaluation_job: if audience.includes_evaluation_job() {
            solution_submission.evaluation_job_id.as_ref().map(|id| {
                shared::models::evaluation::EvaluationJobDto {
                    id: id.clone(),
                    target: solution_submission.target.clone(),
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
        ScoringMode::Official => Some(redact_private_benchmark_details(evaluation)),
    }
}

/// Removes official-run fields that could reveal private benchmark cases or logs.
fn redact_private_benchmark_details(evaluation: &EvaluationDto) -> EvaluationDto {
    EvaluationDto {
        id: evaluation.id.clone(),
        target: evaluation.target.clone(),
        status: evaluation.status,
        eval_type: evaluation.eval_type,
        primary_score: evaluation.primary_score,
        rank_score: evaluation.rank_score,
        aggregate_metrics: Vec::new(),
        run_metrics: Vec::new(),
        public_results: Vec::new(),
        validation_summary: None,
        official_summary: None,
        log_key: None,
        started_at: evaluation.started_at.clone(),
        finished_at: evaluation.finished_at.clone(),
    }
}
