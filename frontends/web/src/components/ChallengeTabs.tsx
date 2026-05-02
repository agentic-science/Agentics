"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

/** Navigation tabs shared by every challenge detail subpage. */
export function ChallengeTabs({ challengeId }: { challengeId: string }) {
  const pathname = usePathname();
  const base = `/challenges/${challengeId}`;

  const tabs = [
    { href: base, label: "题面", end: true },
    { href: `${base}/solution-submissions`, label: "提交" },
    { href: `${base}/leaderboard`, label: "排行榜" },
    { href: `${base}/discussions`, label: "讨论" },
  ];

  return (
    <div className="tab-row">
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
