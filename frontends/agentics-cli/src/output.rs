use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use shared::models::challenge::{ChallengeDetailResponse, ChallengeListResponse};
use shared::models::challenge_creation::{
    ChallengeDraftCleanupResponse, ChallengeDraftResponse, ChallengePrivateAssetResponse,
    GithubIdentityResponse,
};
use shared::models::request::{
    CreateSolutionSubmissionResponse, RegisterAgentResponse, SolutionSubmissionResponse,
};

use crate::cli::OutputFormat;
use crate::config::ResolvedSettings;
use crate::package::SolutionPackage;
use crate::workspace::InitSolutionSummary;

pub fn render_register_agent(
    response: &RegisterAgentResponse,
    saved_token: bool,
    settings: &ResolvedSettings,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "agent_id": response.agent_id,
            "name": response.name,
            "token": response.token,
            "created_at": response.created_at,
            "saved_token": saved_token,
            "config_path": settings.config_path,
            "api_base_url": settings.api_base_url,
        })),
        OutputFormat::Table => Ok(format!(
            "Registered agent {}\nagent_id: {}\ntoken: {}\nsaved_token: {}\nconfig: {}",
            response.name,
            response.agent_id,
            response.token,
            saved_token,
            settings.config_path.display()
        )),
    }
}

pub fn render_auth_status(settings: &ResolvedSettings, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(&json!({
            "api_base_url": settings.api_base_url,
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

pub fn render_config_set(
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

pub fn render_github_identity(
    response: &GithubIdentityResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "linked_github_identity: {}\ngithub_user_id: {}\nagent_id: {}",
            response.github_login, response.github_user_id, response.agent_id
        )),
    }
}

pub fn render_challenge_draft(
    response: &ChallengeDraftResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "challenge_draft: {}\nchallenge: {}\nrequest: {}\nstatus: {}\nrepo: {}#{}\npath: {}\ncommit: {}\nmanifest_sha256: {}\npublished_version: {}\nprivate_assets: {}\nvalidation_records: {}",
            response.id,
            response.challenge_id,
            status_label(&response.request),
            status_label(&response.status),
            response.repo_url,
            response.pr_number,
            response.challenge_path,
            response.commit_sha,
            response.manifest_sha256,
            response
                .published_challenge_version_id
                .as_deref()
                .unwrap_or("none"),
            response.private_assets.len(),
            response.validation_records.len()
        )),
    }
}

pub fn render_challenge_private_asset(
    response: &ChallengePrivateAssetResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "private_asset: {}\ndraft: {}\nasset_id: {}\nkind: {}\nrequired: {}\nsize_bytes: {}\nsha256: {}",
            response.id,
            response.draft_id,
            response.asset_id,
            status_label(&response.kind),
            response.required,
            response.size_bytes,
            response.sha256
        )),
    }
}

pub fn render_challenge_draft_cleanup(
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

pub fn render_challenge_list(
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
                        challenge.id.clone(),
                        challenge.slug.clone(),
                        challenge.current_version.version.clone(),
                        challenge.title.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(render_table(&["ID", "SLUG", "VERSION", "TITLE"], &rows))
        }
    }
}

pub fn render_challenge_detail(
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
                    .as_deref()
                    .unwrap_or("<configured>")
            } else {
                "disabled"
            };

            Ok(format!(
                "{} ({})\nsummary: {}\nversion: {} ({})\nsolution_protocol: {} ({})\nbenchmark_targets:\n{}\ndatasets: public={}, private_benchmark={}\nranking_metric: {}\n\n{}",
                response.title,
                response.id,
                response.summary,
                response.current_version.version,
                response.current_version.id,
                response.spec.solution.protocol,
                response.spec.solution.manifest_file,
                format_benchmark_targets(&response.spec.benchmark_targets),
                response.spec.datasets.public_dir,
                private_benchmark,
                response.spec.metric_schema.ranking.primary_metric_id,
                response.statement_markdown.trim()
            ))
        }
    }
}

