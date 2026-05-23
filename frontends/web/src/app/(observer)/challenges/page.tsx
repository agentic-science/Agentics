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
function challengeCatalogHref(
  page: number,
  filters: { q?: string; keywords?: string[] } = {},
) {
  const params = new URLSearchParams();
  if (page > 1) {
    params.set("page", page.toString());
  }
  if (filters.q) {
    params.set("q", filters.q);
  }
  for (const keyword of filters.keywords ?? []) {
    params.append("keyword", keyword);
  }
  const query = params.toString();
  return query ? `/challenges?${query}` : "/challenges";
}

/** Builds API query parameters for a filtered challenge catalog request. */
function challengeCatalogApiQuery(
  page: number,
  filters: { q?: string; keywords: string[] },
) {
  const params = new URLSearchParams({
    limit: CHALLENGE_CATALOG_PAGE_SIZE.toString(),
    offset: ((page - 1) * CHALLENGE_CATALOG_PAGE_SIZE).toString(),
  });
  if (filters.q) {
    params.set("q", filters.q);
  }
  for (const keyword of filters.keywords) {
    params.append("keyword", keyword);
  }
  return params.toString();
}

/** Collects one or more URL query values from a Next.js search param. */
function searchParamValues(value: string | string[] | undefined) {
  if (!value) {
    return [];
  }
  return Array.isArray(value) ? value : [value];
}

/** Returns a trim-normalized value for query strings and form defaults. */
function normalizedQueryValue(value: string | undefined) {
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : undefined;
}

/** Deduplicates selected keyword filters while preserving display text. */
function selectedKeywords(searchParams: SearchParams) {
  const keywords: string[] = [];
  const seen = new Set<string>();
  for (const rawKeyword of searchParamValues(searchParams.keyword)) {
    const keyword = rawKeyword.trim();
    const key = keyword.toLocaleLowerCase();
    if (keyword && !seen.has(key)) {
      seen.add(key);
      keywords.push(keyword);
    }
  }
  return keywords;
}

/** Adds one keyword to the current catalog filter set and resets pagination. */
function addKeywordHref(
  q: string | undefined,
  keywords: string[],
  keyword: string,
) {
  const nextKeywords = keywords.some(
    (existing) => existing.toLocaleLowerCase() === keyword.toLocaleLowerCase(),
  )
    ? keywords
    : [...keywords, keyword];
  return challengeCatalogHref(1, { q, keywords: nextKeywords });
}

/** Removes one keyword from the current catalog filter set and resets pagination. */
function removeKeywordHref(
  q: string | undefined,
  keywords: string[],
  keyword: string,
) {
  return challengeCatalogHref(1, {
    q,
    keywords: keywords.filter(
      (existing) =>
        existing.toLocaleLowerCase() !== keyword.toLocaleLowerCase(),
    ),
  });
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
  const q = normalizedQueryValue(firstSearchParamValue(params.q));
  const keywords = selectedKeywords(params);
  const filters = { q, keywords };
  const offset = (page - 1) * CHALLENGE_CATALOG_PAGE_SIZE;
  let challenges: ChallengeListResponse;
  let error: string | null = null;

  try {
    challenges = await fetchJson(
      `/api/public/challenges?${challengeCatalogApiQuery(page, filters)}`,
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
    redirect(challengeCatalogHref(totalPages, filters));
  }

  const paginationSlots = buildPaginationSlots(page, totalPages);
  const hasActiveFilters = Boolean(q) || keywords.length > 0;

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

      <section className="challenge-catalog-search-panel">
        <form action="/challenges" className="challenge-catalog-search-form">
          <label className="challenge-catalog-search-field">
            <span>{t("challengeCatalog.searchLabel")}</span>
            <input
              type="search"
              name="q"
              defaultValue={q ?? ""}
              placeholder={t("challengeCatalog.searchPlaceholder")}
            />
          </label>
          {keywords.map((keyword) => (
            <input key={keyword} type="hidden" name="keyword" value={keyword} />
          ))}
          <button type="submit" className="btn btn-primary">
            {t("challengeCatalog.searchButton")}
          </button>
          {hasActiveFilters ? (
            <Link href="/challenges" className="btn btn-secondary">
              {t("challengeCatalog.clearFilters")}
            </Link>
          ) : null}
        </form>

        {keywords.length > 0 ? (
          <div className="challenge-catalog-active-keywords">
            <span>{t("challengeCatalog.activeKeywords")}</span>
            {keywords.map((keyword) => (
              <Link
                key={keyword}
                href={removeKeywordHref(q, keywords, keyword)}
                className="challenge-catalog-active-keyword"
              >
                {keyword}
                <span aria-hidden="true">×</span>
              </Link>
            ))}
          </div>
        ) : null}
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
                key={challenge.challenge_id}
                challenge={challenge}
                locale={locale}
                showKeywords
                keywordHref={(keyword) => addKeywordHref(q, keywords, keyword)}
              />
            ))}
          </div>
        </div>
      )}

      <nav
        className="challenge-catalog-pagination"
        aria-label={t("common.pagination")}
      >
        {page > 1 ? (
          <Link
            href={challengeCatalogHref(page - 1, filters)}
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
