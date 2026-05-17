import { afterEach, describe, expect, it, vi } from "vitest";
import { startGithubLogin } from "./creatorApi";

const originalFetch = globalThis.fetch;

describe("creatorApi", () => {
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("starts GitHub OAuth with POST body instead of URL query secrets", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            authorization_url:
              "https://github.com/login/oauth/authorize?client_id=test&state=oauth-state",
            state: "oauth-state",
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const response = await startGithubLogin("jack-deadbeef");

    expect(response.authorization_url).toBe(
      "https://github.com/login/oauth/authorize?client_id=test&state=oauth-state",
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/auth/github/login",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ pioneer_code: "jack-deadbeef" }),
      }),
    );
    const requestedPath = fetchMock.mock.calls[0]?.[0];
    expect(requestedPath?.toString()).not.toContain("pioneer_code");
  });
});
