import { browserApiBaseUrl, fetchJson, fetchNoContent } from "@/lib/http";
import {
  type CompleteHumanSetupRequest,
  type CompleteHumanSetupResponse,
  completeHumanSetupRequestSchema,
  completeHumanSetupResponseSchema,
  type GithubSignInCallbackRequest,
  type GithubSignInCallbackResponse,
  type GithubSignInLoginRequest,
  type GithubSignInLoginResponse,
  githubSignInCallbackRequestSchema,
  githubSignInCallbackResponseSchema,
  githubSignInLoginRequestSchema,
  githubSignInLoginResponseSchema,
  type HumanSessionResponse,
  humanSessionResponseSchema,
} from "@/lib/schemas";

export const HUMAN_SESSION_CACHE_KEY = "human-session";

/** Restores the current GitHub-authenticated human session from cookies. */
export async function getHumanSession(): Promise<HumanSessionResponse> {
  return fetchJson("/api/auth/session", humanSessionResponseSchema, {
    init: { method: "GET" },
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
  });
}

/** Starts GitHub sign-in and returns the GitHub authorization URL. */
export async function startGithubLogin(
  returnTo: string,
): Promise<GithubSignInLoginResponse> {
  const request = githubSignInLoginRequestSchema.parse({
    ...(returnTo ? { return_to: returnTo } : {}),
  } satisfies GithubSignInLoginRequest);
  return fetchJson("/api/auth/github/login", githubSignInLoginResponseSchema, {
    init: {
      method: "POST",
      body: JSON.stringify(request),
    },
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
  });
}

/** Completes setup for the current signed-in human using a pioneer code. */
export async function completeHumanSetup(
  pioneerCode: string,
  csrfToken: string,
): Promise<CompleteHumanSetupResponse> {
  const request = completeHumanSetupRequestSchema.parse({
    pioneer_code: pioneerCode,
  } satisfies CompleteHumanSetupRequest);
  return fetchJson(
    "/api/auth/setup/pioneer-code",
    completeHumanSetupResponseSchema,
    {
      init: {
        method: "POST",
        body: JSON.stringify(request),
      },
      credentials: "include",
      csrfToken,
      baseUrl: browserApiBaseUrl(),
    },
  );
}

/** Completes GitHub sign-in and returns the issued human session. */
export async function completeGithubLogin(
  code: string,
  state: string,
): Promise<GithubSignInCallbackResponse> {
  const request = githubSignInCallbackRequestSchema.parse({
    code,
    state,
  } satisfies GithubSignInCallbackRequest);
  return fetchJson(
    "/api/auth/github/callback",
    githubSignInCallbackResponseSchema,
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

/** Deletes the current human account and clears browser auth cookies. */
export async function deleteHumanAccount(csrfToken: string): Promise<void> {
  return fetchNoContent("/api/auth/account/delete", {
    init: { method: "POST" },
    credentials: "include",
    csrfToken,
    baseUrl: browserApiBaseUrl(),
  });
}
