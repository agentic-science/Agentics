import Link from "next/link";
import { redirect } from "next/navigation";
import { getLocale, getTranslations } from "next-intl/server";
import { ChallengeCatalogLive } from "@/components/ChallengeCatalogLive";
import { ScrollToTop } from "@/components/ScrollToTop";
import { fetchJson } from "@/lib/api";
import {
  CHALLENGE_CATALOG_PAGE_SIZE,
  challengeCatalogApiQuery,
  challengeCatalogHref,
  removeKeywordHref,
} from "@/lib/challengeCatalog";
import {
  type ChallengeListResponse,
  challengeListResponseSchema,
} from "@/lib/schemas";

type SearchParams = Record<string, string | string[] | undefined>;

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

      <ChallengeCatalogLive
        filters={filters}
        initialChallenges={challenges}
        initialError={error}
        labels={{
          empty: t("challengeCatalog.empty"),
          error: t("common.error"),
          next: t("challengeCatalog.next"),
          pagination: t("common.pagination"),
          previous: t("challengeCatalog.previous"),
        }}
        locale={locale}
        page={page}
      />
    </div>
  );
}
