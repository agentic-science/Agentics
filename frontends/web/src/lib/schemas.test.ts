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

/** Builds the target fixture test fixture. */
function targetFixture(validationEnabled: boolean) {
  return {
    name: "linux-arm64-cpu",
    docker_platform: "linux/arm64",
    accelerator: null,
    validation_enabled: validationEnabled,
    resource_profile: {
      name: "python-cpu-small",
      solution_image: {
        source: "local",
        reference: "agentics-linux-arm64-cpu:ubuntu26.04-local",
      },
      evaluator_image: {
        source: "local",
        reference: "agentics-linux-arm64-cpu:ubuntu26.04-local",
      },
      timeout_sec: 30,
      memory_limit_mb: 512,
      cpu_limit_millis: 1000,
      disk_limit_mb: 1024,
      setup_network_access: "enabled",
      build_network_access: "disabled",
      run_network_access: "disabled",
      evaluator_network_access: "disabled",
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
        summary: { en: "Add two numbers.", zh: "数字求和。" },
        keywords: ["arithmetic"],
        spec: {
          schema_version: 1,
          challenge_name: "sample-sum",
          challenge_title: "Sample Sum",
          summary: { en: "Add two numbers.", zh: "数字求和。" },
          keywords: ["arithmetic"],
          starts_at: "2026-01-01T00:00:00Z",
          ...challengePolicy,
          solution: {
            protocol: "zip_project",
            manifest_file: "agentics.solution.json",
          },
          targets: [targetFixture(true)],
          execution: {
            mode: "separated_evaluator",
            evaluator: {
              command: ["python", "evaluator/run.py"],
              result_file: "result.json",
            },
            validation_runs: "public/runs.json",
          },
          datasets: {
            public_dir: "public",
            public_policy: "full",
            private_benchmark_policy: "score_only",
            private_benchmark_enabled: true,
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

    expect(() =>
      challengeDetailResponseSchema.parse({
        name: "interactive-sum",
        title: "Interactive Sum",
        summary: { en: "Add numbers interactively.", zh: "交互式数字求和。" },
        keywords: ["interactive"],
        spec: {
          schema_version: 1,
          challenge_name: "interactive-sum",
          challenge_title: "Interactive Sum",
          summary: { en: "Add numbers interactively.", zh: "交互式数字求和。" },
          keywords: ["interactive"],
          starts_at: "2026-01-01T00:00:00Z",
          ...challengePolicy,
          solution: {
            protocol: "zip_project",
            manifest_file: "agentics.solution.json",
          },
          targets: [targetFixture(true)],
          execution: {
            mode: "piped_stdio",
            interactor: {
              command: ["python", "interactor/run.py"],
              result_file: "result.json",
            },
            validation_session: "public/session.json",
          },
          datasets: {
            public_dir: "public",
            public_policy: "full",
            private_benchmark_policy: "score_only",
            private_benchmark_enabled: true,
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
            },
          },
        },
        statement_markdown: "# Interactive Sum",
      }),
    ).not.toThrow();
  });

  it("accepts relaxed omission of nullable evaluation fields", () => {
    expect(() =>
      solutionSubmissionResponseSchema.parse({
        id: "11111111-1111-4111-8111-111111111111",
        challenge_name: "sample-sum",
        challenge_title: "Sample Sum",
        target: "linux-arm64-cpu",
        agent_id: "22222222-2222-4222-8222-222222222222",
        agent_display_name: "agent",
        status: "failed",
        note: "",
        explanation: "",
        credit_text: "",
        visible_after_eval: false,
        evaluation: {
          id: "33333333-3333-4333-8333-333333333333",
          target: "linux-arm64-cpu",
          status: "failed",
          eval_type: "validation",
          aggregate_metrics: [],
          run_metrics: [],
          public_results: [],
        },
        validation_evaluation: {
          id: "33333333-3333-4333-8333-333333333333",
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

  it("accepts empty public result messages emitted by relaxed evaluator JSON", () => {
    expect(() =>
      solutionSubmissionResponseSchema.parse({
        id: "11111111-1111-4111-8111-111111111111",
        challenge_name: "sample-sum",
        challenge_title: "Sample Sum",
        target: "linux-arm64-cpu",
        agent_id: "22222222-2222-4222-8222-222222222222",
        agent_display_name: "agent",
        status: "completed",
        note: "validation note",
        explanation: "",
        credit_text: "",
        visible_after_eval: true,
        evaluation: {
          id: "33333333-3333-4333-8333-333333333333",
          target: "linux-arm64-cpu",
          status: "completed",
          eval_type: "validation",
          aggregate_metrics: [],
          run_metrics: [],
          public_results: [
            {
              case_name: "case-1",
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
        agent_id: "22222222-2222-4222-8222-222222222222",
        status: "completed",
        note: "",
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

  it("keeps raw metric payloads out of public leaderboard parsed values", () => {
    const parsed = leaderboardResponseSchema.parse({
      challenge_name: "sample-sum",
      target: "linux-arm64-cpu",
      items: [
        {
          target: "linux-arm64-cpu",
          agent_id: "22222222-2222-4222-8222-222222222222",
          agent_display_name: "solver",
          best_solution_submission_id: "11111111-1111-4111-8111-111111111111",
          best_rank_score: -42,
          rank_score: -42,
          aggregate_metrics: [],
          official_metrics: [],
          official_primary_metric: {
            metric_name: "score",
            value: -42,
          },
          updated_at: "2026-04-28T00:00:00Z",
        },
      ],
    });

    expect("aggregate_metrics" in parsed.items[0]).toBe(false);
    expect("official_metrics" in parsed.items[0]).toBe(false);
  });

  it("accepts admin resource profile and capacity responses", () => {
    expect(() =>
      adminChallengeListResponseSchema.parse({
        items: [
          {
            name: "sample-sum",
            title: "Sample Sum",
            summary: { en: "Add numbers", zh: "数字求和" },
            status: "active",
            ...challengePolicy,
            targets: [
              {
                ...targetFixture(true),
                resource_profile: {
                  ...targetFixture(true).resource_profile,
                  hardware_metadata: { kind: "cpu" },
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
