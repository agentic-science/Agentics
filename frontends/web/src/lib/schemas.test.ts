import { describe, expect, it } from "vitest";

import adminCapacityResponseFixture from "./__fixtures__/dto-contracts/admin-capacity-response.json";
import challengeDetailResponseFixture from "./__fixtures__/dto-contracts/challenge-detail-response.json";
import officialSolutionSubmissionResponseFixture from "./__fixtures__/dto-contracts/solution-submission-response-official.json";
import {
  adminCapacityResponseSchema,
  adminChallengeListResponseSchema,
  challengeDetailResponseSchema,
  leaderboardResponseSchema,
  solutionSubmissionResponseSchema,
} from "./schemas";

function targetFixture(validationEnabled: boolean) {
  return {
    name: "linux-arm64-cpu",
    docker_platform: "linux/arm64",
    accelerator: "cpu",
    validation_enabled: validationEnabled,
    resource_profile: {
      name: "python-cpu-small",
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
  };
}

const challengePolicy = {
  eligibility: { type: "open" },
  visibility: {
    leaderboard: "public_live",
    score_distribution: "public_live",
    result_detail: "submitter_live_public_live",
  },
  solution_publication: "public",
};

describe("frontend API schemas", () => {
  it("accepts Rust-serialized DTO contract fixtures", () => {
    expect(() =>
      challengeDetailResponseSchema.parse(challengeDetailResponseFixture),
    ).not.toThrow();
    expect(() =>
      solutionSubmissionResponseSchema.parse(
        officialSolutionSubmissionResponseFixture,
      ),
    ).not.toThrow();
    expect(() =>
      adminCapacityResponseSchema.parse(adminCapacityResponseFixture),
    ).not.toThrow();
  });

  it("accepts public challenge detail responses", () => {
    expect(() =>
      challengeDetailResponseSchema.parse({
        name: "sample-sum",
        title: "Sample Sum",
        summary: "Add two numbers.",
        spec: {
          schema_version: 1,
          challenge_name: "sample-sum",
          challenge_title: "Sample Sum",
          challenge_summary: "Add two numbers.",
          ...challengePolicy,
          solution: {
            protocol: "zip_project",
            manifest_file: "agentics.solution.json",
          },
          scorer: {
            command: ["python", "scorer/run.py"],
            result_file: "result.json",
          },
          targets: [targetFixture(true)],
          execution: {
            validation_runs: "public/runs.json",
            official_prepare: {
              command: ["python", "scorer/prepare.py"],
              result_runs_file: "generated/runs.json",
              network_access: "enabled",
              reproducibility_notes: "Generated from private seeds.",
              external_data: [
                {
                  url: "https://example.com/dataset-v1.tar.zst",
                  digest: "sha256:abc",
                  version: "v1",
                },
              ],
            },
          },
          datasets: {
            public_dir: "public",
            private_benchmark_dir: "private-benchmark",
            public_policy: "full",
            private_benchmark_policy: "score_only",
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
                name: "score",
                label: "Score",
                direction: "maximize",
                visibility: "public",
              },
              {
                name: "runtime_ms",
                label: "Runtime",
                unit: "ms",
                direction: "minimize",
                visibility: "official",
              },
            ],
            ranking: {
              primary_metric_name: "score",
              tie_breaker_metric_names: ["runtime_ms"],
            },
          },
        },
        statement_markdown: "# Sample Sum",
      }),
    ).not.toThrow();
  });

  it("rejects non-Moltbook community links", () => {
    const payload = {
      name: "sample-sum",
      title: "Sample Sum",
      summary: "Add two numbers.",
      spec: {
        schema_version: 1,
        challenge_name: "sample-sum",
        challenge_title: "Sample Sum",
        challenge_summary: "Add two numbers.",
        ...challengePolicy,
        solution: {
          protocol: "zip_project",
          manifest_file: "agentics.solution.json",
        },
        scorer: {
          command: ["python", "scorer/run.py"],
          result_file: "result.json",
        },
        targets: [targetFixture(false)],
        execution: {
          validation_runs: "public/runs.json",
        },
        datasets: {
          public_dir: "public",
          public_policy: "full",
          private_benchmark_policy: "score_only",
          private_benchmark_enabled: false,
        },
        community: {
          moltbook_submolt_url: "https://example.com/submolts/sample-sum",
        },
        metric_schema: {
          metrics: [
            {
              name: "score",
              label: "Score",
              direction: "maximize",
              visibility: "public",
            },
          ],
          ranking: {
            primary_metric_name: "score",
            tie_breaker_metric_names: [],
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
        id: "11111111-1111-4111-8111-111111111111",
        challenge_name: "sample-sum",
        challenge_title: "Sample Sum",
        target: "linux-arm64-cpu",
        agent_id: "agent-1",
        agent_name: "agent",
        status: "failed",
        explanation: "",
        credit_text: "",
        visible_after_eval: false,
        evaluation: {
          id: "eval-1",
          target: "linux-arm64-cpu",
          status: "failed",
          eval_type: "validation",
          aggregate_metrics: [],
          run_metrics: [],
          public_results: [],
        },
        validation_evaluation: {
          id: "eval-1",
          target: "linux-arm64-cpu",
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
        id: "11111111-1111-4111-8111-111111111111",
        challenge_name: "sample-sum",
        challenge_title: "Sample Sum",
        target: "linux-arm64-cpu",
        agent_id: "agent-1",
        agent_name: "agent",
        status: "completed",
        explanation: "",
        credit_text: "",
        visible_after_eval: true,
        evaluation: {
          id: "eval-1",
          target: "linux-arm64-cpu",
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

  it("rejects explicit nulls for omitted optional response fields", () => {
    expect(() =>
      solutionSubmissionResponseSchema.parse({
        id: "11111111-1111-4111-8111-111111111111",
        challenge_name: "sample-sum",
        target: "linux-arm64-cpu",
        agent_id: "agent-1",
        status: "completed",
        explanation: "",
        parent_solution_submission_id: null,
        credit_text: "",
        visible_after_eval: true,
        official_evaluation: null,
        created_at: "2026-04-28T00:00:00Z",
        updated_at: "2026-04-28T00:00:00Z",
      }),
    ).toThrow();
  });

  it("accepts structured leaderboard metrics", () => {
    expect(() =>
      leaderboardResponseSchema.parse({
        challenge_name: "sample-sum",
        target: "linux-arm64-cpu",
        items: [
          {
            target: "linux-arm64-cpu",
            agent_id: "agent-1",
            agent_name: "solver",
            best_solution_submission_id: "11111111-1111-4111-8111-111111111111",
            best_rank_score: -42,
            rank_score: -42,
            aggregate_metrics: [
              { metric_name: "latency_ms", value: 42 },
              { metric_name: "score", value: 0.9 },
            ],
            official_metrics: [{ metric_name: "latency_ms", value: 42 }],
            official_score: -42,
            updated_at: "2026-04-28T00:00:00Z",
          },
        ],
      }),
    ).not.toThrow();
  });

  it("accepts admin resource profile and capacity responses", () => {
    expect(() =>
      adminChallengeListResponseSchema.parse({
        items: [
          {
            name: "sample-sum",
            title: "Sample Sum",
            summary: "Add numbers",
            status: "active",
            ...challengePolicy,
            targets: [
              {
                ...targetFixture(true),
                resource_profile: {
                  ...targetFixture(true).resource_profile,
                  hardware: { kind: "cpu" },
                },
              },
            ],
            private_benchmark_enabled: true,
            created_at: "2026-04-28T00:00:00Z",
            updated_at: "2026-04-28T00:00:00Z",
          },
        ],
      }),
    ).not.toThrow();

    expect(() =>
      adminCapacityResponseSchema.parse({
        quota_window_seconds: 86400,
        quotas: {
          validation_runs_per_agent_challenge_day: 20,
          official_runs_per_agent_challenge_day: 5,
          max_active_official_jobs: 20,
          max_active_agents: 1000,
        },
        usage: {
          active_agents: 2,
          active_validation_jobs: 1,
          active_official_jobs: 0,
        },
      }),
    ).not.toThrow();
  });
});
