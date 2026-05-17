/** Renders a compact status label using the admin visual token classes. */
export function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const className =
    normalized === "completed" ||
    normalized === "active" ||
    normalized === "idle" ||
    normalized === "published" ||
    normalized === "approved" ||
    normalized === "validated"
      ? "badge-success"
      : normalized === "failed" ||
          normalized === "error" ||
          normalized === "disabled" ||
          normalized === "rejected" ||
          normalized === "revoked" ||
          normalized === "abandoned"
        ? "badge-error"
        : normalized === "running" ||
            normalized === "queued" ||
            normalized === "pending"
          ? "badge-warning"
          : "badge-default";

  return <span className={`badge ${className}`}>{status}</span>;
}