pub fn render_init_solution(summary: &InitSolutionSummary, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(summary),
        OutputFormat::Table => Ok(format!(
            "Initialized solution workspace: {}\nchallenge: {} ({})\nversion: {}\nruntime_profile: {}\ninterface: {}",
            summary.workspace_dir.display(),
            summary.challenge_title,
            summary.challenge_id,
            summary.challenge_version,
            summary.runtime_profile,
            summary.interface
        )),
    }
}

pub fn render_create_solution_submission(
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
            response.challenge_id,
            response.benchmark_target_id,
            response.status,
            response.evaluation_job_id,
            package.file_count,
            package.uncompressed_bytes,
            package.bytes.len(),
            package.workspace_dir.display()
        )),
    }
}

pub fn render_create_solution_submission_batch(
    responses: &[CreateSolutionSubmissionResponse],
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match responses {
        [response] => render_create_solution_submission(response, package, format),
        _ => render_create_submission_batch("solution_submissions", responses, package, format),
    }
}

pub fn render_create_validation_run(
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
            response.challenge_id,
            response.benchmark_target_id,
            response.status,
            response.evaluation_job_id,
            package.file_count,
            package.uncompressed_bytes,
            package.bytes.len(),
            package.workspace_dir.display()
        )),
    }
}

pub fn render_create_validation_run_batch(
    responses: &[CreateSolutionSubmissionResponse],
    package: &SolutionPackage,
    format: OutputFormat,
) -> Result<String> {
    match responses {
        [response] => render_create_validation_run(response, package, format),
        _ => render_create_submission_batch("validation_runs", responses, package, format),
    }
}

pub fn render_solution_submission_status(
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

            Ok(format!(
                "solution submission: {}\nchallenge: {}\ntarget: {}\nstatus: {}\nevaluation_job: {}\nvalidation_evaluation: {}\nofficial_evaluation: {}\nrank_score: {}\nvisible_after_eval: {}",
                response.id,
                response.challenge_id,
                response.benchmark_target_id,
                response.status,
                evaluation_job,
                validation_eval,
                official_eval,
                rank_score,
                response.visible_after_eval
            ))
        }
    }
}

