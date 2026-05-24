import { afterEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { ensureDomEnvironment } from "../test/dom";
import { type ApiClientError, fetchJson } from "./http";

const originalFetch = globalThis.fetch;

ensureDomEnvironment();

describe("http", () => {
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("parses backend error envelopes into typed API errors", async () => {
    globalThis.fetch = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            error: {
              code: "bad_request",
              message: "target is required",
            },
          }),
          {
            status: 400,
            headers: { "content-type": "application/json" },
          },
        );
      },
    ) as unknown as typeof fetch;

    await expect(
      fetchJson("/api/example", z.object({ ok: z.boolean() })),
    ).rejects.toMatchObject({
      status: 400,
      message: "target is required",
    } satisfies Partial<ApiClientError>);
  });

  it("adds csrf and credentials without exposing values in the URL", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(JSON.stringify({ ok: true }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await fetchJson("/admin/example", z.object({ ok: z.boolean() }), {
      csrfToken: "csrf-secret",
      credentials: "include",
      init: { method: "POST", body: JSON.stringify({ value: 1 }) },
      rewriteEndpoint: (path) => path.replace("/admin", "/admin-api"),
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/admin-api/example",
      expect.objectContaining({
        credentials: "include",
        headers: expect.objectContaining({
          "x-agentics-csrf-token": "csrf-secret",
        }),
      }),
    );
    const firstCall = fetchMock.mock.calls[0] as
      | [RequestInfo | URL, RequestInit | undefined]
      | undefined;
    expect(firstCall?.[0].toString()).not.toContain("csrf-secret");
  });
});
