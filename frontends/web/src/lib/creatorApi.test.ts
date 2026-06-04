import { afterEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import {
  completeGithubLogin,
  completeHumanSetup,
  createChallengeReviewRecord,
  createChallengeShortlistRevision,
  createCreatorApiToken,
  revokeCreatorApiToken,
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

    const response = await startGithubLogin("/creator");

    expect(response.authorization_url).toBe(
      "https://github.com/login/oauth/authorize?client_id=test&state=github-sign-in-state",
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/auth/github/login",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          return_to: "/creator",
        }),
      }),
    );
    const requestedPath = fetchMock.mock.calls[0]?.[0];
    expect(requestedPath?.toString()).not.toContain("pioneer_code");
  });

  it("completes human setup with a POST body and CSRF token", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            session: {
              human_id: "11111111-1111-4111-8111-111111111111",
              status: "active",
              github_user_id: 123,
              github_login: "octocat",
              roles: ["creator"],
              csrf_token: "csrf-token",
              expires_at: "2026-05-15T00:00:00Z",
            },
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await completeHumanSetup("jack-deadbeef", "csrf-token");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/auth/setup/pioneer-code",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          pioneer_code: "jack-deadbeef",
        }),
        headers: expect.objectContaining({
          "x-agentics-csrf-token": "csrf-token",
        }),
      }),
    );
    const requestedPath = fetchMock.mock.calls[0]?.[0];
    expect(requestedPath?.toString()).not.toContain("jack-deadbeef");
  });

  it("completes GitHub sign-in with POST body instead of query forwarding", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            session: {
              human_id: "11111111-1111-4111-8111-111111111111",
              status: "active",
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
    await expect(createCreatorApiToken({} as never, "csrf")).rejects.toThrow();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("manages creator API tokens with CSRF-protected browser requests", async () => {
    const fetchMock = vi.fn(
      async (_input: RequestInfo | URL, _init?: RequestInit) => {
        return new Response(
          JSON.stringify({
            token: "agentics_creator_created-secret",
            token_record: {
              id: "22222222-2222-4222-8222-222222222222",
              label: "laptop",
              status: "active",
              created_by_human_id: "11111111-1111-4111-8111-111111111111",
              created_at: "2026-06-05T00:00:00Z",
            },
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    await createCreatorApiToken({ label: "laptop" }, "csrf-token");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/creator/api-tokens",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ label: "laptop" }),
        headers: expect.objectContaining({
          "x-agentics-csrf-token": "csrf-token",
        }),
      }),
    );

    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          token_record: {
            id: "22222222-2222-4222-8222-222222222222",
            label: "laptop",
            status: "revoked",
            created_by_human_id: "11111111-1111-4111-8111-111111111111",
            created_at: "2026-06-05T00:00:00Z",
            revoked_at: "2026-06-05T01:00:00Z",
          },
        }),
        {
          status: 200,
          headers: { "content-type": "application/json" },
        },
      ),
    );

    await revokeCreatorApiToken(
      "22222222-2222-4222-8222-222222222222",
      "csrf-token",
    );

    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/creator/api-tokens/22222222-2222-4222-8222-222222222222/revoke",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({
          "x-agentics-csrf-token": "csrf-token",
        }),
      }),
    );
  });
});
