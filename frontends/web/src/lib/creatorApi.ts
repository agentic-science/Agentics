import type { ZodType } from "zod";
import {
  type ChallengeDraftResponse,
  type ChallengePrivateAssetResponse,
  type ChallengeShortlistResponse,
  type ChallengeShortlistRevisionResponse,
  type CreateChallengeDraftRequest,
  type CreateChallengeShortlistRevisionRequest,
  type CreatorChallengeParticipantsResponse,
  type CreatorChallengeStatsResponse,
  type CreatorMeResponse,
  type CreatorSessionResponse,
  challengeDraftResponseSchema,
  challengePrivateAssetResponseSchema,
  challengeShortlistResponseSchema,
  challengeShortlistRevisionResponseSchema,
  createChallengeDraftRequestSchema,
  createChallengeShortlistRevisionRequestSchema,
  creatorChallengeParticipantsResponseSchema,
  creatorChallengeStatsResponseSchema,
  creatorMeResponseSchema,
  creatorSessionResponseSchema,
  type GithubOauthLoginResponse,
  githubOauthLoginResponseSchema,
  type UploadChallengePrivateAssetRequest,
  uploadChallengePrivateAssetRequestSchema,
} from "@/lib/schemas";

const CREATOR_CSRF_STORAGE_KEY = "agentics.creator.csrf_token";
const DEFAULT_CSRF_COOKIE_NAME = "agentics_csrf";

/** Describes the challenge creation manifest shape used by this module. */
export type ChallengeCreationManifest = ChallengeDraftResponse["manifest"];
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

/** Reads creator csrf token from browser state. */
export function readCreatorCsrfToken(): string {
  if (typeof window === "undefined") {
    return "";
  }

  const stored = window.sessionStorage.getItem(CREATOR_CSRF_STORAGE_KEY);
  if (stored) {
    return stored;
  }

  return readCookie(DEFAULT_CSRF_COOKIE_NAME);
}

/** Stores creator csrf token in browser state. */
export function storeCreatorCsrfToken(csrfToken: string): void {
  if (typeof window !== "undefined") {
    window.sessionStorage.setItem(CREATOR_CSRF_STORAGE_KEY, csrfToken);
  }
}

/** Fetches creator me for the requested UI scope. */
export async function getCreatorMe(): Promise<CreatorMeResponse> {
  return creatorFetchJson("/api/creator/me", creatorMeResponseSchema);
}

/** Starts github login and returns the next navigation target. */
export async function startGithubLogin(): Promise<GithubOauthLoginResponse> {
  return creatorFetchJson(
    "/api/auth/github/login",
    githubOauthLoginResponseSchema,
  );
}

/** Completes github login using the returned state. */
export async function completeGithubLogin(
  code: string,
  state: string,
): Promise<CreatorSessionResponse> {
  const params = new URLSearchParams({ code, state });
  return creatorFetchJson(
    `/api/auth/github/callback?${params.toString()}`,
    creatorSessionResponseSchema,
  );
}

/** Creates challenge draft through the API. */
export async function createChallengeDraft(
  request: CreateChallengeDraftRequest,
  csrfToken: string,
): Promise<ChallengeDraftResponse> {
  return creatorFetchJson(
    "/api/creator/challenge-drafts",
    challengeDraftResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(request),
    },
  );
}

/** Fetches challenge draft for the requested UI scope. */
export async function getChallengeDraft(
  id: string,
): Promise<ChallengeDraftResponse> {
  return creatorFetchJson(
    `/api/creator/challenge-drafts/${encodeURIComponent(id)}`,
    challengeDraftResponseSchema,
  );
}

/** Uploads private asset through the API. */
export async function uploadPrivateAsset(
  draftId: string,
  request: UploadChallengePrivateAssetRequest,
  csrfToken: string,
): Promise<ChallengePrivateAssetResponse> {
  return creatorFetchJson(
    `/api/creator/challenge-drafts/${encodeURIComponent(draftId)}/private-assets`,
    challengePrivateAssetResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(request),
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
  return creatorFetchJson(
    `/api/creator/challenges/${encodeURIComponent(challengeName)}/shortlist-revisions`,
    challengeShortlistRevisionResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(request),
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

/** Reads cookie from browser state. */
function readCookie(name: string): string {
  return (
    document.cookie
      .split(";")
      .map((cookie) => cookie.trim())
      .find((cookie) => cookie.startsWith(`${name}=`))
      ?.slice(name.length + 1) ?? ""
  );
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
