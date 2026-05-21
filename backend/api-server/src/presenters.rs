//! Conversion helpers from database records to API DTOs.

use shared::db::{
    AgentRecord, ChallengeRecord, PioneerCodeRecord, PioneerCodeUseRecord, SolutionSubmissionRecord,
};
use shared::error::{AppError, Result};
use shared::models::challenge::{ChallengeBundleSpec, ChallengeDetailResponse};
use shared::models::evaluation::{
    EvaluationDto, EvaluationJobStatus, ScoringMode, SolutionSubmissionStatus,
};
use shared::models::pioneer_codes::{PioneerCodeStatus, PioneerCodeUseKind};
use shared::models::request::{
    CreateSolutionSubmissionResponse, PioneerCodeDetailResponse, PioneerCodeDto,
    PioneerCodeListResponse, PioneerCodeUseDto, RegisterAgentResponse, SolutionSubmissionResponse,
};

/// Present a newly registered agent together with its one-time bearer token.
pub fn present_register_agent(agent: &AgentRecord, token: &str) -> Result<RegisterAgentResponse> {
    Ok(RegisterAgentResponse {
        agent_id: agent.id.clone(),
        token: token.to_string(),
        display_name: agent.display_name.clone(),
        created_at: agent.created_at.to_rfc3339(),
    })
}

/// Present a pioneer-code list for admin review.
pub fn present_pioneer_code_list(codes: &[PioneerCodeRecord]) -> Result<PioneerCodeListResponse> {
    Ok(PioneerCodeListResponse {
        items: codes
            .iter()
            .map(present_pioneer_code)
            .collect::<Result<Vec<_>>>()?,
    })
}

/// Present one pioneer-code detail response with its created agents.
pub fn present_pioneer_code_detail(
    code: &PioneerCodeRecord,
    uses: &[PioneerCodeUseRecord],
) -> Result<PioneerCodeDetailResponse> {
    Ok(PioneerCodeDetailResponse {
        code: present_pioneer_code(code)?,
        uses: uses
            .iter()
            .map(present_pioneer_code_use)
            .collect::<Result<Vec<_>>>()?,
    })
}

/// Present a pioneer-code row without exposing the hashed validation value.
fn present_pioneer_code(code: &PioneerCodeRecord) -> Result<PioneerCodeDto> {
    Ok(PioneerCodeDto {
        id: code.id.clone(),
        code_display: code.code_display.clone(),
        label: code.label.clone(),
        note: code.note.clone(),
        max_uses: code.max_uses,
        use_count: code.use_count,
        status: PioneerCodeStatus::from_storage_value(&code.status).ok_or_else(|| {
            AppError::Internal(format!(
                "stored invalid pioneer-code status `{}`",
                code.status
            ))
        })?,
        expires_at: code.expires_at.map(|expires_at| expires_at.to_rfc3339()),
        created_by_admin_username: code.created_by_admin_username.clone(),
        created_at: code.created_at.to_rfc3339(),
        revoked_at: code.revoked_at.map(|revoked_at| revoked_at.to_rfc3339()),
    })
}

/// Present an agent account created through a pioneer code.
fn present_pioneer_code_use(use_record: &PioneerCodeUseRecord) -> Result<PioneerCodeUseDto> {
    Ok(PioneerCodeUseDto {
        agent_id: use_record.agent_id.clone(),
        agent_display_name: use_record.agent_display_name.clone(),
        registration_kind: PioneerCodeUseKind::from_storage_value(&use_record.registration_kind)
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "stored invalid pioneer-code registration kind `{}`",
                    use_record.registration_kind
                ))
            })?,
        used_at: use_record.used_at.to_rfc3339(),
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
        keywords: spec.keywords.clone(),
        spec: spec.into(),
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
        status: solution_submission_status_from_storage(&solution_submission.status)?,
        challenge_name: solution_submission.challenge_name.clone(),
        target: solution_submission.target.clone(),
        artifact_key: solution_submission.artifact_key.clone(),
        note: solution_submission.note.clone(),
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
                shared::models::evaluation::MetricValue::find_by_name(
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
                Ok::<_, AppError>(shared::models::evaluation::EvaluationJobDto {
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

/// Parse a persisted solution-submission status for response DTOs.
fn solution_submission_status_from_storage(value: &str) -> Result<SolutionSubmissionStatus> {
    SolutionSubmissionStatus::from_storage_value(value).ok_or_else(|| {
        AppError::Internal(format!(
            "stored invalid solution submission status `{value}`"
        ))
    })
}

/// Parse a persisted evaluation job status for response DTOs.
fn evaluation_job_status_from_storage(value: &str) -> Result<EvaluationJobStatus> {
    EvaluationJobStatus::from_storage_value(value).ok_or_else(|| {
        AppError::Internal(format!("stored invalid evaluation job status `{value}`"))
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
