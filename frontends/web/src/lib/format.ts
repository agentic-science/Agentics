/** Format normalized scores for compact leaderboard and submission tables. */
export function formatScore(value: number | null | undefined): string {
  if (value == null) return "n/a";
  if (Number.isInteger(value)) return String(value);
  return value.toFixed(4);
}

/** Format API timestamps in the same locale used by the rest of the UI. */
export function formatDate(value: string): string {
  const d = new Date(value);
  return d.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
