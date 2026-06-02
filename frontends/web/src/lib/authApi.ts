import { browserApiBaseUrl, fetchJson, fetchNoContent } from "@/lib/http";
import {
  type GithubOauthCallbackRequest,
  type GithubOauthCallbackResponse,
  type GithubOauthLoginRequest,
  type GithubOauthLoginResponse,
  githubOauthCallbackRequestSchema,
  githubOauthCallbackResponseSchema,
  githubOauthLoginRequestSchema,
  githubOauthLoginResponseSchema,
  type HumanSessionResponse,
  humanSessionResponseSchema,
} from "@/lib/schemas";

/** Restores the current GitHub-authenticated human session from cookies. */
export async function getHumanSession(): Promise<HumanSessionResponse> {
  return fetchJson("/api/auth/session", humanSessionResponseSchema, {
    init: { method: "GET" },
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
  });
}

/** Starts GitHub login and returns the GitHub authorization URL. */
export async function startGithubLogin(
  pioneerCode: string,
  returnTo: string,
): Promise<GithubOauthLoginResponse> {
  const request = githubOauthLoginRequestSchema.parse({
    ...(pioneerCode ? { pioneer_code: pioneerCode } : {}),
    ...(returnTo ? { return_to: returnTo } : {}),
  } satisfies GithubOauthLoginRequest);
  return fetchJson("/api/auth/github/login", githubOauthLoginResponseSchema, {
    init: {
      method: "POST",
      body: JSON.stringify(request),
    },
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
  });
}

/** Completes GitHub login and returns the issued human session. */
export async function completeGithubLogin(
  code: string,
  state: string,
): Promise<GithubOauthCallbackResponse> {
  const request = githubOauthCallbackRequestSchema.parse({
    code,
    state,
  } satisfies GithubOauthCallbackRequest);
  return fetchJson(
    "/api/auth/github/callback",
    githubOauthCallbackResponseSchema,
    {
      init: {
        method: "POST",
        body: JSON.stringify(request),
      },
      credentials: "include",
      baseUrl: browserApiBaseUrl(),
    },
  );
}

/** Ends the current human browser session. */
export async function logoutHuman(csrfToken: string): Promise<void> {
  return fetchNoContent("/api/auth/logout", {
    init: { method: "POST" },
    credentials: "include",
    csrfToken,
    baseUrl: browserApiBaseUrl(),
  });
}
