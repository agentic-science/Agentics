import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import { fetchAdminDashboardData, useAdminDashboard } from "./adminData";

const originalFetch = globalThis.fetch;

ensureDomEnvironment();
const { cleanup, render, waitFor } = await import("@testing-library/react");
const { createElement } = await import("react");
const { SWRConfig } = await import("swr");

describe("adminData", () => {
  afterEach(() => {
    cleanup();
    globalThis.fetch = originalFetch;
    vi.clearAllMocks();
  });

  it("fetches dashboard resources with one shared csrf token", async () => {
    globalThis.fetch = adminDashboardFetchMock();

    const data = await fetchAdminDashboardData("csrf-token");

    expect(data.capacity?.quotas.max_active_agents).toBe(50);
    expect(globalThis.fetch).toHaveBeenCalledTimes(8);
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock
      .calls as Array<[RequestInfo | URL, RequestInit | undefined]>;
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

  it("keeps csrf tokens out of SWR dashboard cache keys", async () => {
    globalThis.fetch = adminDashboardFetchMock();
    const cache = new Map();

    render(
      createElement(
        SWRConfig,
        { value: { provider: () => cache, dedupingInterval: 0 } },
        createElement(AdminDashboardProbe, { csrfToken: "csrf-secret" }),
      ),
    );

    await waitFor(() => expect(globalThis.fetch).toHaveBeenCalledTimes(8));
    expect([...cache.keys()].join("\n")).not.toContain("csrf-secret");
  });
});

function AdminDashboardProbe({ csrfToken }: { csrfToken: string }) {
  useAdminDashboard(csrfToken);
  return null;
}

function adminDashboardFetchMock() {
  const responses: Record<string, unknown> = {
    "/admin-api/challenges": { items: [] },
    "/admin-api/challenge-review-records": { items: [] },
    "/admin-api/solution-submissions": { items: [] },
    "/admin-api/service-heartbeats": { items: [] },
    "/admin-api/pioneer-codes": { items: [] },
    "/admin-api/humans": { items: [] },
    "/admin-api/admin-service-tokens": { items: [] },
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
  return vi.fn(
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
  ) as unknown as typeof fetch;
}
