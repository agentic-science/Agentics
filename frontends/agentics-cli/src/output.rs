use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;
use serde_json::{Map, Value, json};
use shared::models::challenge::{ChallengeDetailResponse, ChallengeListResponse, MetricDirection};
use shared::models::challenge_creation::{ChallengeDraftCleanupResponse, ChallengeDraftResponse};
use shared::models::evaluation::ScorerRunResult;
use shared::models::names::{ChallengeName, MetricName, TargetName};
use shared::models::request::{
    CreateSolutionSubmissionResponse, LeaderboardResponse, PublicSolutionSubmissionListResponse,
    RankingContextResponse, RegisterAgentResponse, ScoreDistributionResponse,
    SolutionSubmissionLogsResponse, SolutionSubmissionResponse,
    SolutionSubmissionResultReportResponse,
};

use crate::cli::OutputFormat;
use crate::config::ResolvedSettings;
use crate::package::SolutionPackage;
use crate::workspace::InitSolutionSummary;

#[derive(Debug, Clone, Serialize)]
/// Carries local validation package report data across this module boundary.
pub(crate) struct LocalValidationPackageReport {
    pub workspace_dir: PathBuf,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
    pub zip_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
/// Carries local validation target report data across this module boundary.
pub(crate) struct LocalValidationTargetReport {
    pub target: TargetName,
    pub log_path: PathBuf,
    pub result: ScorerRunResult,
}

#[derive(Debug, Clone, Serialize)]
/// Carries local validation report data across this module boundary.
pub(crate) struct LocalValidationReport {
    pub challenge_name: ChallengeName,
    pub bundle_dir: PathBuf,
    pub storage_root: PathBuf,
    pub package: LocalValidationPackageReport,
    pub targets: Vec<LocalValidationTargetReport>,
}

/// Renders register agent for user-facing output.
pub(crate) fn render_register_agent(
    response: &RegisterAgentResponse,
    saved_token: bool,
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "agent_id": response.agent_id,
            "display_name": response.display_name,
            "token": response.token,
            "created_at": response.created_at,
            "saved_token": saved_token,
            "config_path": settings.config_path,
            "api_base_url": settings.api_base_url.to_string(),
        })),
        OutputFormat::Table => Ok(format!(
            "Registered agent {}\nagent_id: {}\ntoken: {}\nsaved_token: {}\nconfig: {}",
            response.display_name,
            response.agent_id,
            response.token,
            saved_token,
            settings.config_path.display()
        )),
    }
}

/// Renders auth status for user-facing output.
pub(crate) fn render_auth_status(
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "api_base_url": settings.api_base_url.to_string(),
            "api_base_url_source": settings.api_base_url_source.to_string(),
            "token_configured": settings.token_configured(),
            "token_source": settings.token_source.to_string(),
            "config_path": settings.config_path,
        })),
        OutputFormat::Table => Ok(format!(
            "api_base_url: {} ({})\ntoken: {}\ntoken_source: {}\nconfig: {}",
            settings.api_base_url,
            settings.api_base_url_source,
            if settings.token_configured() {
                "configured"
            } else {
                "missing"
            },
            settings.token_source,
            settings.config_path.display()
        )),
    }
}

/// Renders config set for user-facing output.
pub(crate) fn render_config_set(
    key: &str,
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "updated": key,
            "config_path": settings.config_path,
        })),
        OutputFormat::Table => Ok(format!(
            "updated: {key}\nconfig: {}",
            settings.config_path.display()
        )),
    }
}

/// Renders challenge draft for user-facing output.
pub(crate) fn render_challenge_draft(
    response: &ChallengeDraftResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "challenge_draft: {}\nchallenge: {}\nrequest: {}\nstatus: {}\nrepo: {}#{}\npath: {}\ncommit: {}\nmanifest_sha256: {}\npublished_challenge: {}\nprivate_assets: {}\nvalidation_records: {}",
            response.id,
            response.challenge_name,
            status_label(&response.request),
            status_label(&response.status),
            response.repo_url,
            response.pr_number,
            response.challenge_path,
            response.commit_sha,
            response.manifest_sha256,
            response
                .published_challenge_name
                .as_ref()
                .map_or("none", ChallengeName::as_str),
            response.private_assets.len(),
            response.validation_records.len()
        )),
    }
}

/// Renders challenge draft cleanup for user-facing output.
pub(crate) fn render_challenge_draft_cleanup(
    response: &ChallengeDraftCleanupResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "abandoned_drafts: {}\npurged_private_assets: {}",
            response.abandoned_drafts, response.purged_private_assets
        )),
    }
}

