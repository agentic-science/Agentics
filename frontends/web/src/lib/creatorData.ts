"use client";

import useSWR, { mutate } from "swr";
import { getHumanSession, HUMAN_SESSION_CACHE_KEY } from "@/lib/authApi";
import { listCreatorApiTokens } from "@/lib/creatorApi";
import type {
  CreatorApiTokenListResponse,
  HumanSessionResponse,
} from "@/lib/schemas";

/** Restores the cookie-backed creator session through SWR. */
export function useCreatorSession() {
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

const CREATOR_API_TOKENS_CACHE_KEY = "creator-api-tokens";

/** Loads creator API-token metadata for the signed-in creator. */
export function useCreatorApiTokens(enabled: boolean, humanId?: string) {
  const swr = useSWR<CreatorApiTokenListResponse>(
    enabled && humanId ? creatorApiTokensKey(humanId) : null,
    fetchCreatorApiTokensByKey,
    { shouldRetryOnError: false },
  );
  return {
    tokens: swr.data,
    error: swr.error,
    isLoading: swr.isLoading,
    mutate: swr.mutate,
  };
}

/** Refreshes cached creator API tokens after create or revoke. */
export function mutateCreatorApiTokens(humanId?: string) {
  if (!humanId) {
    return Promise.resolve(undefined);
  }
  return mutate(creatorApiTokensKey(humanId));
}

/** Clears all cached creator-token metadata, usually after sign-out. */
export function clearCreatorApiTokenCaches() {
  return mutate(
    (key) => Array.isArray(key) && key[0] === CREATOR_API_TOKENS_CACHE_KEY,
    undefined,
    { revalidate: false },
  );
}

function creatorApiTokensKey(
  humanId: string,
): readonly ["creator-api-tokens", string] {
  return [CREATOR_API_TOKENS_CACHE_KEY, humanId] as const;
}

function fetchCreatorApiTokensByKey(
  _key: readonly ["creator-api-tokens", string],
) {
  return listCreatorApiTokens();
}
