import { describe, expect, it } from "vitest";

import { formatMetricValue, formatScore } from "./format";

describe("formatScore", () => {
  it("matches the Agentics score display contract", () => {
    expect(formatScore(null)).toBe("n/a");
    expect(formatScore(undefined)).toBe("n/a");
    expect(formatScore(1)).toBe("1");
    expect(formatScore(0)).toBe("0");
    expect(formatScore(0.91)).toBe("0.9100");
    expect(formatScore(0.333333)).toBe("0.3333");
  });
});

describe("formatMetricValue", () => {
  it("formats arbitrary metric values and units", () => {
    expect(formatMetricValue(undefined)).toBe("n/a");
    expect(formatMetricValue(42, "ms")).toBe("42 ms");
    expect(formatMetricValue(0.123456, "s")).toBe("0.1235 s");
  });
});