/// Renders challenge list for user-facing output.
pub(crate) fn render_challenge_list(
    response: &ChallengeListResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            if response.items.is_empty() {
                return Ok("No published challenges found.".to_string());
            }

            let rows = response
                .items
                .iter()
                .map(|challenge| {
                    vec![
                        challenge.name.to_string(),
                        status_label(&challenge.eligibility.eligibility_type),
                        challenge.title.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(render_table(&["NAME", "ELIGIBILITY", "TITLE"], &rows))
        }
    }
}

/// Renders challenge detail for user-facing output.
pub(crate) fn render_challenge_detail(
    response: &ChallengeDetailResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let private_benchmark = if response.spec.datasets.private_benchmark_enabled {
                response
                    .spec
                    .datasets
                    .private_benchmark_dir
                    .as_ref()
                    .map_or("<configured>", |path| path.as_str())
            } else {
                "disabled"
            };
            Ok(format!(
                "{} ({})\nsummary: {}\nstarts_at: {}\ncloses_at: {}\neligibility: {}\nleaderboard_visibility: {}\nscore_distribution_visibility: {}\nresult_detail_visibility: {}\nsolution_publication: {}\nsolution_protocol: {} ({})\ntargets:\n{}\ndatasets: public={}, private_benchmark={}\nranking_metric: {}\n\n{}",
                response.title,
                response.name,
                response.summary,
                response.spec.starts_at.as_deref().unwrap_or("none"),
                response.spec.closes_at.as_deref().unwrap_or("none"),
                status_label(&response.spec.eligibility.eligibility_type),
                status_label(&response.spec.visibility.leaderboard),
                status_label(&response.spec.visibility.score_distribution),
                status_label(&response.spec.visibility.result_detail),
                status_label(&response.spec.solution_publication),
                response.spec.solution.protocol,
                response.spec.solution.manifest_file,
                format_targets(&response.spec.targets),
                response.spec.datasets.public_dir,
                private_benchmark,
                response.spec.metric_schema.ranking.primary_metric_name,
                response.statement_markdown.trim()
            ))
        }
    }
}

/// Renders init solution for user-facing output.
pub(crate) fn render_init_solution(
    summary: &InitSolutionSummary,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(summary),
        OutputFormat::Table => Ok(format!(
            "Initialized solution workspace: {}\nchallenge: {} ({})\nruntime_profile: {}\ninterface: {}",
            summary.workspace_dir.display(),
            summary.challenge_title,
            summary.challenge_name,
            summary.runtime_profile,
            summary.interface
        )),
    }
}

/// Renders create solution submission for user-facing output.
pub(crate) fn render_create_solution_submission(
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
pub(crate) fn render_create_validation_run(
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
            let rank_score = response
                .evaluation
                .as_ref()
                .and_then(|eval| eval.rank_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let validation_primary_score = response
                .validation_evaluation
                .as_ref()
                .and_then(|eval| eval.primary_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());

            Ok(format!(
                "solution submission: {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\nvalidation_evaluation: {}\nofficial_evaluation: {}\nvalidation_primary_score: {}\nrank_score: {}\nvisible_after_eval: {}",
                response.id,
                response.challenge_name,
                response.target,
                response.status,
                evaluation_job,
                validation_eval,
                official_eval,
                validation_primary_score,
                rank_score,
                response.visible_after_eval
            ))
        }
    }
}

