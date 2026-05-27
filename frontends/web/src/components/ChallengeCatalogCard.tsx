import { ArrowRight, MessageCircle } from "lucide-react";
import Link from "next/link";
import { selectLocalizedText } from "@/lib/localizedText";
import type { ChallengeListResponse } from "@/lib/schemas";

type ChallengeListItem = ChallengeListResponse["items"][number];

type ChallengeCatalogCardProps = {
  /** Challenge catalog row returned by the public list endpoint. */
  challenge: ChallengeListItem;
  /** Active locale used to select the bilingual challenge summary. */
  locale: string;
  /** Optional builder for keyword filter links. */
  keywordHref?: (keyword: string) => string;
  /** Whether public keyword pills should be shown on this card. */
  showKeywords?: boolean;
};

/** Renders one public challenge card for catalog grids and previews. */
export function ChallengeCatalogCard({
  challenge,
  locale,
  keywordHref,
  showKeywords = false,
}: ChallengeCatalogCardProps) {
  return (
    <article className="home-challenge-card card group flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <Link
          href={`/challenges/${challenge.challenge_name}`}
          className="text-fg group-hover:text-action-fg transition-colors"
        >
          <h3 className="text-h3 font-semibold leading-h3">
            {challenge.title}
          </h3>
        </Link>
        <span className="badge badge-default shrink-0">
          {challenge.eligibility.type}
        </span>
      </div>
      <p className="text-body-sm text-fg-muted leading-body-sm line-clamp-2">
        {selectLocalizedText(challenge.summary, locale)}
      </p>
      {showKeywords && challenge.keywords.length > 0 ? (
        <div className="challenge-card-keywords">
          {challenge.keywords.map((keyword) =>
            keywordHref ? (
              <Link
                key={keyword}
                href={keywordHref(keyword)}
                className="challenge-card-keyword"
              >
                {keyword}
              </Link>
            ) : (
              <span key={keyword} className="challenge-card-keyword">
                {keyword}
              </span>
            ),
          )}
        </div>
      ) : null}
      <div className="flex items-center gap-2 mt-auto pt-2">
        {challenge.moltbook_discussion_url ? (
          <a
            href={challenge.moltbook_discussion_url}
            target="_blank"
            rel="noreferrer"
            aria-label="Open Moltbook discussion"
            title="Moltbook discussion"
            className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-control border border-line text-fg-muted transition-colors hover:border-line-medium hover:text-action-fg"
          >
            <MessageCircle className="h-3.5 w-3.5" />
          </a>
        ) : null}
        <Link
          href={`/challenges/${challenge.challenge_name}`}
          className="flex min-w-0 flex-1 items-center gap-2"
        >
          <span className="home-challenge-name-chip min-w-0 truncate text-caption text-fg-muted font-mono group-hover:border-line-medium transition-colors">
            {challenge.challenge_name}
          </span>
          <ArrowRight className="w-3.5 h-3.5 text-fg-muted group-hover:text-action-fg group-hover:translate-x-0.5 transition-all ml-auto shrink-0" />
        </Link>
      </div>
    </article>
  );
}
