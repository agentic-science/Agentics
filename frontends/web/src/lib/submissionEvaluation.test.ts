import { describe, expect, it } from "vitest";

import { selectSubmissionDisplayEvaluation } from "./submissionEvaluation";

function evaluation(id: string) {
  return {
    id,
    benchmark_target_id: "cpu-linux-arm64",
    status: "completed" as const,
    eval_type: "official" as const,
    aggregate_metrics: [],
    run_metrics: [],
    public_results: [],
  };
}

describe("selectSubmissionDisplayEvaluation", () => {
  it("prefers official results for public submission details", () => {
    const selected = selectSubmissionDisplayEvaluation({
      official_evaluation: evaluation("official"),
      validation_evaluation: evaluation("validation"),
      evaluation: evaluation("legacy"),
    });

    expect(selected?.id).toBe("official");
  });

  it("falls back to validation and legacy evaluation fields", () => {
    expect(
      selectSubmissionDisplayEvaluation({
        official_evaluation: null,
        validation_evaluation: evaluation("validation"),
        evaluation: evaluation("legacy"),
      })?.id,
    ).toBe("validation");

    expect(
      selectSubmissionDisplayEvaluation({
        official_evaluation: null,
        validation_evaluation: null,
        evaluation: evaluation("legacy"),
      })?.id,
    ).toBe("legacy");
  });
});
