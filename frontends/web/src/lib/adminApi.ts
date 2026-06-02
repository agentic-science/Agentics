import type { ZodType } from "zod";
import {
  ApiClientError,
  browserApiBaseUrl,
  fetchJson,
  fetchNoContent,
  rewriteAdminEndpoint,
} from "@/lib/http";
import {
  type AdminChallengePrivateAssetListResponse,
  type AdminLoginRequest,
  type AdminSessionResponse,
  adminChallengePrivateAssetListResponseSchema,
  adminLoginRequestSchema,
  adminSessionResponseSchema,
} from "@/lib/schemas";

/** Describes the admin credentials shape used by this module. */
export type AdminCredentials = AdminLoginRequest;

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

/** Handles admin login behavior for this module. */
export async function adminLogin(
  credentials: AdminCredentials,
): Promise<AdminSessionResponse> {
  const request = adminLoginRequestSchema.parse(credentials);
  return fetchJson("/api/auth/admin/login", adminSessionResponseSchema, {
    init: {
      method: "POST",
      body: JSON.stringify(request),
    },
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
  });
}

/** Restores an admin browser session from the existing cookies. */
export async function adminSession(): Promise<AdminSessionResponse> {
  return fetchJson("/api/auth/admin/session", adminSessionResponseSchema, {
    init: { method: "GET" },
    credentials: "include",
    baseUrl: browserApiBaseUrl(),
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

/** Handles admin logout behavior for this module. */
export async function adminLogout(csrfToken: string): Promise<void> {
  return fetchNoContent("/api/auth/admin/logout", {
    init: { method: "POST" },
    credentials: "include",
    csrfToken,
    baseUrl: browserApiBaseUrl(),
  });
}
