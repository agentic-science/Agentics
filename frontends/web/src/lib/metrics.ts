import { formatMetricValue } from "@/lib/format";
import type { ChallengeDetailResponse } from "@/lib/schemas";

/** Describes the metric schema shape used by this module. */
type MetricSchema = ChallengeDetailResponse["spec"]["metric_schema"];
/** Describes the metric definition shape used by this module. */
type MetricDefinition = MetricSchema["metrics"][number];
/** Describes the metric value shape used by this module. */
type MetricValue = { metric_name: string; value: number };

/** Find display metadata for a metric name declared by the challenge bundle. */
export function metricDefinition(
  schema: MetricSchema,
  metricName: string,
): MetricDefinition | undefined {
  return schema.metrics.find((metric) => metric.name === metricName);
}

/** Return the aggregate metric selected as the primary ranking metric. */
export function primaryMetric(
  schema: MetricSchema,
  metrics: MetricValue[],
): MetricValue | undefined {
  return metrics.find(
    (metric) => metric.metric_name === schema.ranking.primary_metric_name,
  );
}

/** Human-facing metric label with a safe fallback for unknown names. */
export function metricLabel(schema: MetricSchema, metricName: string): string {
  return metricDefinition(schema, metricName)?.label ?? metricName;
}

/** Format a metric value using bundle metadata when available. */
export function formatDeclaredMetric(
  schema: MetricSchema,
  metric: MetricValue | undefined,
): string {
  if (!metric) return "n/a";
  return formatMetricValue(
    metric.value,
    metricDefinition(schema, metric.metric_name)?.unit,
  );
}

/** Compact direction label for leaderboard headers and metric metadata. */
export function metricDirectionLabel(
  direction: "maximize" | "minimize",
): string {
  return direction === "maximize" ? "higher is better" : "lower is better";
}
