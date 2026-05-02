import { describe, expect, it } from "vitest";

import {
  leaderboardResponseSchema,
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
            validation_enabled: true,
            heldout_enabled: true,
          },
          metric_schema: {
            metrics: [
              {
                id: "score",
                label: "Score",
                direction: "maximize",
                visibility: "public",
              },
              {
                id: "runtime_ms",
                label: "Runtime",
                unit: "ms",
                direction: "minimize",
                visibility: "official",
              },
            ],
            ranking: {
              primary_metric_id: "score",
              tie_breaker_metric_ids: ["runtime_ms"],
            },
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
          aggregate_metrics: [],
          run_metrics: [],
          shown_results: [],
        },
        public_evaluation: {
          id: "eval-1",
          status: "failed",
          eval_type: "validation",
          aggregate_metrics: [],
          run_metrics: [],
          shown_results: [],
        },
        created_at: "2026-04-28T00:00:00Z",
        updated_at: "2026-04-28T00:00:00Z",
      }),
    ).not.toThrow();
  });

  it("accepts structured leaderboard metrics", () => {
    expect(() =>
      leaderboardResponseSchema.parse({
        items: [
          {
            agent_id: "agent-1",
            agent_name: "solver",
            best_submission_id: "submission-1",
            best_hidden_score: -42,
            rank_score: -42,
            aggregate_metrics: [
              { metric_id: "latency_ms", value: 42 },
              { metric_id: "score", value: 0.9 },
            ],
            official_metrics: [{ metric_id: "latency_ms", value: 42 }],
            official_score: -42,
            updated_at: "2026-04-28T00:00:00Z",
          },
        ],
      }),
    ).not.toThrow();
  });
});
