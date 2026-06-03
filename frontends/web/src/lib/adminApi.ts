import type { ZodType } from "zod";
import {
  ApiClientError,
  browserApiBaseUrl,
  fetchJson,
  rewriteAdminEndpoint,
} from "@/lib/http";
import {
  type AdminChallengePrivateAssetListResponse,
  type AdminHumanListResponse,
  type AdminHumanRoleResponse,
  type AdminServiceTokenCreatedResponse,
  type AdminServiceTokenListResponse,
  adminChallengePrivateAssetListResponseSchema,
  adminHumanListResponseSchema,
  adminHumanRoleResponseSchema,
  adminServiceTokenCreatedResponseSchema,
  adminServiceTokenListResponseSchema,
  type CreateAdminServiceTokenRequest,
  createAdminServiceTokenRequestSchema,
  type RevokeAdminServiceTokenResponse,
  revokeAdminServiceTokenResponseSchema,
} from "@/lib/schemas";

export { ApiClientError as AdminApiError };

/** Handles admin fetch json behavior for this module. */
export async function adminFetchJson<T>(
  path: string,
  schema: ZodType<T>,
  csrfToken: string,
  init: RequestInit = {},
): Promise<T> {
  return fetchJson(path, schema, {
    init,
    csrfToken,
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
    rewriteEndpoint: rewriteAdminEndpoint,
  });
}

/** Lists every private asset lifecycle record for a challenge review record. */
export async function listAdminChallengeReviewRecordPrivateAssets(
  reviewRecordId: string,
  csrfToken: string,
): Promise<AdminChallengePrivateAssetListResponse> {
  return adminFetchJson(
    `/admin/challenge-review-records/${reviewRecordId}/private-assets`,
    adminChallengePrivateAssetListResponseSchema,
    csrfToken,
  );
}

/** Lists humans and roles visible to admins. */
export async function listAdminHumans(
  csrfToken: string,
): Promise<AdminHumanListResponse> {
  return adminFetchJson(
    "/admin/humans",
    adminHumanListResponseSchema,
    csrfToken,
  );
}

/** Grants the admin role to a human. */
export async function grantHumanAdminRole(
  humanId: string,
  csrfToken: string,
): Promise<AdminHumanRoleResponse> {
  return adminFetchJson(
    `/admin/humans/${encodeURIComponent(humanId)}/roles/admin/grant`,
    adminHumanRoleResponseSchema,
    csrfToken,
    { method: "POST" },
  );
}

/** Revokes the admin role from a human. */
export async function revokeHumanAdminRole(
  humanId: string,
  csrfToken: string,
): Promise<AdminHumanRoleResponse> {
  return adminFetchJson(
    `/admin/humans/${encodeURIComponent(humanId)}/roles/admin/revoke`,
    adminHumanRoleResponseSchema,
    csrfToken,
    { method: "POST" },
  );
}

/** Lists admin service tokens. */
export async function listAdminServiceTokens(
  csrfToken: string,
): Promise<AdminServiceTokenListResponse> {
  return adminFetchJson(
    "/admin/admin-service-tokens",
    adminServiceTokenListResponseSchema,
    csrfToken,
  );
}

/** Creates an admin service token and returns the one-time raw token. */
export async function createAdminServiceToken(
  request: CreateAdminServiceTokenRequest,
  csrfToken: string,
): Promise<AdminServiceTokenCreatedResponse> {
  const body = createAdminServiceTokenRequestSchema.parse(request);
  return adminFetchJson(
    "/admin/admin-service-tokens",
    adminServiceTokenCreatedResponseSchema,
    csrfToken,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
}

/** Revokes an admin service token. */
export async function revokeAdminServiceToken(
  tokenId: string,
  csrfToken: string,
): Promise<RevokeAdminServiceTokenResponse> {
  return adminFetchJson(
    `/admin/admin-service-tokens/${encodeURIComponent(tokenId)}/revoke`,
    revokeAdminServiceTokenResponseSchema,
    csrfToken,
    { method: "POST" },
  );
}
