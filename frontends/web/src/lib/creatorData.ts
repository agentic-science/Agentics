"use client";

import useSWR, { mutate } from "swr";
import {
  getChallengeDraft,
  getChallengeShortlist,
  getCreatorChallengeParticipants,
  getCreatorChallengeStats,
  getCreatorSession,
} from "@/lib/creatorApi";
import type {
  ChallengeShortlistResponse,
  CreatorChallengeDraftResponse,
  CreatorChallengeParticipantsResponse,
  CreatorChallengeStatsResponse,
  CreatorSessionResponse,
} from "@/lib/schemas";

/** Creator owner surfaces fetched together for one challenge and optional target. */
export interface CreatorOwnerBundle {
  stats: CreatorChallengeStatsResponse;
  participants: CreatorChallengeParticipantsResponse;
  shortlist: ChallengeShortlistResponse;
}

/** Published challenge owner scope used by creator data hooks. */
export interface CreatorOwnerScope {
  challengeName: string;
  target?: string;
}

/** Restores the cookie-backed creator session through SWR. */
export function useCreatorSession() {
  const swr = useSWR<CreatorSessionResponse>(
    "creator-session",
    getCreatorSession,
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

/** Loads one creator-owned draft when an id is available. */
export function useCreatorDraft(draftId: string) {
  const normalized = draftId.trim();
  const swr = useSWR<CreatorChallengeDraftResponse>(
    normalized ? creatorDraftKey(normalized) : null,
    fetchCreatorDraftByKey,
    { shouldRetryOnError: false },
  );
  return {
    draft: swr.data,
    error: swr.error,
    isLoading: swr.isLoading,
    mutate: swr.mutate,
  };
}

/** Loads creator-owned stats, participants, and shortlist for one challenge. */
export function useCreatorOwnerBundle(scope: CreatorOwnerScope | null) {
  const swr = useSWR<CreatorOwnerBundle>(
    scope ? creatorOwnerBundleKey(scope) : null,
    fetchCreatorOwnerBundleByKey,
    { shouldRetryOnError: false },
  );
  return {
    bundle: swr.data,
    error: swr.error,
    isLoading: swr.isLoading,
    mutate: swr.mutate,
  };
}

/** Refreshes one cached creator draft after asset upload or draft creation. */
export function mutateCreatorDraft(draftId: string) {
  return mutate(creatorDraftKey(draftId));
}

/** Refreshes owner surfaces after shortlist mutation. */
export function mutateCreatorOwnerBundle(scope: CreatorOwnerScope) {
  return mutate(creatorOwnerBundleKey(scope));
}

/** Fetches owner surfaces as one bundle. */
export async function fetchCreatorOwnerBundle(
  scope: CreatorOwnerScope,
): Promise<CreatorOwnerBundle> {
  const [stats, participants, shortlist] = await Promise.all([
    getCreatorChallengeStats(scope.challengeName, scope.target),
    getCreatorChallengeParticipants(scope.challengeName, scope.target),
    getChallengeShortlist(scope.challengeName),
  ]);
  return { stats, participants, shortlist };
}

function creatorDraftKey(draftId: string): readonly ["creator-draft", string] {
  return ["creator-draft", draftId] as const;
}

function creatorOwnerBundleKey(
  scope: CreatorOwnerScope,
): readonly ["creator-owner-bundle", string, string] {
  return [
    "creator-owner-bundle",
    scope.challengeName,
    scope.target ?? "",
  ] as const;
}

function fetchCreatorDraftByKey(key: readonly ["creator-draft", string]) {
  return getChallengeDraft(key[1]);
}

function fetchCreatorOwnerBundleByKey(
  key: readonly ["creator-owner-bundle", string, string],
) {
  return fetchCreatorOwnerBundle({
    challengeName: key[1],
    target: key[2] || undefined,
  });
}
