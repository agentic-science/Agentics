"use client";

import { useTranslations } from "next-intl";
import { StatusBadge } from "@/components/admin/StatusBadge";

/** Renders a localized status badge for known platform statuses. */
export function LocalizedStatusBadge({ status }: { status: string }) {
  const t = useTranslations("common.statuses");
  const labels: Record<string, string> = {
    active: t("active"),
    abandoned: t("abandoned"),
    approved: t("approved"),
    completed: t("completed"),
    deleted: t("deleted"),
    disabled: t("disabled"),
    pending_review: t("pending_review"),
    failed: t("failed"),
    passed: t("passed"),
    pending: t("pending"),
    published: t("published"),
    publishing: t("publishing"),
    queued: t("queued"),
    rejected: t("rejected"),
    revoked: t("revoked"),
    running: t("running"),
    validated: t("validated"),
  };
  return <StatusBadge status={status}>{labels[status] ?? status}</StatusBadge>;
}
