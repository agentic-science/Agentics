"use client";

import useSWR, { mutate } from "swr";
import { getHumanSession, HUMAN_SESSION_CACHE_KEY } from "@/lib/authApi";
import { ApiClientError } from "@/lib/http";
import type { HumanSessionResponse } from "@/lib/schemas";

/** Restores the cookie-backed human session and clears stale auth failures. */
export function useHumanSession() {
  const swr = useSWR<HumanSessionResponse>(
    HUMAN_SESSION_CACHE_KEY,
    getHumanSession,
    {
      onError(error) {
        if (isSessionAuthError(error)) {
          void clearHumanSessionCache();
        }
      },
      shouldRetryOnError: false,
    },
  );
  const signedOut = isSessionAuthError(swr.error);
  return {
    ...swr,
    data: signedOut ? undefined : swr.data,
    session: signedOut ? undefined : swr.data,
  };
}

/** Clears cached human session state without starting a revalidation. */
export function clearHumanSessionCache() {
  return mutate(HUMAN_SESSION_CACHE_KEY, undefined, { revalidate: false });
}

function isSessionAuthError(error: unknown): boolean {
  const status =
    error instanceof ApiClientError
      ? error.status
      : typeof error === "object" &&
          error !== null &&
          "status" in error &&
          typeof error.status === "number"
        ? error.status
        : null;
  return status === 401 || status === 403;
}
