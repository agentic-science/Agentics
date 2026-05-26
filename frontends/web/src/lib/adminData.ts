"use client";

import useSWR, { mutate } from "swr";
import { adminFetchJson, adminSession } from "@/lib/adminApi";
import {
  type AdminCapacityResponse,
  type AdminChallengeListResponse,
  type AdminServiceHeartbeatListResponse,
  type AdminSessionResponse,
  type AdminSolutionSubmissionListResponse,
  adminCapacityResponseSchema,
  adminChallengeListResponseSchema,
  adminServiceHeartbeatListResponseSchema,
  adminSolutionSubmissionListResponseSchema,
  type ChallengeDraftListResponse,
  challengeDraftListResponseSchema,
  type PioneerCodeListResponse,
  pioneerCodeListResponseSchema,
} from "@/lib/schemas";

/** Admin dashboard data fetched as one cacheable bundle. */
export interface AdminData {
  challenges: AdminChallengeListResponse;
  drafts: ChallengeDraftListResponse;
  submissions: AdminSolutionSubmissionListResponse;
  heartbeats: AdminServiceHeartbeatListResponse;
  pioneerCodes: PioneerCodeListResponse;
  capacity: AdminCapacityResponse | null;
}

export const emptyAdminData: AdminData = {
  challenges: { items: [] },
  drafts: { items: [] },
  submissions: { items: [] },
  heartbeats: { items: [] },
  pioneerCodes: { items: [] },
  capacity: null,
};

/** Restores the cookie-backed admin browser session through SWR. */
export function useAdminSession() {
  const swr = useSWR<AdminSessionResponse>("admin-session", adminSession, {
    shouldRetryOnError: false,
  });
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
  const [challenges, drafts, submissions, heartbeats, pioneerCodes, capacity] =
    await Promise.all([
      adminFetchJson(
        "/admin/challenges",
        adminChallengeListResponseSchema,
        csrfToken,
      ),
      adminFetchJson(
        "/admin/challenge-drafts",
        challengeDraftListResponseSchema,
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
      adminFetchJson("/admin/capacity", adminCapacityResponseSchema, csrfToken),
    ]);

  return {
    challenges,
    drafts,
    submissions,
    heartbeats,
    pioneerCodes,
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
