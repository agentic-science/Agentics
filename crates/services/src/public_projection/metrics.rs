//! Shared metric projection helpers for public and audience-scoped surfaces.

use agentics_domain::models::challenge::{ChallengeBundleSpec, MetricVisibility};
use agentics_domain::models::evaluation::MetricValue;
use agentics_domain::models::names::MetricName;
use agentics_error::{Result, ServiceError};
use agentics_persistence::LeaderboardMetricEntry;

/// Return the declared primary metric from an official aggregate metric payload.
pub(super) fn official_primary_metric(
    official_metrics: &[MetricValue],
    spec: &ChallengeBundleSpec,
) -> Option<MetricValue> {
    MetricValue::find_by_name(
        official_metrics,
        &spec.metric_schema.ranking.primary_metric_name,
    )
}

/// Select the metric value that participates in one distribution.
pub(super) fn metric_value_from_leaderboard_entry(
    entry: &LeaderboardMetricEntry,
    metric_name: &MetricName,
    spec: &ChallengeBundleSpec,
) -> Option<f64> {
    match metric_name.as_str() {
        "rank_score" | "best_rank_score" => Some(entry.best_rank_score),
        _ if metric_name == &spec.metric_schema.ranking.primary_metric_name => {
            metric_value_by_name(&entry.official_metrics, metric_name)
                .or_else(|| metric_value_by_name(&entry.aggregate_metrics, metric_name))
        }
        _ => None,
    }
}

/// Reject distribution requests that would require private aggregate metrics.
pub(super) fn ensure_metric_is_publicly_distributable(
    metric_name: &MetricName,
    spec: &ChallengeBundleSpec,
) -> Result<()> {
    if matches!(metric_name.as_str(), "rank_score" | "best_rank_score") {
        return Ok(());
    }

    if metric_name == &spec.metric_schema.ranking.primary_metric_name
        && spec
            .metric_schema
            .metric(metric_name)
            .is_some_and(|metric| metric.visibility == MetricVisibility::Public)
    {
        return Ok(());
    }

    Err(ServiceError::Forbidden(
        "score distribution is available only for rank_score, best_rank_score, or the public primary ranking metric"
            .to_string(),
    ))
}

/// Find one metric by name in an evaluator aggregate metric payload.
fn metric_value_by_name(metrics: &[MetricValue], metric_name: &MetricName) -> Option<f64> {
    metrics
        .iter()
        .find(|metric| &metric.metric_name == metric_name)
        .map(|metric| metric.value)
}
