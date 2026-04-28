export function formatScore(value: number | null | undefined): string {
  if (value == null) return "n/a";
  if (Number.isInteger(value)) return String(value);
  return value.toFixed(4);
}

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
