"use client";

import { ArrowLeft, ArrowRight } from "lucide-react";
import Link from "next/link";
import { ChallengeCatalogCard } from "@/components/ChallengeCatalogCard";
import {
  addKeywordHref,
  buildPaginationSlots,
  type ChallengeCatalogFilters,
  challengeCatalogApiQuery,
  challengeCatalogHref,
} from "@/lib/challengeCatalog";
import {
  type ChallengeListResponse,
  challengeListResponseSchema,
} from "@/lib/schemas";
import { usePublicLiveJson } from "@/lib/usePublicLiveJson";

type ChallengeCatalogLabels = {
  empty: string;
  error: string;
  next: string;
  pagination: string;
  previous: string;
};

function challengeCatalogSignature(challenges: ChallengeListResponse) {
  return [
    challenges.total_count,
    challenges.has_more ? "more" : "end",
    challenges.offset,
    ...challenges.items.map((challenge) => challenge.challenge_name),
  ].join(":");
}

/** Renders live-updating challenge catalog results and pagination. */
export function ChallengeCatalogLive({
  filters,
  initialChallenges,
  initialError,
  labels,
  locale,
  page,
}: {
  filters: ChallengeCatalogFilters;
  initialChallenges: ChallengeListResponse;
  initialError: string | null;
  labels: ChallengeCatalogLabels;
  locale: string;
  page: number;
}) {
  const apiPath = `/api/public/challenges?${challengeCatalogApiQuery(
    page,
    filters,
  )}`;
  const { data, error, isValidating } = usePublicLiveJson({
    fallbackData: initialChallenges,
    path: apiPath,
    schema: challengeListResponseSchema,
    surface: "challenge catalog",
    updatedDetails: (challenges) => ({
      has_more: challenges.has_more,
      items: challenges.items.length,
      offset: challenges.offset,
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
  const totalPages = Math.max(
    1,
    Math.ceil(challenges.total_count / challenges.limit),
  );
  const paginationSlots = buildPaginationSlots(page, totalPages);

  return (
    <>
      {message && !hasChallengeData ? (
        <div className="card text-center py-12 text-[var(--status-error)]">
          {labels.error}: {message}
        </div>
      ) : !hasChallengeData ? (
        <div className="empty-state">
          <p className="text-[var(--text-muted)]">{labels.empty}</p>
        </div>
      ) : (
        <div
          className="challenge-catalog-grid live-refresh-region"
          data-refreshing={isValidating ? "true" : "false"}
        >
          <div
            className="home-challenge-grid live-refresh-frame"
            key={challengeCatalogSignature(challenges)}
          >
            {challenges.items.map((challenge) => (
              <ChallengeCatalogCard
                key={challenge.challenge_name}
                challenge={challenge}
                locale={locale}
                showKeywords
                keywordHref={(keyword) =>
                  addKeywordHref(filters.q, filters.keywords, keyword)
                }
              />
            ))}
          </div>
        </div>
      )}

      <nav
        className="challenge-catalog-pagination"
        aria-label={labels.pagination}
      >
        {page > 1 ? (
          <Link
            href={challengeCatalogHref(page - 1, filters)}
            className="challenge-catalog-pagination-button"
          >
            <ArrowLeft className="w-4 h-4" />
            {labels.previous}
          </Link>
        ) : (
          <span className="challenge-catalog-pagination-button is-disabled">
            <ArrowLeft className="w-4 h-4" />
            {labels.previous}
          </span>
        )}

        <div
          className="challenge-catalog-page-list live-refresh-frame"
          key={`pages-${totalPages}-${page}`}
        >
          {paginationSlots.map((slot) =>
            typeof slot === "number" ? (
              <Link
                key={slot}
                href={challengeCatalogHref(slot, filters)}
                aria-current={slot === page ? "page" : undefined}
                className={
                  slot === page
                    ? "challenge-catalog-page-number is-current"
                    : "challenge-catalog-page-number"
                }
              >
                {slot}
              </Link>
            ) : (
              <span key={slot} className="challenge-catalog-page-ellipsis">
                ...
              </span>
            ),
          )}
        </div>

        {challenges.has_more ? (
          <Link
            href={challengeCatalogHref(page + 1, filters)}
            className="challenge-catalog-pagination-button"
          >
            {labels.next}
            <ArrowRight className="w-4 h-4" />
          </Link>
        ) : (
          <span className="challenge-catalog-pagination-button is-disabled">
            {labels.next}
            <ArrowRight className="w-4 h-4" />
          </span>
        )}
      </nav>
    </>
  );
}
