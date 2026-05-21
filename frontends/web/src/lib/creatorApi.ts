import type { ZodType } from "zod";
import {
  type ChallengePrivateAssetResponse,
  type ChallengeShortlistResponse,
  type ChallengeShortlistRevisionResponse,
  type CreateChallengeDraftRequest,
  type CreateChallengeShortlistRevisionRequest,
  type CreatorChallengeDraftResponse,
  type CreatorChallengeParticipantsResponse,
  type CreatorChallengeStatsResponse,
  type CreatorMeResponse,
  type CreatorSessionResponse,
  challengePrivateAssetResponseSchema,
  challengeShortlistResponseSchema,
  challengeShortlistRevisionResponseSchema,
  createChallengeDraftRequestSchema,
  createChallengeShortlistRevisionRequestSchema,
  creatorChallengeDraftResponseSchema,
  creatorChallengeParticipantsResponseSchema,
  creatorChallengeStatsResponseSchema,
  creatorMeResponseSchema,
  creatorSessionResponseSchema,
  type GithubOauthCallbackRequest,
  type GithubOauthLoginRequest,
  type GithubOauthLoginResponse,
  githubOauthCallbackRequestSchema,
  githubOauthLoginRequestSchema,
  githubOauthLoginResponseSchema,
  type UploadChallengePrivateAssetRequest,
  uploadChallengePrivateAssetRequestSchema,
} from "@/lib/schemas";

const CREATOR_CSRF_STORAGE_KEY = "agentics.creator.csrf_token";
const CREATOR_OAUTH_STATE_STORAGE_KEY = "agentics.creator.oauth_state";

/** Describes the challenge creation manifest shape used by this module. */
export type ChallengeCreationManifest =
  CreatorChallengeDraftResponse["manifest"];
/** Describes the challenge private asset kind shape used by this module. */
export type ChallengePrivateAssetKind =
  UploadChallengePrivateAssetRequest["kind"];

export type {
  CreateChallengeDraftRequest,
  CreateChallengeShortlistRevisionRequest,
  UploadChallengePrivateAssetRequest,
};

export {
  createChallengeDraftRequestSchema,
  createChallengeShortlistRevisionRequestSchema,
  uploadChallengePrivateAssetRequestSchema,
};

/** Error thrown when a creator-session API request fails. */
export class CreatorApiError extends Error {
  readonly status: number;

  /** Stores the HTTP status alongside the backend error message. */
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** Reads creator csrf token from browser session storage. */
export function readCreatorCsrfToken(): string {
  if (typeof window === "undefined") {
    return "";
  }

  return window.sessionStorage.getItem(CREATOR_CSRF_STORAGE_KEY) ?? "";
}

/** Stores creator csrf token in browser state. */
export function storeCreatorCsrfToken(csrfToken: string): void {
  if (typeof window !== "undefined") {
    window.sessionStorage.setItem(CREATOR_CSRF_STORAGE_KEY, csrfToken);
  }
}

/** Stores the expected OAuth state before navigating to GitHub. */
export function storeExpectedGithubOauthState(state: string): void {
  if (typeof window !== "undefined") {
    window.sessionStorage.setItem(CREATOR_OAUTH_STATE_STORAGE_KEY, state);
  }
}

/** Consumes the expected OAuth state and returns whether it matches. */
export function consumeExpectedGithubOauthState(
  returnedState: string,
): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  const expected = window.sessionStorage.getItem(
    CREATOR_OAUTH_STATE_STORAGE_KEY,
  );
  window.sessionStorage.removeItem(CREATOR_OAUTH_STATE_STORAGE_KEY);
  return Boolean(expected) && expected === returnedState;
}

/** Fetches creator me for the requested UI scope. */
export async function getCreatorMe(): Promise<CreatorMeResponse> {
  return creatorFetchJson("/api/creator/me", creatorMeResponseSchema);
}

/** Fetches creator session bootstrap data including the current csrf token. */
export async function getCreatorSession(): Promise<CreatorSessionResponse> {
  const session = await creatorFetchJson(
    "/api/creator/session",
    creatorSessionResponseSchema,
  );
  storeCreatorCsrfToken(session.csrf_token);
  return session;
}

