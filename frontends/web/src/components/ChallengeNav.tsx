"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";

/** Renders the challenge nav component. */
export function ChallengeNav({
  challengeId,
  defaultTarget,
}: {
  challengeId: string;
  defaultTarget: string;
}) {
  const pathname = usePathname();
  const t = useTranslations("challenge");
  const base = `/challenges/${challengeId}`;

  const tabs = [
    { href: base, label: t("overview"), end: true },
    { href: `${base}/solution-submissions`, label: t("submissions") },
    {
      href: `${base}/leaderboard?target=${encodeURIComponent(defaultTarget)}`,
      label: t("leaderboard"),
      match: `${base}/leaderboard`,
    },
  ];

  return (
    <div className="tab-list">
      {tabs.map((tab) => {
        const activeHref = "match" in tab && tab.match ? tab.match : tab.href;
        const isActive = tab.end
          ? pathname === tab.href
          : pathname.startsWith(activeHref);
        return (
          <Link
            key={tab.href}
            href={tab.href}
            className={`tab-link${isActive ? " active" : ""}`}
          >
            {tab.label}
          </Link>
        );
      })}
    </div>
  );
}
