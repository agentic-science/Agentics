import type { ZodType } from "zod";
import {
  completeGithubLogin,
  completeHumanSetup,
  getHumanSession as getCreatorSession,
  startGithubLogin,
} from "@/lib/authApi";
import { ApiClientError, browserApiBaseUrl, fetchJson } from "@/lib/http";
import {
  type CreateCreatorApiTokenRequest,
  type CreatorApiTokenCreatedResponse,
  type CreatorApiTokenListResponse,
  createCreatorApiTokenRequestSchema,
  creatorApiTokenCreatedResponseSchema,
  creatorApiTokenListResponseSchema,
  type RevokeCreatorApiTokenResponse,
  revokeCreatorApiTokenResponseSchema,
} from "@/lib/schemas";

export type { CreateCreatorApiTokenRequest };
export {
  ApiClientError as CreatorApiError,
  completeGithubLogin,
  completeHumanSetup,
  createCreatorApiTokenRequestSchema,
  getCreatorSession,
  startGithubLogin,
};

/** Lists API tokens owned by the current creator. */
export async function listCreatorApiTokens(): Promise<CreatorApiTokenListResponse> {
  return creatorFetchJson(
    "/api/creator/api-tokens",
    creatorApiTokenListResponseSchema,
  );
}

/** Creates a creator API token and returns the one-time raw token. */
export async function createCreatorApiToken(
  request: CreateCreatorApiTokenRequest,
  csrfToken: string,
): Promise<CreatorApiTokenCreatedResponse> {
  const body = createCreatorApiTokenRequestSchema.parse(request);
  return creatorFetchJson(
    "/api/creator/api-tokens",
    creatorApiTokenCreatedResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

/** Revokes a creator API token owned by the current creator. */
export async function revokeCreatorApiToken(
  tokenId: string,
  csrfToken: string,
): Promise<RevokeCreatorApiTokenResponse> {
  return creatorFetchJson(
    `/api/creator/api-tokens/${encodeURIComponent(tokenId)}/revoke`,
    revokeCreatorApiTokenResponseSchema,
    csrfToken,
    { method: "POST" },
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
