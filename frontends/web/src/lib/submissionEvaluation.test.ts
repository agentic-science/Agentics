import { describe, expect, it } from "vitest";

import { selectSubmissionDisplayEvaluation } from "./submissionEvaluation";

/** Builds the evaluation test fixture. */
function evaluation(id: string) {
  return {
    id,
    target: "linux-arm64-cpu",
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
        validation_evaluation: evaluation("validation"),
        evaluation: evaluation("legacy"),
      })?.id,
    ).toBe("validation");

    expect(
      selectSubmissionDisplayEvaluation({
        evaluation: evaluation("legacy"),
      })?.id,
    ).toBe("legacy");
  });
});
