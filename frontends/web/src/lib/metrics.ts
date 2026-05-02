import { formatMetricValue } from "@/lib/format";
import type { ProblemDetailResponse } from "@/lib/schemas";

type MetricSchema = ProblemDetailResponse["spec"]["metric_schema"];
type MetricDefinition = MetricSchema["metrics"][number];
type MetricValue = { metric_id: string; value: number };

/** Find display metadata for a metric id declared by the problem bundle. */
export function metricDefinition(
  schema: MetricSchema,
  metricId: string,
): MetricDefinition | undefined {
  return schema.metrics.find((metric) => metric.id === metricId);
}

/** Return the aggregate metric selected as the primary ranking metric. */
export function primaryMetric(
  schema: MetricSchema,
  metrics: MetricValue[],
): MetricValue | undefined {
  return metrics.find(
    (metric) => metric.metric_id === schema.ranking.primary_metric_id,
  );
}

/** Human-facing metric label with a safe fallback for unknown legacy ids. */
export function metricLabel(schema: MetricSchema, metricId: string): string {
  return metricDefinition(schema, metricId)?.label ?? metricId;
}

/** Format a metric value using bundle metadata when available. */
export function formatDeclaredMetric(
  schema: MetricSchema,
  metric: MetricValue | undefined,
): string {
  if (!metric) return "n/a";
  return formatMetricValue(
    metric.value,
    metricDefinition(schema, metric.metric_id)?.unit,
  );
}

/** Compact direction label for leaderboard headers and metric metadata. */
export function metricDirectionLabel(
  direction: "maximize" | "minimize",
): string {
  return direction === "maximize" ? "higher is better" : "lower is better";
}
