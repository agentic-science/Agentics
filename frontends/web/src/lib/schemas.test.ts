import { describe, expect, it } from "vitest";

import {
  challengeDetailResponseSchema,
  leaderboardResponseSchema,
  solutionSubmissionResponseSchema,
} from "./schemas";

describe("frontend API schemas", () => {
  it("accepts public challenge detail responses", () => {
    expect(() =>
      challengeDetailResponseSchema.parse({
        id: "sample-sum",
        slug: "sample-sum",
        title: "Sample Sum",
        description: "Add two numbers.",
        current_version: { id: "sample-sum:v1", version: "v1" },
        spec: {
          schema_version: 1,
          challenge_id: "sample-sum",
          challenge_title: "Sample Sum",
          challenge_version: "v1",
          solution: {
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
            public_dir: "public",
            private_benchmark_dir: "private-benchmark",
            public_policy: "full",
            private_benchmark_policy: "score_only",
            validation_enabled: true,
            private_benchmark_enabled: true,
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
      solutionSubmissionResponseSchema.parse({
        id: "sub-1",
        challenge_id: "sample-sum",
        challenge_title: "Sample Sum",
        challenge_version_id: "sample-sum:v1",
        agent_id: "agent-1",
        agent_name: "agent",
        status: "failed",
        explanation: "",
        parent_solution_submission_id: null,
        credit_text: "",
        visible_after_eval: false,
        evaluation: {
          id: "eval-1",
          status: "failed",
          eval_type: "validation",
          aggregate_metrics: [],
          run_metrics: [],
          public_results: [],
        },
        validation_evaluation: {
          id: "eval-1",
          status: "failed",
          eval_type: "validation",
          aggregate_metrics: [],
          run_metrics: [],
          public_results: [],
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
            best_solution_submission_id: "solution_submission-1",
            best_rank_score: -42,
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
