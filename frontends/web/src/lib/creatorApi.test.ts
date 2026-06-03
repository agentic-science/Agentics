import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import {
  completeGithubLogin,
  createChallengeReviewRecord,
  createChallengeShortlistRevision,
  startGithubLogin,
  uploadPrivateAsset,
} from "./creatorApi";

const originalFetch = globalThis.fetch;

ensureDomEnvironment();

describe("creatorApi", () => {
  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("starts GitHub sign-in with POST body instead of URL query secrets", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            authorization_url:
              "https://github.com/login/oauth/authorize?client_id=test&state=github-sign-in-state",
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const response = await startGithubLogin("jack-deadbeef", "/creator");

    expect(response.authorization_url).toBe(
      "https://github.com/login/oauth/authorize?client_id=test&state=github-sign-in-state",
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/auth/github/login",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          pioneer_code: "jack-deadbeef",
          return_to: "/creator",
        }),
      }),
    );
    const requestedPath = fetchMock.mock.calls[0]?.[0];
    expect(requestedPath?.toString()).not.toContain("pioneer_code");
  });

  it("completes GitHub sign-in with POST body instead of query forwarding", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            session: {
              human_id: "11111111-1111-4111-8111-111111111111",
              github_user_id: 123,
              github_login: "octocat",
              roles: ["creator"],
              csrf_token: "csrf-token",
              expires_at: "2026-05-15T00:00:00Z",
            },
            return_to: "/creator",
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await completeGithubLogin("github-sign-in-code", "github-sign-in-state");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/auth/github/callback",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          code: "github-sign-in-code",
          state: "github-sign-in-state",
        }),
      }),
    );
    const requestedPath = fetchMock.mock.calls[0]?.[0];
    expect(requestedPath?.toString()).not.toContain("github-sign-in-code");
    expect(requestedPath?.toString()).not.toContain("github-sign-in-state");
  });

  it("validates creator mutation request bodies before fetch", async () => {
    const fetchMock = vi.fn();
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await expect(
      createChallengeReviewRecord({} as never, "csrf"),
    ).rejects.toThrow();
    await expect(
      uploadPrivateAsset("reviewRecord-id", {} as never, "csrf"),
    ).rejects.toThrow();
    await expect(
      createChallengeShortlistRevision("sample-sum", {} as never, "csrf"),
    ).rejects.toThrow();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
