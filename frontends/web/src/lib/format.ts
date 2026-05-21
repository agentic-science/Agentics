/** Format score-like numeric values for compact leaderboard and submission tables. */
export function formatScore(value: number | null | undefined): string {
  if (value == null) return "n/a";
  if (Number.isInteger(value)) return String(value);
  return value.toFixed(4);
}

/** Format arbitrary metric values with an optional display unit. */
export function formatMetricValue(
  value: number | null | undefined,
  unit?: string,
): string {
  if (value == null) return "n/a";
  const formatted = Number.isInteger(value) ? String(value) : value.toFixed(4);
  return unit ? `${formatted} ${unit}` : formatted;
}

/** Format API timestamps in the same locale used by the current route. */
export function formatDate(value: string, locale = "en"): string {
  const d = new Date(value);
  return d.toLocaleString(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
