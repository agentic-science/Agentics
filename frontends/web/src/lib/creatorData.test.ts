import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import { fetchCreatorOwnerBundle } from "./creatorData";

const originalFetch = globalThis.fetch;

ensureDomEnvironment();

describe("creatorData", () => {
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("fetches owner stats, participants, and shortlist as one bundle", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = input.toString();
      if (url.includes("/stats")) {
        return jsonResponse({
          challenge_id: "11111111-1111-4111-8111-111111111111",
          challenge_name: "sample",
          target: "linux-arm64-cpu",
          agent_count: 1,
          solution_submission_count: 2,
          visible_solution_submission_count: 1,
          completed_solution_submission_count: 1,
          failed_solution_submission_count: 0,
          queued_or_running_solution_submission_count: 1,
          validation_run_count: 1,
          official_run_count: 1,
          best_rank_score_mean: 4.2,
        });
      }
      if (url.includes("/participants")) {
        return jsonResponse({
          challenge_id: "11111111-1111-4111-8111-111111111111",
          challenge_name: "sample",
          target: "linux-arm64-cpu",
          items: [],
        });
      }
      return jsonResponse({
        challenge_id: "11111111-1111-4111-8111-111111111111",
        challenge_name: "sample",
        items: [],
      });
    });
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const bundle = await fetchCreatorOwnerBundle({
      challengeId: "11111111-1111-4111-8111-111111111111",
      target: "linux-arm64-cpu",
    });

    expect(bundle.stats.best_rank_score_mean).toBe(4.2);
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock.mock.calls.map(([input]) => input.toString())).toEqual(
      expect.arrayContaining([
        expect.stringContaining("/stats?target=linux-arm64-cpu"),
        expect.stringContaining("/participants?target=linux-arm64-cpu"),
        expect.stringContaining("/shortlist"),
      ]),
    );
  });
});

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}
