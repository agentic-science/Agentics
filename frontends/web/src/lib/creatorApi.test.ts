import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import {
  completeGithubLogin,
  consumeExpectedGithubOauthState,
  createChallengeDraft,
  createChallengeShortlistRevision,
  startGithubLogin,
  storeExpectedGithubOauthState,
  uploadPrivateAsset,
} from "./creatorApi";

const originalFetch = globalThis.fetch;

ensureDomEnvironment();

describe("creatorApi", () => {
  afterEach(() => {
    globalThis.fetch = originalFetch;
    window.sessionStorage.clear();
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

  it("completes GitHub OAuth with POST body instead of query forwarding", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            agent_id: "11111111-1111-4111-8111-111111111111",
            github_user_id: 123,
            github_login: "octocat",
            csrf_token: "csrf-token",
            expires_at: "2026-05-15T00:00:00Z",
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await completeGithubLogin("oauth-code", "oauth-state");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/auth/github/callback",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ code: "oauth-code", state: "oauth-state" }),
      }),
    );
    const requestedPath = fetchMock.mock.calls[0]?.[0];
    expect(requestedPath?.toString()).not.toContain("oauth-code");
    expect(requestedPath?.toString()).not.toContain("oauth-state");
  });

  it("stores and consumes expected GitHub OAuth state once", () => {
    storeExpectedGithubOauthState("oauth-state");

    expect(consumeExpectedGithubOauthState("wrong-state")).toBe(false);
    expect(consumeExpectedGithubOauthState("oauth-state")).toBe(false);

    storeExpectedGithubOauthState("oauth-state");
    expect(consumeExpectedGithubOauthState("oauth-state")).toBe(true);
    expect(consumeExpectedGithubOauthState("oauth-state")).toBe(false);
  });

  it("validates creator mutation request bodies before fetch", async () => {
    const fetchMock = vi.fn();
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await expect(createChallengeDraft({} as never, "csrf")).rejects.toThrow();
    await expect(
      uploadPrivateAsset("draft-id", {} as never, "csrf"),
    ).rejects.toThrow();
    await expect(
      createChallengeShortlistRevision("sample-sum", {} as never, "csrf"),
    ).rejects.toThrow();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
