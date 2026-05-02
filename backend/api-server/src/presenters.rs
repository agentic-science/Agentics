//! Conversion helpers from database records to API DTOs.

use shared::db::{AgentRecord, ChallengeVersionRecord, SolutionSubmissionRecord};
use shared::models::challenge::*;
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
) -> ChallengeDetailResponse {
    let spec: ChallengeBundleSpec = serde_json::from_value(challenge.spec_json.clone())
        .unwrap_or_else(|_| ChallengeBundleSpec {
            schema_version: 1,
            challenge_id: challenge.challenge_id.clone(),
            challenge_title: challenge.title.clone(),
            challenge_version: challenge.version.clone(),
            solution: SolutionSpec {
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
                public_dir: "data/public".to_string(),
                private_benchmark_dir: None,
                public_policy: shared::models::evaluation::ScoreVisibility::Full,
                private_benchmark_policy: "score_only".to_string(),
                validation_enabled: false,
                private_benchmark_enabled: false,
            },
            community: None,
            metric_schema: MetricSchemaSpec::default(),
        });

    ChallengeDetailResponse {
        id: challenge.challenge_id.clone(),
        slug: challenge.slug.clone(),
        title: challenge.title.clone(),
        description: challenge.description.clone(),
        current_version: shared::models::CurrentVersionDto {
            id: challenge.challenge_version_id.clone(),
            version: challenge.version.clone(),
        },
        spec,
        statement_markdown: statement.to_string(),
    }
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

/// Present a solution submission while controlling fields that are hidden on public routes.
pub fn present_solution_submission(
    solution_submission: &SolutionSubmissionRecord,
    include_artifact_path: bool,
    include_evaluation_job: bool,
) -> SolutionSubmissionResponse {
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
        artifact_path: if include_artifact_path {
            Some(solution_submission.artifact_path.clone())
        } else {
            None
        },
        evaluation_job: if include_evaluation_job {
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
        evaluation: solution_submission.evaluation.clone(),
        validation_evaluation: solution_submission.validation_evaluation.clone(),
        official_evaluation: solution_submission.official_evaluation.clone(),
        created_at: solution_submission.created_at.to_rfc3339(),
        updated_at: solution_submission.updated_at.to_rfc3339(),
    }
}
