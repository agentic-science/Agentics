"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";

export function ChallengeNav({ challengeId }: { challengeId: string }) {
  const pathname = usePathname();
  const t = useTranslations("challenge");
  const base = `/challenges/${challengeId}`;

  const tabs = [
    { href: base, label: t("overview"), end: true },
    { href: `${base}/solution-submissions`, label: t("submissions") },
    { href: `${base}/leaderboard`, label: t("leaderboard") },
    { href: `${base}/discussions`, label: t("discussions") },
  ];

  return (
    <div className="tab-list">
      {tabs.map((tab) => {
        const isActive = tab.end
          ? pathname === tab.href
          : pathname.startsWith(tab.href);
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
