import type { ZodType } from "zod";
import { ApiClientError, browserApiBaseUrl, fetchJson } from "@/lib/http";
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
  ApiClientError as CreatorApiError,
  createChallengeDraftRequestSchema,
  createChallengeShortlistRevisionRequestSchema,
  uploadChallengePrivateAssetRequestSchema,
};

/** Fetches creator me for the requested UI scope. */
export async function getCreatorMe(): Promise<CreatorMeResponse> {
  return creatorFetchJson("/api/creator/me", creatorMeResponseSchema);
}

/** Fetches creator session bootstrap data including the current csrf token. */
export async function getCreatorSession(): Promise<CreatorSessionResponse> {
  return creatorFetchJson("/api/creator/session", creatorSessionResponseSchema);
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
  return fetchJson(path, schema, {
    init,
    csrfToken,
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
  });
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
