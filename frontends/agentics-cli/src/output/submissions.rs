use agentics_domain::models::evaluation::EvaluationDto;
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::models::request::{
    CreateSolutionSubmissionResponse, PublicSolutionSubmissionListResponse, RankingContextResponse,
    SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};
use anyhow::Result;
use serde_json::{Map, Value, json};

use super::OutputFormat;
use super::format::{
    first_aggregate_metric, format_optional_metric, format_score, pretty_json, render_table,
    status_label,
};
use crate::package::SolutionPackage;

/// Renders create solution submission for user-facing output.
fn render_create_solution_submission(
    response: &CreateSolutionSubmissionResponse,
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "solution_submission": response,
            "package": {
                "workspace_dir": package.workspace_dir,
                "file_count": package.file_count,
                "uncompressed_bytes": package.uncompressed_bytes,
                "zip_bytes": package.bytes.len(),
            }
        })),
        OutputFormat::Table => Ok(format!(
            "Submitted {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}",
            response.id,
            response.challenge_name,
            response.target,
            response.status,
            response.evaluation_job_id,
            package.file_count,
            package.uncompressed_bytes,
            package.bytes.len(),
            package.workspace_dir.display()
        )),
    }
}

/// Renders create solution submission batch for user-facing output.
pub(crate) fn render_create_solution_submission_batch(
    responses: &[CreateSolutionSubmissionResponse],
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match responses {
        [response] => render_create_solution_submission(response, package, format),
        _ => render_create_submission_batch("solution_submissions", responses, package, format),
    }
}

/// Renders create validation run for user-facing output.
fn render_create_validation_run(
    response: &CreateSolutionSubmissionResponse,
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "validation_run": response,
            "package": {
                "workspace_dir": package.workspace_dir,
                "file_count": package.file_count,
                "uncompressed_bytes": package.uncompressed_bytes,
                "zip_bytes": package.bytes.len(),
            }
        })),
        OutputFormat::Table => Ok(format!(
            "Created validation run {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}",
            response.id,
            response.challenge_name,
            response.target,
            response.status,
            response.evaluation_job_id,
            package.file_count,
            package.uncompressed_bytes,
            package.bytes.len(),
            package.workspace_dir.display()
        )),
    }
}

/// Renders create validation run batch for user-facing output.
pub(crate) fn render_create_validation_run_batch(
    responses: &[CreateSolutionSubmissionResponse],
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match responses {
        [response] => render_create_validation_run(response, package, format),
        _ => render_create_submission_batch("validation_runs", responses, package, format),
    }
}

/// Renders solution submission status for user-facing output.
pub(crate) fn render_solution_submission_status(
    response: &SolutionSubmissionResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let evaluation_job = response
                .evaluation_job
                .as_ref()
                .map(|job| format!("{} ({})", job.id, status_label(&job.status)))
                .unwrap_or_else(|| "none".to_string());
            let validation_eval = response
                .validation_evaluation
                .as_ref()
                .map(|eval| status_label(&eval.status))
                .unwrap_or_else(|| "none".to_string());
            let official_eval = response
                .official_evaluation
                .as_ref()
                .map(|eval| status_label(&eval.status))
                .unwrap_or_else(|| "none".to_string());
            let display_eval = select_submission_display_evaluation(response);
            let rank_score = display_eval
                .and_then(|eval| eval.rank_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let official_primary_metric =
                format_optional_metric(response.official_primary_metric.as_ref());

            Ok(format!(
                "solution submission: {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\nvalidation_evaluation: {}\nofficial_evaluation: {}\nofficial_primary_metric: {}\nrank_score: {}\nvisible_after_eval: {}",
                response.id,
                response.challenge_name,
                response.target,
                response.status,
                evaluation_job,
                validation_eval,
                official_eval,
                official_primary_metric,
                rank_score,
                response.visible_after_eval
            ))
        }
    }
}

fn select_submission_display_evaluation(
    submission: &SolutionSubmissionResponse,
) -> Option<&EvaluationDto> {
    submission
        .official_evaluation
        .as_ref()
        .or(submission.validation_evaluation.as_ref())
        .or(submission.evaluation.as_ref())
}

