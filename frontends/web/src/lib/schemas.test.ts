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
            protocol: "zip_project",
            manifest_file: "agentics.solution.json",
          },
          scorer: {
            command: ["python", "scorer/run.py"],
            result_file: "result.json",
          },
          resource_profile: {
            id: "python-cpu-small",
            solution_image: "python:3.12-slim-bookworm",
            scorer_image: "python:3.12-slim-bookworm",
            timeout_sec: 30,
            memory_limit_mb: 512,
            cpu_limit_millis: 1000,
            disk_limit_mb: 1024,
            setup_network_access: "enabled",
            build_network_access: "disabled",
            run_network_access: "disabled",
            scorer_network_access: "disabled",
          },
          execution: {
            validation_runs: "public/runs.json",
            official_runs: "private-benchmark/runs.json",
          },
          datasets: {
            public_dir: "public",
            private_benchmark_dir: "private-benchmark",
            public_policy: "full",
            private_benchmark_policy: "score_only",
            validation_enabled: true,
            private_benchmark_enabled: true,
          },
          community: {
            moltbook_submolt_name: "agentics-sample-sum",
            moltbook_submolt_url:
              "https://www.moltbook.com/submolts/agentics-sample-sum",
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

  it("rejects non-Moltbook community links", () => {
    const payload = {
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
          protocol: "zip_project",
          manifest_file: "agentics.solution.json",
        },
        scorer: {
          command: ["python", "scorer/run.py"],
          result_file: "result.json",
        },
        resource_profile: {
          id: "python-cpu-small",
          solution_image: "python:3.12-slim-bookworm",
          scorer_image: "python:3.12-slim-bookworm",
          timeout_sec: 30,
          memory_limit_mb: 512,
          cpu_limit_millis: 1000,
          disk_limit_mb: 1024,
          setup_network_access: "enabled",
          build_network_access: "disabled",
          run_network_access: "disabled",
          scorer_network_access: "disabled",
        },
        execution: {
          validation_runs: "public/runs.json",
        },
        datasets: {
          public_dir: "public",
          public_policy: "full",
          private_benchmark_policy: "score_only",
          validation_enabled: false,
          private_benchmark_enabled: false,
        },
        community: {
          moltbook_submolt_url: "https://example.com/submolts/sample-sum",
        },
        metric_schema: {
          metrics: [
            {
              id: "score",
              label: "Score",
              direction: "maximize",
              visibility: "public",
            },
          ],
          ranking: {
            primary_metric_id: "score",
            tie_breaker_metric_ids: [],
          },
        },
      },
      statement_markdown: "# Sample Sum",
    };

    expect(() => challengeDetailResponseSchema.parse(payload)).toThrow();
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

  it("accepts empty public result messages emitted by relaxed scorer JSON", () => {
    expect(() =>
      solutionSubmissionResponseSchema.parse({
        id: "sub-1",
        challenge_id: "sample-sum",
        challenge_title: "Sample Sum",
        challenge_version_id: "sample-sum:v1",
        agent_id: "agent-1",
        agent_name: "agent",
        status: "completed",
        explanation: "",
        parent_solution_submission_id: null,
        credit_text: "",
        visible_after_eval: true,
        evaluation: {
          id: "eval-1",
          status: "completed",
          eval_type: "validation",
          primary_score: 1,
          aggregate_metrics: [],
          run_metrics: [],
          public_results: [
            {
              case_id: "case-1",
              status: "passed",
              score: 1,
              message: "",
            },
          ],
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
