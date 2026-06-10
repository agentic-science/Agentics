use agentics_domain::models::evaluation::{EvaluationDto, MetricValue};
use agentics_domain::models::names::MetricName;
use agentics_domain::models::request::ScoreDistributionResponse;
use anyhow::Result;
use serde::Serialize;

pub(super) fn pretty_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

pub(super) fn status_label<T: Serialize>(status: &T) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn format_score(score: f64) -> String {
    if score.fract() == 0.0 {
        format!("{score:.0}")
    } else {
        format!("{score:.4}")
    }
}

pub(super) fn format_metric(metric: &MetricValue) -> String {
    format!("{}={}", metric.metric_name, format_score(metric.value))
}

pub(super) fn format_optional_metric(metric: Option<&MetricValue>) -> String {
    metric
        .map(format_metric)
        .unwrap_or_else(|| "none".to_string())
}

pub(super) fn format_warnings(warnings: &[String]) -> String {
    if warnings.is_empty() {
        return "none".to_string();
    }
    warnings
        .iter()
        .map(|warning| format!("- {warning}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn aggregate_metric_by_name<'a>(
    evaluation: &'a EvaluationDto,
    metric_name: &MetricName,
) -> Option<&'a MetricValue> {
    evaluation
        .aggregate_metrics
        .iter()
        .find(|metric| &metric.metric_name == metric_name)
}

pub(super) fn quantile_value(response: &ScoreDistributionResponse, expected: f64) -> Option<f64> {
    response
        .quantiles
        .iter()
        .find(|quantile| (quantile.quantile - expected).abs() < f64::EPSILON)
        .map(|quantile| quantile.value)
}

pub(super) fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
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
