"use client";

import useSWR, { mutate } from "swr";
import { adminFetchJson } from "@/lib/adminApi";
import { getHumanSession, HUMAN_SESSION_CACHE_KEY } from "@/lib/authApi";
import {
  type AdminCapacityResponse,
  type AdminChallengeListResponse,
  type AdminHumanListResponse,
  type AdminServiceHeartbeatListResponse,
  type AdminServiceTokenListResponse,
  type AdminSolutionSubmissionListResponse,
  adminCapacityResponseSchema,
  adminChallengeListResponseSchema,
  adminHumanListResponseSchema,
  adminServiceHeartbeatListResponseSchema,
  adminServiceTokenListResponseSchema,
  adminSolutionSubmissionListResponseSchema,
  type ChallengeReviewRecordListResponse,
  challengeReviewRecordListResponseSchema,
  type HumanSessionResponse,
  type PioneerCodeListResponse,
  pioneerCodeListResponseSchema,
} from "@/lib/schemas";

/** Admin dashboard data fetched as one cacheable bundle. */
export interface AdminData {
  challenges: AdminChallengeListResponse;
  reviewRecords: ChallengeReviewRecordListResponse;
  submissions: AdminSolutionSubmissionListResponse;
  heartbeats: AdminServiceHeartbeatListResponse;
  pioneerCodes: PioneerCodeListResponse;
  humans: AdminHumanListResponse;
  adminServiceTokens: AdminServiceTokenListResponse;
  capacity: AdminCapacityResponse | null;
}

export const emptyAdminData: AdminData = {
  challenges: { items: [] },
  reviewRecords: { items: [] },
  submissions: { items: [] },
  heartbeats: { items: [] },
  pioneerCodes: { items: [] },
  humans: { items: [] },
  adminServiceTokens: { items: [] },
  capacity: null,
};

/** Restores the cookie-backed admin browser session through SWR. */
export function useAdminSession() {
  const swr = useSWR<HumanSessionResponse>(
    HUMAN_SESSION_CACHE_KEY,
    getHumanSession,
    {
      shouldRetryOnError: false,
    },
  );
  return {
    session: swr.data,
    error: swr.error,
    isLoading: swr.isLoading,
    mutate: swr.mutate,
  };
}

/** Fetches all admin dashboard data with one SWR key. */
export function useAdminDashboard(csrfToken: string) {
  const swr = useSWR<AdminData>(
    csrfToken ? adminDashboardKey(csrfToken) : null,
    fetchAdminDashboardDataByKey,
    {
      keepPreviousData: true,
      shouldRetryOnError: false,
    },
  );
  return {
    data: swr.data ?? emptyAdminData,
    error: swr.error,
    isLoading: swr.isLoading,
    mutate: swr.mutate,
  };
}

/** Refreshes the admin dashboard cache after a mutation. */
export function mutateAdminDashboard(csrfToken: string) {
  return mutate(
    adminDashboardKey(csrfToken),
    fetchAdminDashboardData(csrfToken),
    {
      revalidate: false,
    },
  );
}

/** Clears the admin dashboard cache after logout. */
export function clearAdminDashboard(csrfToken: string) {
  return mutate(adminDashboardKey(csrfToken), emptyAdminData, {
    revalidate: false,
  });
}

/** Fetches the full admin dashboard bundle. */
export async function fetchAdminDashboardData(
  csrfToken: string,
): Promise<AdminData> {
  const [
    challenges,
    reviewRecords,
    submissions,
    heartbeats,
    pioneerCodes,
    humans,
    adminServiceTokens,
    capacity,
  ] = await Promise.all([
    adminFetchJson(
      "/admin/challenges",
      adminChallengeListResponseSchema,
      csrfToken,
    ),
    adminFetchJson(
      "/admin/challenge-review-records",
      challengeReviewRecordListResponseSchema,
      csrfToken,
    ),
    adminFetchJson(
      "/admin/solution-submissions",
      adminSolutionSubmissionListResponseSchema,
      csrfToken,
    ),
    adminFetchJson(
      "/admin/service-heartbeats",
      adminServiceHeartbeatListResponseSchema,
      csrfToken,
    ),
    adminFetchJson(
      "/admin/pioneer-codes",
      pioneerCodeListResponseSchema,
      csrfToken,
    ),
    adminFetchJson("/admin/humans", adminHumanListResponseSchema, csrfToken),
    adminFetchJson(
      "/admin/admin-service-tokens",
      adminServiceTokenListResponseSchema,
      csrfToken,
    ),
    adminFetchJson("/admin/capacity", adminCapacityResponseSchema, csrfToken),
  ]);

  return {
    challenges,
    reviewRecords,
    submissions,
    heartbeats,
    pioneerCodes,
    humans,
    adminServiceTokens,
    capacity,
  };
}

function adminDashboardKey(
  csrfToken: string,
): readonly ["admin-dashboard", string] {
  return ["admin-dashboard", csrfToken] as const;
}

function fetchAdminDashboardDataByKey(
  key: readonly ["admin-dashboard", string],
) {
  return fetchAdminDashboardData(key[1]);
}
