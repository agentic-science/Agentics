import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import { fetchAdminDashboardData } from "./adminData";

const originalFetch = globalThis.fetch;

ensureDomEnvironment();

describe("adminData", () => {
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("fetches dashboard resources with one shared csrf token", async () => {
    const responses: Record<string, unknown> = {
      "/admin-api/challenges": { items: [] },
      "/admin-api/challenge-drafts": { items: [] },
      "/admin-api/solution-submissions": { items: [] },
      "/admin-api/service-heartbeats": { items: [] },
      "/admin-api/pioneer-codes": { items: [] },
      "/admin-api/capacity": {
        quota_window_seconds: 86400,
        quotas: {
          validation_runs_per_agent_challenge_day: 10,
          official_runs_per_agent_challenge_day: 3,
          max_active_official_jobs: 2,
          max_active_agents: 50,
        },
        usage: {
          active_agents: 1,
          active_validation_jobs: 0,
          active_official_jobs: 0,
        },
      },
    };
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        _init?: RequestInit,
      ): Promise<Response> => {
        const body = responses[input.toString()];
        if (!body) {
          return new Response("missing", { status: 404 });
        }
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const data = await fetchAdminDashboardData("csrf-token");

    expect(data.capacity?.quotas.max_active_agents).toBe(50);
    expect(fetchMock).toHaveBeenCalledTimes(6);
    const calls = fetchMock.mock.calls as Array<
      [RequestInfo | URL, RequestInit | undefined]
    >;
    for (const [, init] of calls) {
      expect(init).toEqual(
        expect.objectContaining({
          credentials: "include",
          headers: expect.objectContaining({
            "x-agentics-csrf-token": "csrf-token",
          }),
        }),
      );
    }
  });
});
