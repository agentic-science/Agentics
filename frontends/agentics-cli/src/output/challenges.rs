use agentics_domain::models::challenge::{
    ChallengeDetailResponse, ChallengeListResponse, ChallengeTargetSpec, MetricDirection,
    PublicChallengeExecutionSpec,
};
use agentics_domain::models::names::ChallengeKeyword;
use anyhow::Result;

use super::OutputFormat;
use super::format::{format_score, pretty_json, quantile_value, render_table, status_label};

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
                        challenge.challenge_name.to_string(),
                        status_label(&challenge.eligibility.eligibility_type),
                        format_keywords(&challenge.keywords),
                        challenge.title.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(render_table(
                &["NAME", "ELIGIBILITY", "KEYWORDS", "TITLE"],
                &rows,
            ))
        }
    }
}

/// Renders challenge detail for user-facing output.
pub(crate) fn render_challenge_detail(
    response: &ChallengeDetailResponse,
    format: OutputFormat,
) -> Result<String> {
    validate_challenge_detail_topology(response)?;
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let private_benchmark = if response.spec.datasets.private_benchmark_enabled {
                "<configured>"
            } else {
                "disabled"
            };
            let execution = &response.spec.execution;
            let trusted_executor_label = match execution {
                PublicChallengeExecutionSpec::SeparatedEvaluator(_) => "separated-evaluator",
                PublicChallengeExecutionSpec::PipedStdio(_) => "interactive-evaluator",
                PublicChallengeExecutionSpec::CoexecutedBenchmark(_) => "coexecuted-evaluator",
            };
            let trust_boundary_note = coexecuted_trust_boundary_note(execution);
            let discussion_url = response
                .moltbook
                .discussion_url
                .as_ref()
                .map(|url| url.as_str())
                .unwrap_or("none");
            Ok(format!(
                "{} ({})\nsummary: {}\nkeywords: {}\nstarts_at: {}\ncloses_at: {}\neligibility: {}\nmoltbook_submolt: {} ({})\nmoltbook_discussion: {}\nleaderboard_visibility: {}\nscore_distribution_visibility: {}\nresult_detail_visibility: {}\nsolution_publication: {}\nsolution_protocol: {} ({})\nexecution_mode: {}\n{}: command={}, result_file={}{}targets:\n{}\ndatasets: public={}, private_benchmark={}\nranking_metric: {}\n\n{}",
                response.title,
                response.challenge_name,
                response.summary.en,
                format_keywords(&response.keywords),
                response.spec.starts_at.as_str(),
                response.spec.closes_at.as_deref().unwrap_or("none"),
                status_label(&response.spec.eligibility.eligibility_type),
                response.moltbook.submolt_name,
                response.moltbook.submolt_url,
                discussion_url,
                status_label(&response.spec.visibility.leaderboard),
                status_label(&response.spec.visibility.score_distribution),
                status_label(&response.spec.visibility.result_detail),
                status_label(&response.spec.solution_publication),
                response.spec.solution.protocol,
                response.spec.solution.manifest_file,
                execution_mode_label(execution),
                trusted_executor_label,
                execution.trusted_evaluator().command.join(" "),
                execution.trusted_evaluator().result_file,
                trust_boundary_note,
                format_targets(&response.spec.targets),
                response.spec.datasets.public_dir,
                private_benchmark,
                response.spec.metric_schema.ranking.primary_metric_name,
                response.statement_markdown.trim()
            ))
        }
    }
}

pub(super) fn validate_challenge_detail_topology(response: &ChallengeDetailResponse) -> Result<()> {
    let coexecuted = matches!(
        response.spec.execution,
        PublicChallengeExecutionSpec::CoexecutedBenchmark(_)
    );
    for target in &response.spec.targets {
        let has_solution_run = target.resource_profile.solution.run.is_some();
        if coexecuted && has_solution_run {
            anyhow::bail!(
                "invalid challenge DTO: solution.run is forbidden for coexecuted_benchmark execution"
            );
        }
        if !coexecuted && !has_solution_run {
            anyhow::bail!(
                "invalid challenge DTO: solution.run is required for {} execution",
                execution_mode_label(&response.spec.execution)
            );
        }
    }
    Ok(())
}

fn coexecuted_trust_boundary_note(execution: &PublicChallengeExecutionSpec) -> &'static str {
    match execution {
        PublicChallengeExecutionSpec::CoexecutedBenchmark(_) => {
            "\ntrust_boundary: coexecuted-evaluator and participant workspace share the evaluator container; official private data shares that boundary\n"
        }
        PublicChallengeExecutionSpec::SeparatedEvaluator(_)
        | PublicChallengeExecutionSpec::PipedStdio(_) => "\n",
    }
}

pub(super) fn format_targets(targets: &[ChallengeTargetSpec]) -> String {
    if targets.is_empty() {
        return "  <none>".to_string();
    }

    targets
        .iter()
        .map(|target| {
            let solution_run = &target.resource_profile.solution.run;
            let evaluator_run = &target.resource_profile.evaluator.run;
            let solution_run_summary = solution_run
                .as_ref()
                .map(|limits| format!("{} sec/{} MB", limits.timeout_sec, limits.memory_limit_mb))
                .unwrap_or_else(|| "not used".to_string());
            format!(
                "  - {}: {} {}, profile={}, solution_image={}, evaluator_image={}, solution_run={}, evaluator_run={} sec/{} MB, validation={}",
                target.name,
                target.docker_platform.as_str(),
                target.accelerator.as_str(),
                target.resource_profile.name,
                target.resource_profile.solution_image,
                target.resource_profile.evaluator_image,
                solution_run_summary,
                evaluator_run.timeout_sec,
                evaluator_run.memory_limit_mb,
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

pub(super) fn format_keywords(keywords: &[ChallengeKeyword]) -> String {
    if keywords.is_empty() {
        "none".to_string()
    } else {
        keywords
            .iter()
            .map(ChallengeKeyword::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(super) fn execution_mode_label(execution: &PublicChallengeExecutionSpec) -> &'static str {
    match execution {
        PublicChallengeExecutionSpec::SeparatedEvaluator(_) => "separated_evaluator",
        PublicChallengeExecutionSpec::PipedStdio(_) => "piped_stdio",
        PublicChallengeExecutionSpec::CoexecutedBenchmark(_) => "coexecuted_benchmark",
    }
}

pub(crate) fn best_challenge_score(
    challenge: &ChallengeDetailResponse,
    metric_name: &agentics_domain::models::names::MetricName,
    distribution: &agentics_domain::models::request::ScoreDistributionResponse,
) -> String {
    challenge
        .spec
        .metric_schema
        .metric(metric_name)
        .map(|metric| metric.direction)
        .map_or(distribution.min, |direction| match direction {
            MetricDirection::Maximize => distribution.max,
            MetricDirection::Minimize => distribution.min,
        })
        .map(format_score)
        .unwrap_or_else(|| "none".to_string())
}

pub(crate) fn median_and_p90(
    distribution: &agentics_domain::models::request::ScoreDistributionResponse,
) -> (String, String) {
    let median = quantile_value(distribution, 0.5)
        .map(format_score)
        .unwrap_or_else(|| "none".to_string());
    let p90 = quantile_value(distribution, 0.9)
        .map(format_score)
        .unwrap_or_else(|| "none".to_string());
    (median, p90)
}
