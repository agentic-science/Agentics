/** Returns the shared visual badge class for a platform status value. */
export function statusBadgeClass(status: string): string {
  const normalized = status.toLowerCase();
  if (
    normalized === "completed" ||
    normalized === "active" ||
    normalized === "idle" ||
    normalized === "published" ||
    normalized === "approved" ||
    normalized === "validated" ||
    normalized === "passed"
  ) {
    return "badge-success";
  }
  if (
    normalized === "failed" ||
    normalized === "error" ||
    normalized === "disabled" ||
    normalized === "rejected" ||
    normalized === "revoked" ||
    normalized === "abandoned"
  ) {
    return "badge-error";
  }
  if (
    normalized === "running" ||
    normalized === "queued" ||
    normalized === "pending" ||
    normalized === "draft" ||
    normalized === "publishing"
  ) {
    return "badge-warning";
  }
  return "badge-default";
}

/** Renders a compact status label using shared badge classes. */
export function StatusBadge({
  status,
  children,
}: {
  status: string;
  children?: ReactNode;
}) {
  return (
    <span className={`badge ${statusBadgeClass(status)}`}>
      {children ?? status}
    </span>
  );
}

import type { ReactNode } from "react";