/// Renders validation run status for user-facing output.
pub(crate) fn render_validation_run_status(
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
                .evaluation
                .as_ref()
                .or(response.validation_evaluation.as_ref());
            let validation_status = validation_eval
                .map(|eval| status_label(&eval.status))
                .unwrap_or_else(|| "none".to_string());
            let primary_score = validation_eval
                .and_then(|eval| eval.primary_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let rank_score = validation_eval
                .and_then(|eval| eval.rank_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());

            Ok(format!(
                "validation_run: {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\nvalidation: {}\nprimary_score: {}\nrank_score: {}\nvisible_after_eval: {}",
                response.id,
                response.challenge_name,
                response.target,
                response.status,
                evaluation_job,
                validation_status,
                primary_score,
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

/// Renders local validation report for user-facing output.
pub(crate) fn render_local_validation_report(
    report: &LocalValidationReport,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(report),
        OutputFormat::Table => match report.targets.as_slice() {
            [target] => Ok(format!(
                "Local validation completed\nchallenge: {}\ntarget: {}\nstatus: {}\nprimary_score: {}\nrank_score: {}\nlog: {}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}\nbundle: {}\nstorage: {}",
                report.challenge_name,
                target.target,
                status_label(&target.result.status),
                format_score(target.result.primary_score),
                target
                    .result
                    .rank_score
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                target.log_path.display(),
                report.package.file_count,
                report.package.uncompressed_bytes,
                report.package.zip_bytes,
                report.package.workspace_dir.display(),
                report.bundle_dir.display(),
                report.storage_root.display()
            )),
            _ => {
                let rows = report
                    .targets
                    .iter()
                    .map(|target| {
                        vec![
                            target.target.to_string(),
                            status_label(&target.result.status),
                            format_score(target.result.primary_score),
                            target
                                .result
                                .rank_score
                                .map(format_score)
                                .unwrap_or_else(|| "none".to_string()),
                            target.log_path.display().to_string(),
                        ]
                    })
                    .collect::<Vec<_>>();
                Ok(format!(
                    "Local validation completed\nchallenge: {}\n{}\npackage: {} files, {} bytes uncompressed, {} bytes zipped\nworkspace: {}\nbundle: {}\nstorage: {}",
                    report.challenge_name,
                    render_table(&["TARGET", "STATUS", "PRIMARY", "RANK", "LOG"], &rows),
                    report.package.file_count,
                    report.package.uncompressed_bytes,
                    report.package.zip_bytes,
                    report.package.workspace_dir.display(),
                    report.bundle_dir.display(),
                    report.storage_root.display()
                ))
            }
        },
    }
}

/// Renders create submission batch for user-facing output.
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
            "solution_submission: {}\nlog_key: {}\ntruncated: {}\n\n{}",
            response.solution_submission_id,
            response
                .log_key
                .as_ref()
                .map_or("none", shared::storage::StorageKey::as_str),
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
                        item.official_score
                            .map(format_score)
                            .unwrap_or_else(|| "none".to_string()),
                        item.created_at.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(format!(
                "challenge: {challenge_name}\ntarget: {target}\ntotal_visible: {}\n{}",
                response.total_count,
                render_table(
                    &[
                        "ID",
                        "AGENT",
                        "TARGET",
                        "STATUS",
                        "RANK_SCORE",
                        "OFFICIAL_SCORE",
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
            let validation_score = submission
                .validation_evaluation
                .as_ref()
                .and_then(|evaluation| evaluation.primary_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let official_score = submission
                .official_evaluation
                .as_ref()
                .and_then(|evaluation| evaluation.primary_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let rank_score = submission
                .official_evaluation
                .as_ref()
                .or(submission.evaluation.as_ref())
                .and_then(|evaluation| evaluation.rank_score)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let metrics = submission
                .official_evaluation
                .as_ref()
                .or(submission.validation_evaluation.as_ref())
                .or(submission.evaluation.as_ref())
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
                "solution_submission: {}\nchallenge: {}\ntarget: {}\nagent: {}\nstatus: {}\ncreated_at: {}\nupdated_at: {}\nvalidation_primary_score: {}\nofficial_score: {}\nrank_score: {}\nrank: {}\ntotal_ranked: {}\npercentile: {}\nis_agent_best: {}\nmetrics:\n{}\nlogs: {}",
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
                validation_score,
                official_score,
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

/// Renders a target-scoped challenge statistics summary for agent iteration.
pub(crate) fn render_challenge_stats(
    challenge: &ChallengeDetailResponse,
    leaderboard: &LeaderboardResponse,
    distribution: &ScoreDistributionResponse,
    submissions: Option<&PublicSolutionSubmissionListResponse>,
    metric_name: &MetricName,
    format: OutputFormat,
) -> Result<String> {
    let visible_submission_count = submissions.map(|submission_list| submission_list.total_count);
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "challenge": challenge,
            "target": leaderboard.target,
            "metric_name": metric_name,
            "visible_submission_count": visible_submission_count,
            "ranked_agent_count": distribution.count,
            "leaderboard": leaderboard,
            "score_distribution": distribution,
        })),
        OutputFormat::Table => {
            let top_rows = leaderboard
                .items
                .iter()
                .take(5)
                .enumerate()
                .map(|(index, entry)| {
                    let rank = index
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("leaderboard rank overflow"))?;
                    Ok(vec![
                        rank.to_string(),
                        entry.agent_display_name.clone(),
                        entry.best_solution_submission_id.to_string(),
                        format_score(entry.best_rank_score),
                        entry.updated_at.clone(),
                    ])
                })
                .collect::<Result<Vec<_>>>()?;
            let median = quantile_value(distribution, 0.5)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let p90 = quantile_value(distribution, 0.9)
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            let best_score = challenge
                .spec
                .metric_schema
                .metric(metric_name)
                .map(|metric| metric.direction)
                .map_or(distribution.min, |direction| match direction {
                    MetricDirection::Maximize => distribution.max,
                    MetricDirection::Minimize => distribution.min,
                })
                .map(format_score)
                .unwrap_or_else(|| "none".to_string());
            Ok(format!(
                "challenge: {} ({})\ntarget: {}\nstatus: {}\nstarts_at: {}\ncloses_at: {}\neligibility: {}\nranking_metric: {}\nranked_agents: {}\nvisible_submissions: {}\nbest_score: {}\nmean_score: {}\nmedian_score: {}\np90_score: {}\ntop:\n{}",
                challenge.name,
                challenge.title,
                leaderboard.target,
                "published",
                challenge.spec.starts_at.as_deref().unwrap_or("none"),
                challenge.spec.closes_at.as_deref().unwrap_or("none"),
                status_label(&challenge.spec.eligibility.eligibility_type),
                metric_name,
                distribution.count,
                visible_submission_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "unavailable".to_string()),
                best_score,
                distribution
                    .mean
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                median,
                p90,
                render_table(
                    &["RANK", "AGENT", "SUBMISSION", "SCORE", "UPDATED"],
                    &top_rows
                )
            ))
        }
    }
}

/// Renders leaderboard for user-facing output.
pub(crate) fn render_leaderboard(
    response: &LeaderboardResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let rows = response
                .items
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let rank = index
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("leaderboard rank overflow"))?;
                    Ok(vec![
                        rank.to_string(),
                        entry.agent_display_name.clone(),
                        entry.best_solution_submission_id.to_string(),
                        format_score(entry.best_rank_score),
                        entry.updated_at.clone(),
                    ])
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!(
                "challenge: {}\ntarget: {}\n{}",
                response.challenge_name,
                response.target,
                render_table(&["RANK", "AGENT", "SUBMISSION", "SCORE", "UPDATED"], &rows)
            ))
        }
    }
}

