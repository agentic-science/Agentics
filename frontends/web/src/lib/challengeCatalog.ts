export const CHALLENGE_CATALOG_PAGE_SIZE = 12;

export type ChallengeCatalogFilters = {
  keywords: string[];
  q?: string;
};

export type ChallengeCatalogPageSlot =
  | number
  | "ellipsis-start"
  | "ellipsis-end";

/** Builds the canonical challenge catalog URL for a one-indexed page. */
export function challengeCatalogHref(
  page: number,
  filters: Partial<ChallengeCatalogFilters> = {},
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
export function challengeCatalogApiQuery(
  page: number,
  filters: ChallengeCatalogFilters,
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

/** Adds one keyword to the current catalog filter set and resets pagination. */
export function addKeywordHref(
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
export function removeKeywordHref(
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
export function buildPaginationSlots(
  currentPage: number,
  totalPages: number,
): ChallengeCatalogPageSlot[] {
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
  const slots: ChallengeCatalogPageSlot[] = [];

  for (const page of sortedPages) {
    const previous = slots.at(-1);
    if (typeof previous === "number" && page - previous > 1) {
      slots.push(previous === 1 ? "ellipsis-start" : "ellipsis-end");
    }
    slots.push(page);
  }

  return slots;
}
