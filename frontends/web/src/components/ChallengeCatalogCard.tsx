import { ArrowRight } from "lucide-react";
import Link from "next/link";
import { selectLocalizedText } from "@/lib/localizedText";
import type { ChallengeListResponse } from "@/lib/schemas";

type ChallengeListItem = ChallengeListResponse["items"][number];

type ChallengeCatalogCardProps = {
  /** Challenge catalog row returned by the public list endpoint. */
  challenge: ChallengeListItem;
  /** Active locale used to select the bilingual challenge summary. */
  locale: string;
};

/** Renders one public challenge card for catalog grids and previews. */
export function ChallengeCatalogCard({
  challenge,
  locale,
}: ChallengeCatalogCardProps) {
  return (
    <Link
      href={`/challenges/${challenge.name}`}
      className="home-challenge-card card group flex flex-col gap-3"
    >
      <div className="flex items-start justify-between gap-3">
        <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] group-hover:text-[var(--accent-primary-text)] transition-colors leading-[var(--leading-h3)]">
          {challenge.title}
        </h3>
        <span className="badge badge-default shrink-0">
          {challenge.eligibility.type}
        </span>
      </div>
      <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] leading-[var(--leading-body-sm)] line-clamp-2">
        {selectLocalizedText(challenge.summary, locale)}
      </p>
      <div className="flex items-center gap-2 mt-auto pt-2">
        <span className="home-challenge-name-chip text-[var(--text-caption)] text-[var(--text-muted)] font-mono">
          {challenge.name}
        </span>
        <ArrowRight className="w-3.5 h-3.5 text-[var(--text-muted)] group-hover:text-[var(--accent-primary-text)] group-hover:translate-x-0.5 transition-all ml-auto" />
      </div>
    </Link>
  );
}
