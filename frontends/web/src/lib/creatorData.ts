"use client";

import useSWR, { mutate } from "swr";
import { getHumanSession, HUMAN_SESSION_CACHE_KEY } from "@/lib/authApi";
import {
  getChallengeReviewRecord,
  getChallengeShortlist,
  getCreatorChallengeParticipants,
  getCreatorChallengeStats,
} from "@/lib/creatorApi";
import type {
  ChallengeShortlistResponse,
  CreatorChallengeParticipantsResponse,
  CreatorChallengeReviewRecordResponse,
  CreatorChallengeStatsResponse,
  HumanSessionResponse,
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

/** Loads one creator-owned review record when an id is available. */
export function useCreatorReviewRecord(reviewRecordId: string) {
  const normalized = reviewRecordId.trim();
  const swr = useSWR<CreatorChallengeReviewRecordResponse>(
    normalized ? creatorReviewRecordKey(normalized) : null,
    fetchCreatorReviewRecordByKey,
    { shouldRetryOnError: false },
  );
  return {
    reviewRecord: swr.data,
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

/** Refreshes one cached creator review record after asset upload or review record creation. */
export function mutateCreatorReviewRecord(reviewRecordId: string) {
  return mutate(creatorReviewRecordKey(reviewRecordId));
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

function creatorReviewRecordKey(
  reviewRecordId: string,
): readonly ["creator-review-record", string] {
  return ["creator-review-record", reviewRecordId] as const;
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

function fetchCreatorReviewRecordByKey(
  key: readonly ["creator-review-record", string],
) {
  return getChallengeReviewRecord(key[1]);
}

function fetchCreatorOwnerBundleByKey(
  key: readonly ["creator-owner-bundle", string, string],
) {
  return fetchCreatorOwnerBundle({
    challengeName: key[1],
    target: key[2] || undefined,
  });
}
