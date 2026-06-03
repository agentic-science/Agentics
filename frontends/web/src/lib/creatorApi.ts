import type { ZodType } from "zod";
import {
  completeGithubLogin,
  getHumanSession as getCreatorSession,
  startGithubLogin,
} from "@/lib/authApi";
import { ApiClientError, browserApiBaseUrl, fetchJson } from "@/lib/http";
import {
  type ChallengePrivateAssetResponse,
  type ChallengeShortlistResponse,
  type ChallengeShortlistRevisionResponse,
  type CreateChallengeReviewRecordRequest,
  type CreateChallengeShortlistRevisionRequest,
  type CreatorChallengeParticipantsResponse,
  type CreatorChallengeReviewRecordResponse,
  type CreatorChallengeStatsResponse,
  challengePrivateAssetResponseSchema,
  challengeShortlistResponseSchema,
  challengeShortlistRevisionResponseSchema,
  createChallengeReviewRecordRequestSchema,
  createChallengeShortlistRevisionRequestSchema,
  creatorChallengeParticipantsResponseSchema,
  creatorChallengeReviewRecordResponseSchema,
  creatorChallengeStatsResponseSchema,
  type UploadChallengePrivateAssetRequest,
  uploadChallengePrivateAssetRequestSchema,
} from "@/lib/schemas";

/** Describes the challenge creation manifest shape used by this module. */
export type ChallengeCreationManifest =
  CreatorChallengeReviewRecordResponse["manifest"];
/** Describes the challenge private asset kind shape used by this module. */
export type ChallengePrivateAssetKind =
  UploadChallengePrivateAssetRequest["kind"];

export type {
  CreateChallengeReviewRecordRequest,
  CreateChallengeShortlistRevisionRequest,
  UploadChallengePrivateAssetRequest,
};
export {
  ApiClientError as CreatorApiError,
  completeGithubLogin,
  createChallengeReviewRecordRequestSchema,
  createChallengeShortlistRevisionRequestSchema,
  getCreatorSession,
  startGithubLogin,
  uploadChallengePrivateAssetRequestSchema,
};

/** Creates challenge review record through the API. */
export async function createChallengeReviewRecord(
  request: CreateChallengeReviewRecordRequest,
  csrfToken: string,
): Promise<CreatorChallengeReviewRecordResponse> {
  const body = createChallengeReviewRecordRequestSchema.parse(request);
  return creatorFetchJson(
    "/api/creator/challenge-review-records",
    creatorChallengeReviewRecordResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

/** Fetches challenge review record for the requested UI scope. */
export async function getChallengeReviewRecord(
  id: string,
): Promise<CreatorChallengeReviewRecordResponse> {
  return creatorFetchJson(
    `/api/creator/challenge-review-records/${encodeURIComponent(id)}`,
    creatorChallengeReviewRecordResponseSchema,
  );
}

/** Uploads private asset through the API. */
export async function uploadPrivateAsset(
  reviewRecordId: string,
  request: UploadChallengePrivateAssetRequest,
  csrfToken: string,
): Promise<ChallengePrivateAssetResponse> {
  const body = uploadChallengePrivateAssetRequestSchema.parse(request);
  return creatorFetchJson(
    `/api/creator/challenge-review-records/${encodeURIComponent(reviewRecordId)}/private-assets`,
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
