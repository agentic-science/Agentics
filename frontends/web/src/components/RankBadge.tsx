/** Renders a compact leaderboard rank marker with shared podium styling. */
export function RankBadge({
  rank,
  size = "md",
}: {
  rank: number;
  size?: "sm" | "md";
}) {
  const sizeClass = size === "sm" ? "w-6 h-6 text-[11px]" : "w-7 h-7 text-xs";
  const toneClass =
    rank === 1
      ? "bg-action/20 text-action-fg"
      : rank === 2
        ? "bg-fg-muted/20 text-fg-muted"
        : rank === 3
          ? "bg-data/20 text-data"
          : "text-fg-muted";

  return (
    <span
      className={`inline-flex items-center justify-center rounded-full font-bold ${sizeClass} ${toneClass}`}
    >
      {rank}
    </span>
  );
}
