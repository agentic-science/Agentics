import { describe, expect, it } from "vitest";

import {
  problemDetailResponseSchema,
  submissionResponseSchema,
} from "./schemas";

describe("frontend API schemas", () => {
  it("accepts public problem detail responses", () => {
    expect(() =>
      problemDetailResponseSchema.parse({
        id: "sample-sum",
        slug: "sample-sum",
        title: "Sample Sum",
        description: "Add two numbers.",
        current_version: { id: "sample-sum:v1", version: "v1" },
        spec: {
          schema_version: 1,
          problem_id: "sample-sum",
          problem_title: "Sample Sum",
          problem_version: "v1",
          submission: {
            format: "python_zip_project",
            language: "python",
            entrypoint: "main.py",
          },
          scorer: {
            entrypoint: "scorer/run.py",
            result_file: "result.json",
          },
          limits: {
            time_limit_sec: 2,
            memory_limit_mb: 128,
          },
          datasets: {
            shown_dir: "shown",
            hidden_dir: "hidden",
            heldout_dir: "heldout",
            shown_policy: "full",
            hidden_policy: "score_only",
            heldout_enabled: true,
          },
        },
        statement_markdown: "# Sample Sum",
      }),
    ).not.toThrow();
  });

  it("accepts relaxed omission of nullable evaluation fields", () => {
    expect(() =>
      submissionResponseSchema.parse({
        id: "sub-1",
        problem_id: "sample-sum",
        problem_title: "Sample Sum",
        problem_version_id: "sample-sum:v1",
        agent_id: "agent-1",
        agent_name: "agent",
        status: "failed",
        explanation: "",
        parent_submission_id: null,
        credit_text: "",
        visible_after_eval: false,
        evaluation: {
          id: "eval-1",
          status: "failed",
          eval_type: "validation",
          shown_results: [],
        },
        public_evaluation: {
          id: "eval-1",
          status: "failed",
          eval_type: "validation",
          shown_results: [],
        },
        created_at: "2026-04-28T00:00:00Z",
        updated_at: "2026-04-28T00:00:00Z",
      }),
    ).not.toThrow();
  });
});
