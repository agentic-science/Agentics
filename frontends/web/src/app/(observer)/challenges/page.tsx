import { ArrowLeft, ArrowRight } from "lucide-react";
import Link from "next/link";
import { redirect } from "next/navigation";
import { getLocale, getTranslations } from "next-intl/server";
import { ChallengeCatalogCard } from "@/components/ChallengeCatalogCard";
import { ScrollToTop } from "@/components/ScrollToTop";
import { fetchJson } from "@/lib/api";
import {
  type ChallengeListResponse,
  challengeListResponseSchema,
} from "@/lib/schemas";

const CHALLENGE_CATALOG_PAGE_SIZE = 12;

type SearchParams = Record<string, string | string[] | undefined>;
type PageSlot = number | "ellipsis-start" | "ellipsis-end";

/** Selects the first value for a Next.js search parameter. */
function firstSearchParamValue(value: string | string[] | undefined) {
  return Array.isArray(value) ? value[0] : value;
}

/** Parses the requested catalog page, defaulting invalid input to page one. */
function parseCatalogPage(searchParams: SearchParams) {
  const rawPage = firstSearchParamValue(searchParams.page);
  if (!rawPage) {
    return 1;
  }
  if (!/^[1-9][0-9]*$/.test(rawPage)) {
    return 1;
  }
  const page = Number.parseInt(rawPage, 10);
  return Number.isSafeInteger(page) ? page : 1;
}

/** Builds the canonical challenge catalog URL for a one-indexed page. */
function challengeCatalogHref(page: number) {
  return page <= 1 ? "/challenges" : `/challenges?page=${page}`;
}

/** Builds compact numbered pagination with stable ellipsis slots. */
function buildPaginationSlots(currentPage: number, totalPages: number) {
  const pages = new Set([
    1,
    totalPages,
    currentPage - 1,
    currentPage,
    currentPage + 1,
  ]);
  const sortedPages = [...pages]
    .filter((page) => page >= 1 && page <= totalPages)
    .sort((left, right) => left - right);
  const slots: PageSlot[] = [];

  for (const page of sortedPages) {
    const previous = slots.at(-1);
    if (typeof previous === "number" && page - previous > 1) {
      slots.push(previous === 1 ? "ellipsis-start" : "ellipsis-end");
    }
    slots.push(page);
  }

  return slots;
}

/** Renders the paginated public challenge catalog page. */
export default async function ChallengeCatalogPage({
  searchParams,
}: {
  searchParams: Promise<SearchParams>;
}) {
  const [params, t, locale] = await Promise.all([
    searchParams,
    getTranslations(),
    getLocale(),
  ]);
  const page = parseCatalogPage(params);
  const offset = (page - 1) * CHALLENGE_CATALOG_PAGE_SIZE;
  let challenges: ChallengeListResponse;
  let error: string | null = null;

  try {
    challenges = await fetchJson(
      `/api/public/challenges?limit=${CHALLENGE_CATALOG_PAGE_SIZE}&offset=${offset}`,
      challengeListResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : t("common.error");
    challenges = {
      items: [],
      total_count: 0,
      limit: CHALLENGE_CATALOG_PAGE_SIZE,
      offset,
      has_more: false,
    };
  }

  const totalPages = Math.max(
    1,
    Math.ceil(challenges.total_count / CHALLENGE_CATALOG_PAGE_SIZE),
  );
  if (!error && page > totalPages) {
    redirect(challengeCatalogHref(totalPages));
  }

  const paginationSlots = buildPaginationSlots(page, totalPages);

  return (
    <div className="challenge-catalog">
      <ScrollToTop key={`challenges-${page}`} />
      <section className="challenge-catalog-header">
        <div>
          <h1 className="challenge-catalog-title">
            {t("challengeCatalog.title")}
          </h1>
        </div>
      </section>

      {error ? (
        <div className="card text-center py-12 text-[var(--status-error)]">
          {t("common.error")}: {error}
        </div>
      ) : challenges.items.length === 0 ? (
        <div className="empty-state">
          <p className="text-[var(--text-muted)]">
            {t("challengeCatalog.empty")}
          </p>
        </div>
      ) : (
        <div className="challenge-catalog-grid">
          <div className="home-challenge-grid">
            {challenges.items.map((challenge) => (
              <ChallengeCatalogCard
                key={challenge.name}
                challenge={challenge}
                locale={locale}
              />
            ))}
          </div>
        </div>
      )}

      <nav className="challenge-catalog-pagination" aria-label="Pagination">
        {page > 1 ? (
          <Link
            href={challengeCatalogHref(page - 1)}
            className="challenge-catalog-pagination-button"
          >
            <ArrowLeft className="w-4 h-4" />
            {t("challengeCatalog.previous")}
          </Link>
        ) : (
          <span className="challenge-catalog-pagination-button is-disabled">
            <ArrowLeft className="w-4 h-4" />
            {t("challengeCatalog.previous")}
          </span>
        )}

        <div className="challenge-catalog-page-list">
          {paginationSlots.map((slot) =>
            typeof slot === "number" ? (
              <Link
                key={slot}
                href={challengeCatalogHref(slot)}
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
            href={challengeCatalogHref(page + 1)}
            className="challenge-catalog-pagination-button"
          >
            {t("challengeCatalog.next")}
            <ArrowRight className="w-4 h-4" />
          </Link>
        ) : (
          <span className="challenge-catalog-pagination-button is-disabled">
            {t("challengeCatalog.next")}
            <ArrowRight className="w-4 h-4" />
          </span>
        )}
      </nav>
    </div>
  );
}