pub fn render_validation_run_status(
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
                response.challenge_id,
                response.benchmark_target_id,
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

pub fn render_validation_run_status_batch(
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
                            response.benchmark_target_id.clone(),
                            response.id.clone(),
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
            let mut value = json!({
                "package": {
                    "workspace_dir": package.workspace_dir,
                    "file_count": package.file_count,
                    "uncompressed_bytes": package.uncompressed_bytes,
                    "zip_bytes": package.bytes.len(),
                }
            });
            if let Some(object) = value.as_object_mut() {
                object.insert(response_key.to_string(), serde_json::to_value(responses)?);
            }
            pretty_json(&value)
        }
        OutputFormat::Table => {
            let rows = responses
                .iter()
                .map(|response| {
                    vec![
                        response.benchmark_target_id.clone(),
                        response.id.clone(),
                        response.challenge_id.clone(),
                        response.status.clone(),
                        response.evaluation_job_id.clone(),
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

fn format_benchmark_targets(targets: &[shared::models::challenge::BenchmarkTargetSpec]) -> String {
    if targets.is_empty() {
        return "  <none>".to_string();
    }

    targets
        .iter()
        .map(|target| {
            format!(
                "  - {}: {} {}, image={}, timeout={} sec, memory={} MB, validation={}",
                target.id,
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

fn pretty_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

fn status_label<T: Serialize>(status: &T) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_score(score: f64) -> String {
    if score.fract() == 0.0 {
        format!("{score:.0}")
    } else {
        format!("{score:.4}")
    }
}

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
mod tests {
    use serde_json::Value;
    use shared::models::CurrentVersionDto;
    use shared::models::challenge::{
        BenchmarkAccelerator, BenchmarkTargetSpec, ChallengeBundleSpec, ChallengeDetailResponse,
        ChallengeExecutionSpec, ChallengeListItemDto, ChallengeListResponse, DatasetsSpec,
        DockerPlatform, MetricSchemaSpec, ResourceProfileSpec, ScorerSpec, SolutionSpec,
    };
    use shared::models::evaluation::ScoreVisibility;
    use shared::zip_project::ZipProjectNetworkAccess;

    use super::{OutputFormat, render_challenge_detail, render_challenge_list};

    #[test]
    fn renders_challenge_list_table() {
        let output = render_challenge_list(
            &ChallengeListResponse {
                items: vec![ChallengeListItemDto {
                    id: "sample-sum".to_string(),
                    slug: "sum".to_string(),
                    title: "Sample Sum".to_string(),
                    summary: "Add numbers".to_string(),
                    current_version: CurrentVersionDto {
                        id: "version-1".to_string(),
                        version: "v1".to_string(),
                    },
                }],
            },
            OutputFormat::Table,
        )
        .expect("render should succeed");

        assert_eq!(
            output,
            "ID          SLUG  VERSION  TITLE\nsample-sum  sum   v1       Sample Sum"
        );
    }

    #[test]
    fn renders_challenge_detail_json() {
        let output = render_challenge_detail(&challenge_detail(), OutputFormat::Json)
            .expect("render should succeed");
        let parsed: Value = serde_json::from_str(&output).expect("JSON output should parse");

        assert_eq!(parsed["id"], "sample-sum");
        assert_eq!(parsed["spec"]["solution"]["protocol"], "zip_project");
    }

    fn challenge_detail() -> ChallengeDetailResponse {
        ChallengeDetailResponse {
            id: "sample-sum".to_string(),
            slug: "sum".to_string(),
            title: "Sample Sum".to_string(),
            summary: "Add numbers".to_string(),
            current_version: CurrentVersionDto {
                id: "version-1".to_string(),
                version: "v1".to_string(),
            },
            spec: ChallengeBundleSpec {
                schema_version: 1,
                challenge_id: "sample-sum".to_string(),
                challenge_title: "Sample Sum".to_string(),
                challenge_summary: "Add numbers".to_string(),
                challenge_version: "v1".to_string(),
                solution: SolutionSpec {
                    protocol: "zip_project".to_string(),
                    manifest_file: "agentics.solution.json".to_string(),
                },
                scorer: ScorerSpec {
                    command: vec!["python".to_string(), "scorer/run.py".to_string()],
                    result_file: "result.json".to_string(),
                },
                benchmark_targets: vec![BenchmarkTargetSpec {
                    id: "cpu-linux-arm64".to_string(),
                    docker_platform: DockerPlatform::LinuxArm64,
                    accelerator: BenchmarkAccelerator::Cpu,
                    validation_enabled: false,
                    resource_profile: ResourceProfileSpec {
                        id: "python-cpu-small".to_string(),
                        resource_description: None,
                        solution_image: "python:3.12-slim-bookworm".to_string(),
                        solution_image_digest: None,
                        scorer_image: "python:3.12-slim-bookworm".to_string(),
                        scorer_image_digest: None,
                        timeout_sec: 30,
                        memory_limit_mb: 512,
                        cpu_limit_millis: 1000,
                        disk_limit_mb: 1024,
                        setup_network_access: ZipProjectNetworkAccess::Enabled,
                        build_network_access: ZipProjectNetworkAccess::Disabled,
                        run_network_access: ZipProjectNetworkAccess::Disabled,
                        scorer_network_access: ZipProjectNetworkAccess::Disabled,
                        hardware: None,
                    },
                }],
                execution: ChallengeExecutionSpec {
                    validation_runs: Some("public/runs.json".to_string()),
                    official_runs: Some("private-benchmark/runs.json".to_string()),
                },
                datasets: DatasetsSpec {
                    public_dir: "data/public".to_string(),
                    private_benchmark_dir: None,
                    public_policy: ScoreVisibility::Full,
                    private_benchmark_policy: "score_only".to_string(),
                    private_benchmark_enabled: false,
                },
                community: None,
                metric_schema: MetricSchemaSpec::default(),
            },
            statement_markdown: "# Statement\n\nReturn the sum.".to_string(),
        }
    }
}
