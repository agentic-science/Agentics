/** Renders a compact leaderboard rank marker with shared podium styling. */
export function RankBadge({
  rank,
  size = "md",
}: {
  rank: number;
  size?: "sm" | "md";
}) {
  const sizeClass =
    size === "sm"
      ? "w-6 h-6 text-[11px]"
      : "w-7 h-7 text-xs";
  const toneClass =
    rank === 1
      ? "bg-[var(--accent-primary-500)]/20 text-[var(--accent-primary-text)]"
      : rank === 2
        ? "bg-[var(--text-muted)]/20 text-[var(--text-muted)]"
        : rank === 3
          ? "bg-[var(--accent-secondary-500)]/20 text-[var(--accent-secondary-text)]"
          : "text-[var(--text-muted)]";

  return (
    <span
      className={`inline-flex items-center justify-center rounded-full font-bold ${sizeClass} ${toneClass}`}
    >
      {rank}
    </span>
  );
}
