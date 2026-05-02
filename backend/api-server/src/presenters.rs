//! Conversion helpers from database records to API DTOs.

use shared::db::queries::{AgentRecord, ChallengeVersionRecord, SubmissionRecord};
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
                public_dir: "data/public".to_string(),
                private_benchmark_dir: None,
                public_policy: shared::models::evaluation::ScoreVisibility::Full,
                private_benchmark_policy: "score_only".to_string(),
                validation_enabled: false,
                private_benchmark_enabled: false,
            },
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

/// Present the response returned immediately after submission creation.
pub fn present_create_submission(submission: &SubmissionRecord) -> CreateSubmissionResponse {
    CreateSubmissionResponse {
        id: submission.id.clone(),
        status: submission.status.clone(),
        challenge_id: submission.challenge_id.clone(),
        challenge_version_id: submission.challenge_version_id.clone(),
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
        challenge_id: submission.challenge_id.clone(),
        challenge_title: submission.challenge_title.clone(),
        challenge_version_id: submission.challenge_version_id.clone(),
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
        validation_evaluation: submission.validation_evaluation.clone(),
        official_evaluation: submission.official_evaluation.clone(),
        created_at: submission.created_at.to_rfc3339(),
        updated_at: submission.updated_at.to_rfc3339(),
    }
}
