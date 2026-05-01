//! Conversion helpers from database records to API DTOs.

use shared::db::queries::{AgentRecord, ProblemVersionRecord, SubmissionRecord};
use shared::models::problem::*;
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

/// Present public problem details from a published version record and statement body.
pub fn present_problem_detail(
    problem: &ProblemVersionRecord,
    statement: &str,
) -> ProblemDetailResponse {
    let spec: ProblemBundleSpec =
        serde_json::from_value(problem.spec_json.clone()).unwrap_or_else(|_| ProblemBundleSpec {
            schema_version: 1,
            problem_id: problem.problem_id.clone(),
            problem_title: problem.title.clone(),
            problem_version: problem.version.clone(),
            submission: SubmissionSpec {
                format: "python_zip_project".to_string(),
                language: "python".to_string(),
                entrypoint: "main.py".to_string(),
            },
            scorer: ScorerSpec {
                entrypoint: "scorer/run.py".to_string(),
                result_file: "result.json".to_string(),
            },
            limits: LimitsSpec {
                time_limit_sec: 30.0,
                memory_limit_mb: 512,
            },
            datasets: DatasetsSpec {
                shown_dir: "data/shown".to_string(),
                hidden_dir: "data/hidden".to_string(),
                heldout_dir: None,
                shown_policy: shared::models::evaluation::ScoreVisibility::Full,
                hidden_policy: "score_only".to_string(),
                validation_enabled: false,
                heldout_enabled: false,
            },
        });

    ProblemDetailResponse {
        id: problem.problem_id.clone(),
        slug: problem.slug.clone(),
        title: problem.title.clone(),
        description: problem.description.clone(),
        current_version: shared::models::CurrentVersionDto {
            id: problem.problem_version_id.clone(),
            version: problem.version.clone(),
        },
        spec,
        statement_markdown: statement.to_string(),
    }
}

/// Present the response returned immediately after submission creation.
pub fn present_create_submission(submission: &SubmissionRecord) -> CreateSubmissionResponse {
    CreateSubmissionResponse {
        id: submission.id.clone(),
        status: submission.status.clone(),
        problem_id: submission.problem_id.clone(),
        problem_version_id: submission.problem_version_id.clone(),
        artifact_path: submission.artifact_path.clone(),
        evaluation_job_id: submission.evaluation_job_id.clone().unwrap_or_default(),
        created_at: submission.created_at.to_rfc3339(),
    }
}

/// Present a submission while controlling fields that are hidden on public routes.
pub fn present_submission(
    submission: &SubmissionRecord,
    include_artifact_path: bool,
    include_evaluation_job: bool,
) -> SubmissionResponse {
    SubmissionResponse {
        id: submission.id.clone(),
        problem_id: submission.problem_id.clone(),
        problem_title: submission.problem_title.clone(),
        problem_version_id: submission.problem_version_id.clone(),
        agent_id: submission.agent_id.clone(),
        agent_name: submission.agent_name.clone(),
        status: submission.status.clone(),
        explanation: submission.explanation.clone(),
        parent_submission_id: submission.parent_submission_id.clone(),
        credit_text: submission.credit_text.clone(),
        visible_after_eval: submission.visible_after_eval,
        artifact_path: if include_artifact_path {
            Some(submission.artifact_path.clone())
        } else {
            None
        },
        evaluation_job: if include_evaluation_job {
            submission.evaluation_job_id.as_ref().map(|id| {
                shared::models::evaluation::EvaluationJobDto {
                    id: id.clone(),
                    status: match submission.evaluation_job_status.as_deref() {
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
        evaluation: submission.evaluation.clone(),
        public_evaluation: submission.public_evaluation.clone(),
        official_evaluation: submission.official_evaluation.clone(),
        created_at: submission.created_at.to_rfc3339(),
        updated_at: submission.updated_at.to_rfc3339(),
    }
}
