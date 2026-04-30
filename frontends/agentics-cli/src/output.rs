use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use shared::models::problem::{ProblemDetailResponse, ProblemListResponse};
use shared::models::request::RegisterAgentResponse;

use crate::cli::OutputFormat;
use crate::config::ResolvedSettings;

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

pub fn render_problem_list(response: &ProblemListResponse, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            if response.items.is_empty() {
                return Ok("No published problems found.".to_string());
            }

            let rows = response
                .items
                .iter()
                .map(|problem| {
                    vec![
                        problem.id.clone(),
                        problem.slug.clone(),
                        problem.current_version.version.clone(),
                        problem.title.clone(),
                    ]
                })
                .collect::<Vec<_>>();
            Ok(render_table(&["ID", "SLUG", "VERSION", "TITLE"], &rows))
        }
    }
}

pub fn render_problem_detail(
    response: &ProblemDetailResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => {
            let heldout = if response.spec.datasets.heldout_enabled {
                response
                    .spec
                    .datasets
                    .heldout_dir
                    .as_deref()
                    .unwrap_or("<configured>")
            } else {
                "disabled"
            };

            Ok(format!(
                "{} ({})\nversion: {} ({})\nsubmission: {} / {} / {}\nlimits: {} sec, {} MB\ndatasets: shown={}, hidden={}, heldout={}\n\n{}",
                response.title,
                response.id,
                response.current_version.version,
                response.current_version.id,
                response.spec.submission.format,
                response.spec.submission.language,
                response.spec.submission.entrypoint,
                response.spec.limits.time_limit_sec,
                response.spec.limits.memory_limit_mb,
                response.spec.datasets.shown_dir,
                response.spec.datasets.hidden_dir,
                heldout,
                response.statement_markdown.trim()
            ))
        }
    }
}

fn pretty_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
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

    let mut lines = Vec::with_capacity(rows.len() + 1);
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
    use shared::models::evaluation::ScoreVisibility;
    use shared::models::problem::{
        DatasetsSpec, LimitsSpec, ProblemBundleSpec, ProblemDetailResponse, ProblemListItemDto,
        ProblemListResponse, ScorerSpec, SubmissionSpec,
    };

    use super::{OutputFormat, render_problem_detail, render_problem_list};

    #[test]
    fn renders_problem_list_table() {
        let output = render_problem_list(
            &ProblemListResponse {
                items: vec![ProblemListItemDto {
                    id: "sample-sum".to_string(),
                    slug: "sum".to_string(),
                    title: "Sample Sum".to_string(),
                    description: "Add numbers".to_string(),
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
    fn renders_problem_detail_json() {
        let output = render_problem_detail(&problem_detail(), OutputFormat::Json)
            .expect("render should succeed");
        let parsed: Value = serde_json::from_str(&output).expect("JSON output should parse");

        assert_eq!(parsed["id"], "sample-sum");
        assert_eq!(parsed["spec"]["submission"]["entrypoint"], "main.py");
    }

    fn problem_detail() -> ProblemDetailResponse {
        ProblemDetailResponse {
            id: "sample-sum".to_string(),
            slug: "sum".to_string(),
            title: "Sample Sum".to_string(),
            description: "Add numbers".to_string(),
            current_version: CurrentVersionDto {
                id: "version-1".to_string(),
                version: "v1".to_string(),
            },
            spec: ProblemBundleSpec {
                schema_version: 1,
                problem_id: "sample-sum".to_string(),
                problem_title: "Sample Sum".to_string(),
                problem_version: "v1".to_string(),
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
                    shown_policy: ScoreVisibility::Full,
                    hidden_policy: "score_only".to_string(),
                    heldout_enabled: false,
                },
            },
            statement_markdown: "# Statement\n\nReturn the sum.".to_string(),
        }
    }
}
