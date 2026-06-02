"use client";

import Link from "next/link";
import { ChallengeCatalogCard } from "@/components/ChallengeCatalogCard";
import {
  type ChallengeListResponse,
  challengeListResponseSchema,
} from "@/lib/schemas";
import { usePublicLiveJson } from "@/lib/usePublicLiveJson";

function challengePreviewSignature(challenges: ChallengeListResponse) {
  return [
    challenges.total_count,
    challenges.has_more ? "more" : "end",
    ...challenges.items.map((challenge) => challenge.challenge_name),
  ].join(":");
}

/** Renders the live-updating home challenge preview grid. */
export function HomeChallengePreview({
  emptyLabel,
  errorLabel,
  initialChallenges,
  initialError,
  locale,
  moreLabel,
  previewLimit,
}: {
  emptyLabel: string;
  errorLabel: string;
  initialChallenges: ChallengeListResponse;
  initialError: string | null;
  locale: string;
  moreLabel: string;
  previewLimit: number;
}) {
  const apiPath = `/api/public/challenges?limit=${previewLimit}&offset=0`;
  const { data, error, isValidating } = usePublicLiveJson({
    fallbackData: initialChallenges,
    path: apiPath,
    schema: challengeListResponseSchema,
    surface: "home challenge preview",
    updatedDetails: (challenges) => ({
      has_more: challenges.has_more,
      items: challenges.items.length,
      total: challenges.total_count,
    }),
  });
  const challenges = data ?? initialChallenges;
  const hasLoadedRemoteData = data !== initialChallenges;
  const hasChallengeData = challenges.items.length > 0;
  const message =
    !hasChallengeData && error
      ? error instanceof Error
        ? error.message
        : String(error)
      : hasLoadedRemoteData
        ? null
        : initialError;
  const shouldFadeChallengePreview = challenges.items.length > 9;

  if (message && !hasChallengeData) {
    return (
      <div className="card text-center py-12 text-[var(--status-error)]">
        {errorLabel}: {message}
      </div>
    );
  }

  if (!hasChallengeData) {
    return (
      <div className="empty-state">
        <p className="text-[var(--text-muted)]">{emptyLabel}</p>
      </div>
    );
  }

  return (
    <div
      className={
        shouldFadeChallengePreview
          ? "home-challenge-preview home-challenge-preview-fade live-refresh-region"
          : "home-challenge-preview live-refresh-region"
      }
      data-refreshing={isValidating ? "true" : "false"}
    >
      <div
        className="live-refresh-frame"
        key={challengePreviewSignature(challenges)}
      >
        <div className="home-challenge-grid">
          {challenges.items.map((challenge) => (
            <ChallengeCatalogCard
              key={challenge.challenge_name}
              challenge={challenge}
              locale={locale}
            />
          ))}
        </div>
      </div>
      {shouldFadeChallengePreview ? (
        <Link href="/challenges" className="home-challenge-more-pill">
          {moreLabel}
        </Link>
      ) : null}
    </div>
  );
}