/** Starts github login and returns the next navigation target. */
export async function startGithubLogin(
  pioneerCode: string,
): Promise<GithubOauthLoginResponse> {
  const request = githubOauthLoginRequestSchema.parse({
    ...(pioneerCode ? { pioneer_code: pioneerCode } : {}),
  } satisfies GithubOauthLoginRequest);
  return creatorFetchJson(
    "/api/auth/github/login",
    githubOauthLoginResponseSchema,
    undefined,
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

/** Completes github login using the returned state. */
export async function completeGithubLogin(
  code: string,
  state: string,
): Promise<CreatorSessionResponse> {
  const request = githubOauthCallbackRequestSchema.parse({
    code,
    state,
  } satisfies GithubOauthCallbackRequest);
  return creatorFetchJson(
    "/api/auth/github/callback",
    creatorSessionResponseSchema,
    undefined,
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

/** Creates challenge draft through the API. */
export async function createChallengeDraft(
  request: CreateChallengeDraftRequest,
  csrfToken: string,
): Promise<CreatorChallengeDraftResponse> {
  const body = createChallengeDraftRequestSchema.parse(request);
  return creatorFetchJson(
    "/api/creator/challenge-drafts",
    creatorChallengeDraftResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

/** Fetches challenge draft for the requested UI scope. */
export async function getChallengeDraft(
  id: string,
): Promise<CreatorChallengeDraftResponse> {
  return creatorFetchJson(
    `/api/creator/challenge-drafts/${encodeURIComponent(id)}`,
    creatorChallengeDraftResponseSchema,
  );
}

/** Uploads private asset through the API. */
export async function uploadPrivateAsset(
  draftId: string,
  request: UploadChallengePrivateAssetRequest,
  csrfToken: string,
): Promise<ChallengePrivateAssetResponse> {
  const body = uploadChallengePrivateAssetRequestSchema.parse(request);
  return creatorFetchJson(
    `/api/creator/challenge-drafts/${encodeURIComponent(draftId)}/private-assets`,
    challengePrivateAssetResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

/** Fetches creator challenge stats for the requested UI scope. */
export async function getCreatorChallengeStats(
  challengeName: string,
  target?: string,
): Promise<CreatorChallengeStatsResponse> {
  return creatorFetchJson(
    creatorChallengePath(challengeName, "stats", target),
    creatorChallengeStatsResponseSchema,
  );
}

/** Fetches creator challenge participants for the requested UI scope. */
export async function getCreatorChallengeParticipants(
  challengeName: string,
  target?: string,
): Promise<CreatorChallengeParticipantsResponse> {
  return creatorFetchJson(
    creatorChallengePath(challengeName, "participants", target),
    creatorChallengeParticipantsResponseSchema,
  );
}

/** Fetches challenge shortlist for the requested UI scope. */
export async function getChallengeShortlist(
  challengeName: string,
): Promise<ChallengeShortlistResponse> {
  return creatorFetchJson(
    `/api/creator/challenges/${encodeURIComponent(challengeName)}/shortlist`,
    challengeShortlistResponseSchema,
  );
}

/** Creates challenge shortlist revision through the API. */
export async function createChallengeShortlistRevision(
  challengeName: string,
  request: CreateChallengeShortlistRevisionRequest,
  csrfToken: string,
): Promise<ChallengeShortlistRevisionResponse> {
  const body = createChallengeShortlistRevisionRequestSchema.parse(request);
  return creatorFetchJson(
    `/api/creator/challenges/${encodeURIComponent(challengeName)}/shortlist-revisions`,
    challengeShortlistRevisionResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

/** Handles creator fetch json behavior for this module. */
async function creatorFetchJson<T>(
  path: string,
  schema: ZodType<T>,
  csrfToken?: string,
  init: RequestInit = {},
): Promise<T> {
  const headers: Record<string, string> = {
    "content-type": "application/json",
  };
  if (csrfToken) {
    headers["x-agentics-csrf-token"] = csrfToken;
  }

  const response = await fetch(path, {
    ...init,
    credentials: "include",
    headers: {
      ...headers,
      ...init.headers,
    },
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      /** Handles body behavior for this component. */
      const body = (await response.json()) as { message?: string };
      message = body.message ?? message;
    } catch {
      // Non-JSON error responses still surface the status text.
    }
    throw new CreatorApiError(response.status, message);
  }

  return schema.parse(await response.json());
}

/** Handles creator challenge path behavior for this module. */
function creatorChallengePath(
  challengeName: string,
  surface: "stats" | "participants",
  target?: string,
): string {
  const params = new URLSearchParams();
  if (target?.trim()) {
    params.set("target", target.trim());
  }
  const query = params.toString();
  return `/api/creator/challenges/${encodeURIComponent(challengeName)}/${surface}${query ? `?${query}` : ""}`;
}
