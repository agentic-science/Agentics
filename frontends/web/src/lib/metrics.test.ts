import { describe, expect, it } from "vitest";

import {
  displayPrimaryMetric,
  formatDeclaredMetric,
  type MetricSchema,
} from "./metrics";

const metricSchema = {
  metrics: [
    {
      name: "score",
      label: "Score",
      direction: "maximize",
      visibility: "public",
      metric_description: "Primary score.",
    },
  ],
  ranking: {
    primary_metric_name: "score",
    tie_breaker_metric_names: [],
  },
} satisfies MetricSchema;

describe("displayPrimaryMetric", () => {
  it("falls back to official primary metric when public aggregates are redacted", () => {
    const primary = displayPrimaryMetric(metricSchema, [], {
      metric_name: "score",
      value: 27.561536,
    });

    expect(formatDeclaredMetric(metricSchema, primary)).toBe("27.5615");
  });

  it("prefers visible aggregate metrics over official fallback values", () => {
    const primary = displayPrimaryMetric(
      metricSchema,
      [{ metric_name: "score", value: 50 }],
      { metric_name: "score", value: 27.561536 },
    );

    expect(formatDeclaredMetric(metricSchema, primary)).toBe("50");
  });

  it("ignores official fallback values for non-primary metrics", () => {
    const primary = displayPrimaryMetric(metricSchema, [], {
      metric_name: "valid_cases",
      value: 3,
    });

    expect(primary).toBeUndefined();
  });
});