/// Renders validation run status for user-facing output.
fn render_validation_run_status(
    response: &SolutionSubmissionResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let evaluation_job = response
                .evaluation_job
                .as_ref()
                .map(|job| format!("{} ({})", job.id, status_label(&job.status)))
                .unwrap_or_else(|| "none".to_string());
            let validation_eval = response
                .validation_evaluation
                .as_ref()
                .or(response.evaluation.as_ref());
            let validation_status = validation_eval
                .map(|eval| status_label(&eval.status))
                .unwrap_or_else(|| "none".to_string());
            let primary_metric =
                format_optional_metric(validation_eval.and_then(first_aggregate_metric));
            let rank_score = validation_eval
                .and_then(|eval| eval.rank_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());

            Ok(format!(
                "validation_run: {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\nvalidation: {}\nprimary_metric: {}\nrank_score: {}\nvisible_after_eval: {}",
                response.id,
                response.challenge_name,
                response.target,
                response.status,
                evaluation_job,
                validation_status,
                primary_metric,
                rank_score,
                response.visible_after_eval
            ))
        }
    }
}

/// Renders validation run status batch for user-facing output.
pub(crate) fn render_validation_run_status_batch(
    responses: &[SolutionSubmissionResponse],
    format: OutputFormat,
) -> Result<String> {
    match responses {
        [response] => render_validation_run_status(response, format),
        _ => match format {
            OutputFormat::Json => pretty_json(&json!({ "validation_runs": responses })),
            OutputFormat::Table => {
                let rows = responses
                    .iter()
                    .map(|response| {
                        let evaluation_job = response
                            .evaluation_job
                            .as_ref()
                            .map(|job| format!("{} ({})", job.id, status_label(&job.status)))
                            .unwrap_or_else(|| "none".to_string());
                        let validation_eval = response
                            .evaluation
                            .as_ref()
                            .or(response.validation_evaluation.as_ref());
                        let validation_status = validation_eval
                            .map(|eval| status_label(&eval.status))
                            .unwrap_or_else(|| "none".to_string());
                        let rank_score = validation_eval
                            .and_then(|eval| eval.rank_score)
                            .map(format_score)
                            .unwrap_or_else(|| "none".to_string());
                        vec![
                            response.target.to_string(),
                            response.id.to_string(),
                            status_label(&response.status),
                            evaluation_job,
                            validation_status,
                            rank_score,
                        ]
                    })
                    .collect::<Vec<_>>();
                Ok(render_table(
                    &["TARGET", "ID", "STATUS", "JOB", "VALIDATION", "RANK_SCORE"],
                    &rows,
                ))
            }
        },
    }
}

fn render_create_submission_batch(
    response_key: &str,
    responses: &[CreateSolutionSubmissionResponse],
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => {
            let mut object = Map::new();
            object.insert(
                "package".to_string(),
                json!({
                    "workspace_dir": package.workspace_dir,
                    "file_count": package.file_count,
                    "uncompressed_bytes": package.uncompressed_bytes,
                    "zip_bytes": package.bytes.len(),
                }),
            );
            object.insert(response_key.to_string(), serde_json::to_value(responses)?);
            pretty_json(&Value::Object(object))
        }
        OutputFormat::Table => {
            let rows = responses
                .iter()
                .map(|response| {
                    vec![
                        response.target.to_string(),
                        response.id.to_string(),
                        response.challenge_name.to_string(),
                        response.status.to_string(),
                        response.evaluation_job_id.to_string(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(format!(
                "{}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}",
                render_table(&["TARGET", "ID", "CHALLENGE", "STATUS", "JOB"], &rows),
                package.file_count,
                package.uncompressed_bytes,
                package.bytes.len(),
                package.workspace_dir.display()
            ))
        }
    }
}

/// Renders solution submission logs for user-facing output.
pub(crate) fn render_solution_submission_logs(
    response: &SolutionSubmissionLogsResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "solution_submission: {}\navailability: {}\nrunner_log_storage_key: {}\ntruncated: {}\n\n{}",
            response.solution_submission_id,
            response.availability,
            response
                .runner_log_storage_key
                .as_ref()
                .map_or("none", agentics_storage::StorageKey::as_str),
            response.truncated,
            response.content.as_deref().unwrap_or("")
        )),
    }
}

