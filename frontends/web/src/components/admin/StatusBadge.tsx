/** Renders a compact status label using the admin visual token classes. */
export function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const className =
    normalized === "completed" ||
    normalized === "active" ||
    normalized === "idle"
      ? "badge-success"
      : normalized === "failed" ||
          normalized === "error" ||
          normalized === "disabled"
        ? "badge-error"
        : normalized === "running" ||
            normalized === "queued" ||
            normalized === "pending"
          ? "badge-warning"
          : "badge-default";

  return <span className={`badge ${className}`}>{status}</span>;
}