/// Renders score distribution for user-facing output.
pub(crate) fn render_score_distribution(
    response: &ScoreDistributionResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let quantiles = response
                .quantiles
                .iter()
                .map(|quantile| {
                    vec![
                        format_score(quantile.quantile),
                        format_score(quantile.value),
                    ]
                })
                .collect::<Vec<_>>();
            let buckets = response
                .histogram
                .iter()
                .map(|bucket| {
                    vec![
                        format_score(bucket.lower),
                        format_score(bucket.upper),
                        bucket.count.to_string(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(format!(
                "challenge: {}\ntarget: {}\nmetric: {}\ncount: {}\nmin: {}\nmax: {}\nmean: {}\nquantiles:\n{}\nhistogram:\n{}",
                response.challenge_name,
                response.target,
                response.metric_name,
                response.count,
                response
                    .min
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                response
                    .max
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                response
                    .mean
                    .map(format_score)
                    .unwrap_or_else(|| "none".to_string()),
                render_table(&["Q", "VALUE"], &quantiles),
                render_table(&["LOWER", "UPPER", "COUNT"], &buckets)
            ))
        }
    }
}

/// Handles format targets for this module.
fn format_targets(targets: &[shared::models::challenge::ChallengeTargetSpec]) -> String {
    if targets.is_empty() {
        return "  <none>".to_string();
    }

    targets
        .iter()
        .map(|target| {
            format!(
                "  - {}: {} {}, image={}, timeout={} sec, memory={} MB, validation={}",
                target.name,
                target.docker_platform.as_str(),
                target.accelerator.as_str(),
                target.resource_profile.solution_image,
                target.resource_profile.timeout_sec,
                target.resource_profile.memory_limit_mb,
                if target.validation_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Handles pretty json for this module.
fn pretty_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

/// Handles status label for this module.
fn status_label<T: Serialize>(status: &T) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Handles format score for this module.
fn format_score(score: f64) -> String {
    if score.fract() == 0.0 {
        format!("{score:.0}")
    } else {
        format!("{score:.4}")
    }
}

/// Looks up an exact quantile value when the API returned it.
fn quantile_value(response: &ScoreDistributionResponse, expected: f64) -> Option<f64> {
    response
        .quantiles
        .iter()
        .find(|quantile| (quantile.quantile - expected).abs() < f64::EPSILON)
        .map(|quantile| quantile.value)
}

/// Renders table for user-facing output.
fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            rows.iter()
                .filter_map(|row| row.get(index))
                .map(|value| value.len())
                .max()
                .unwrap_or(0)
                .max(header.len())
        })
        .collect::<Vec<_>>();

    let mut lines = Vec::new();
    lines.push(render_table_row(
        &headers
            .iter()
            .map(|header| header.to_string())
            .collect::<Vec<_>>(),
        &widths,
    ));
    for row in rows {
        lines.push(render_table_row(row, &widths));
    }
    lines.join("\n")
}

/// Renders table row for user-facing output.
fn render_table_row(row: &[String], widths: &[usize]) -> String {
    row.iter()
        .enumerate()
        .map(|(index, value)| {
            let width = widths.get(index).copied().unwrap_or(value.len());
            format!("{value:<width$}")
        })
        .collect::<Vec<_>>()
        .join("  ")
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests;