/// Renders public solution submission rows for a challenge target.
pub(crate) fn render_public_solution_submission_list(
    response: &PublicSolutionSubmissionListResponse,
    challenge_name: &ChallengeName,
    target: &TargetName,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let rows = response
                .items
                .iter()
                .map(|item| {
                    vec![
                        item.id.to_string(),
                        item.agent_display_name.clone(),
                        item.target.to_string(),
                        item.status.to_string(),
                        item.rank_score
                            .map(format_score)
                            .unwrap_or_else(|| "none".to_string()),
                        format_optional_metric(item.official_primary_metric.as_ref()),
                        item.created_at.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(format!(
                "challenge_name: {challenge_name}\ntarget: {target}\ntotal_visible: {}\n{}",
                response.total_count,
                render_table(
                    &[
                        "ID",
                        "AGENT",
                        "TARGET",
                        "STATUS",
                        "RANK_SCORE",
                        "OFFICIAL_PRIMARY_METRIC",
                        "CREATED",
                    ],
                    &rows,
                )
            ))
        }
    }
}

/// Renders a detailed report for a submitted solution.
pub(crate) fn render_solution_submission_report(
    response: &SolutionSubmissionResultReportResponse,
    ranking_context: Option<&RankingContextResponse>,
    authenticated_logs_available: bool,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "result_report": response,
            "ranking_context": ranking_context,
            "authenticated_logs_available": authenticated_logs_available,
        })),
        OutputFormat::Table => {
            let submission = &response.solution_submission;
            let validation_primary_metric = format_optional_metric(
                submission
                    .validation_evaluation
                    .as_ref()
                    .and_then(first_aggregate_metric),
            );
            let official_primary_metric =
                format_optional_metric(submission.official_primary_metric.as_ref());
            let rank_score = select_submission_display_evaluation(submission)
                .and_then(|evaluation| evaluation.rank_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let metrics = select_submission_display_evaluation(submission)
                .map(|evaluation| {
                    evaluation
                        .aggregate_metrics
                        .iter()
                        .map(|metric| {
                            vec![metric.metric_name.to_string(), format_score(metric.value)]
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let rank = ranking_context
                .and_then(|context| context.rank)
                .map(|rank| rank.to_string())
                .unwrap_or_else(|| "unranked".to_string());
            let total_ranked = ranking_context
                .map(|context| context.total_ranked.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let percentile = ranking_context
                .and_then(|context| context.percentile)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let is_agent_best = ranking_context
                .map(|context| context.is_agent_best.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let log_hint = if authenticated_logs_available {
                format!("agentics submissions logs {}", submission.id)
            } else {
                "configure the submitter token to inspect private logs".to_string()
            };

            Ok(format!(
                "solution_submission: {}\nchallenge: {}\ntarget: {}\nagent: {}\nstatus: {}\ncreated_at: {}\nupdated_at: {}\nvalidation_primary_metric: {}\nofficial_primary_metric: {}\nrank_score: {}\nrank: {}\ntotal_ranked: {}\npercentile: {}\nis_agent_best: {}\nmetrics:\n{}\nlogs: {}",
                submission.id,
                submission.challenge_name,
                submission.target,
                submission
                    .agent_display_name
                    .as_deref()
                    .unwrap_or(submission.agent_id.as_str()),
                submission.status,
                submission.created_at,
                submission.updated_at,
                validation_primary_metric,
                official_primary_metric,
                rank_score,
                rank,
                total_ranked,
                percentile,
                is_agent_best,
                render_table(&["METRIC", "VALUE"], &metrics),
                log_hint,
            ))
        }
    }
}

/// Renders ranking context for user-facing output.
pub(crate) fn render_ranking_context(
    response: &RankingContextResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let nearby = response
                .nearby_entries
                .iter()
                .map(|entry| {
                    vec![
                        entry.rank.to_string(),
                        entry.entry.agent_display_name.clone(),
                        entry.entry.best_solution_submission_id.to_string(),
                        format_score(entry.entry.best_rank_score),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(format!(
                "solution_submission: {}\nchallenge: {}\ntarget: {}\nrank: {}\ntotal_ranked: {}\npercentile: {}\nis_agent_best: {}\nnearby:\n{}",
                response.solution_submission_id,
                response.challenge_name,
                response.target,
                response
                    .rank
                    .map(|rank| rank.to_string())
                    .unwrap_or_else(|| "unranked".to_string()),
                response.total_ranked,
                response
                    .percentile
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                response.is_agent_best,
                render_table(&["RANK", "AGENT", "SUBMISSION", "SCORE"], &nearby)
            ))
        }
    }
}
