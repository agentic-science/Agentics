"use client";

import {
  CheckCircle2,
  RefreshCw,
  RotateCcw,
  Send,
  XCircle,
} from "lucide-react";
import { useTranslations } from "next-intl";
import { Fragment, type ReactNode } from "react";
import { formatDate } from "@/lib/format";
import type {
  AdminChallengePrivateAssetListResponse,
  ChallengeReviewRecordListItem,
} from "@/lib/schemas";
import { StatusBadge } from "./StatusBadge";

type AdminChallengePrivateAssetResponse =
  AdminChallengePrivateAssetListResponse["items"][number];
type ReviewRecordAction =
  | "validate"
  | "approve"
  | "publish"
  | "reject"
  | "abandon";
type ReviewRecordTranslator = ReturnType<typeof useTranslations>;

interface ChallengeReviewRecordTableProps {
  reviewRecords: ChallengeReviewRecordListItem[];
  locale: string;
  csrfToken: string;
  busyReviewRecordId: string | null;
  expandedReviewRecordId: string | null;
  assetRowsByReviewRecordId: Record<
    string,
    AdminChallengePrivateAssetListResponse
  >;
  loadingAssetsReviewRecordId: string | null;
  onToggleAssetRows: (reviewRecordId: string) => Promise<void>;
  onRunReviewRecordAction: (
    reviewRecord: ChallengeReviewRecordListItem,
    action: ReviewRecordAction,
  ) => Promise<void>;
}

/** Renders the admin challenge-review-record review table. */
export function ChallengeReviewRecordTable({
  reviewRecords,
  locale,
  csrfToken,
  busyReviewRecordId,
  expandedReviewRecordId,
  assetRowsByReviewRecordId,
  loadingAssetsReviewRecordId,
  onToggleAssetRows,
  onRunReviewRecordAction,
}: ChallengeReviewRecordTableProps) {
  const t = useTranslations("admin.reviewRecords");

  if (reviewRecords.length === 0) {
    return <div className="empty-state">{t("noReviewRecords")}</div>;
  }

  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>{t("reviewRecord")}</th>
          <th>{t("status")}</th>
          <th>{t("creator")}</th>
          <th>{t("digests")}</th>
          <th>{t("assets")}</th>
          <th>{t("updated")}</th>
          <th>{t("actions")}</th>
        </tr>
      </thead>
      <tbody>
        {reviewRecords.map((reviewRecord) => {
          const busy = busyReviewRecordId === reviewRecord.id;
          const assetRows = assetRowsByReviewRecordId[reviewRecord.id];
          const assetWarning = reviewRecordAssetWarning(
            reviewRecord,
            assetRows,
            t,
          );
          return (
            <Fragment key={reviewRecord.id}>
              <tr>
                <td>
                  <div className="font-medium">
                    {reviewRecord.manifest.title}
                  </div>
                  <div className="font-mono text-caption text-fg-muted">
                    {reviewRecord.challenge_name} · {reviewRecord.request}
                  </div>
                  <a
                    href={reviewRecord.pr_url}
                    target="_blank"
                    rel="noreferrer"
                    className="text-caption text-data hover:underline"
                  >
                    PR #{reviewRecord.pr_number}
                  </a>
                </td>
                <td>
                  <LocalizedStatusBadge status={reviewRecord.status} />
                </td>
                <td>
                  <div className="font-mono">
                    {reviewRecord.creator_github_login}
                  </div>
                  <div className="text-caption text-fg-muted">
                    {reviewRecord.creator_github_user_id}
                  </div>
                </td>
                <td className="font-mono text-caption">
                  <Digest
                    displayLabel={t("manifestDigest")}
                    value={reviewRecord.manifest_sha256}
                  />
                  <Digest
                    displayLabel={t("validatedDigest")}
                    value={reviewRecord.validation_bundle_sha256}
                  />
                  <Digest
                    displayLabel={t("approvedDigest")}
                    value={reviewRecord.approved_bundle_sha256}
                  />
                </td>
                <td>
                  <div className="font-mono">
                    {t("activeAssets", {
                      count: reviewRecord.private_assets.length,
                    })}
                  </div>
                  <div className="text-caption text-fg-muted">
                    {t("declaredAssets", {
                      count: reviewRecord.manifest.private_assets.length,
                    })}
                  </div>
                  {assetWarning ? (
                    <div className="mt-1 text-caption text-warning">
                      {assetWarning}
                    </div>
                  ) : null}
                  <button
                    type="button"
                    className="mt-2 text-caption text-data hover:underline"
                    onClick={() => void onToggleAssetRows(reviewRecord.id)}
                    disabled={!csrfToken}
                  >
                    {expandedReviewRecordId === reviewRecord.id
                      ? t("hideLifecycle")
                      : t("inspectLifecycle")}
                  </button>
                </td>
                <td className="text-fg-muted">
                  {formatDate(reviewRecord.updated_at, locale)}
                </td>
                <td>
                  <div className="flex flex-wrap gap-2">
                    <ActionButton
                      label={t("validate")}
                      icon={<RefreshCw className="w-3 h-3" />}
                      disabled={busy || !csrfToken || !!assetWarning}
                      onClick={() =>
                        void onRunReviewRecordAction(reviewRecord, "validate")
                      }
                    />
                    <ActionButton
                      label={t("approve")}
                      icon={<CheckCircle2 className="w-3 h-3" />}
                      disabled={
                        busy ||
                        !csrfToken ||
                        !!assetWarning ||
                        !reviewRecord.validation_bundle_sha256
                      }
                      onClick={() =>
                        void onRunReviewRecordAction(reviewRecord, "approve")
                      }
                    />
                    <ActionButton
                      label={t("publish")}
                      icon={<Send className="w-3 h-3" />}
                      disabled={busy || !csrfToken || !!assetWarning}
                      onClick={() =>
                        void onRunReviewRecordAction(reviewRecord, "publish")
                      }
                    />
                    <ActionButton
                      label={t("reject")}
                      icon={<XCircle className="w-3 h-3" />}
                      disabled={busy || !csrfToken}
                      onClick={() =>
                        void onRunReviewRecordAction(reviewRecord, "reject")
                      }
                      danger
                    />
                    <ActionButton
                      label={t("abandon")}
                      icon={<RotateCcw className="w-3 h-3" />}
                      disabled={busy || !csrfToken}
                      onClick={() =>
                        void onRunReviewRecordAction(reviewRecord, "abandon")
                      }
                    />
                  </div>
                </td>
              </tr>
              {expandedReviewRecordId === reviewRecord.id ? (
                <tr>
                  <td colSpan={7}>
                    <PrivateAssetLifecycleTable
                      assets={assetRows?.items ?? []}
                      loading={loadingAssetsReviewRecordId === reviewRecord.id}
                      locale={locale}
                    />
                  </td>
                </tr>
              ) : null}
            </Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

function reviewRecordAssetWarning(
  reviewRecord: ChallengeReviewRecordListItem,
  lifecycleRows: AdminChallengePrivateAssetListResponse | undefined,
  t: ReviewRecordTranslator,
): string | null {
  const activeNames = new Set(
    reviewRecord.private_assets.map((asset) => asset.asset_name),
  );
  const requiredManifestNames = reviewRecord.manifest.private_assets
    .filter((asset) => asset.required)
    .map((asset) => asset.asset_name);
  const missing = requiredManifestNames.filter(
    (name) => !activeNames.has(name),
  );
  if (missing.length > 0) {
    return t("missingRequiredAsset", { assets: missing.join(", ") });
  }

  const nonActiveRequired = lifecycleRows?.items
    .filter((asset) => asset.required && asset.status !== "active")
    .map((asset) => `${asset.asset_name} (${asset.status})`);
  if (nonActiveRequired && nonActiveRequired.length > 0) {
    return t("requiredAssetNotActive", {
      assets: nonActiveRequired.join(", "),
    });
  }

  return null;
}

function PrivateAssetLifecycleTable({
  assets,
  loading,
  locale,
}: {
  assets: AdminChallengePrivateAssetResponse[];
  loading: boolean;
  locale: string;
}) {
  const t = useTranslations("admin.reviewRecords");
  const common = useTranslations("common");

  if (loading) {
    return (
      <div className="rounded-control border border-line bg-surface-2 p-4 text-fg-muted">
        {t("loadingLifecycle")}
      </div>
    );
  }
  if (assets.length === 0) {
    return (
      <div className="rounded-control border border-line bg-surface-2 p-4 text-fg-muted">
        {t("noLifecycle")}
      </div>
    );
  }

  return (
    <div className="rounded-control border border-line bg-surface-2 p-3 overflow-x-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>{t("asset")}</th>
            <th>{t("status")}</th>
            <th>{t("required")}</th>
            <th>{t("size")}</th>
            <th>{t("digest")}</th>
            <th>{t("storage")}</th>
            <th>{t("updated")}</th>
            <th>{t("failure")}</th>
          </tr>
        </thead>
        <tbody>
          {assets.map((asset) => (
            <tr key={asset.id}>
              <td>
                <div className="font-mono">{asset.asset_name}</div>
                <div className="text-caption text-fg-muted">{asset.kind}</div>
              </td>
              <td>
                <LocalizedStatusBadge status={asset.status} />
              </td>
              <td>{asset.required ? common("yes") : common("no")}</td>
              <td className="font-mono">{asset.size_bytes}</td>
              <td className="font-mono text-caption">
                {asset.sha256.slice(0, 12)}
              </td>
              <td className="font-mono text-caption">
                <div>{asset.storage_key}</div>
                {asset.temporary_storage_key ? (
                  <div className="text-fg-muted">
                    {t("temporaryStorage", {
                      key: asset.temporary_storage_key,
                    })}
                  </div>
                ) : null}
              </td>
              <td className="text-caption text-fg-muted">
                {formatDate(
                  asset.activated_at ?? asset.failed_at ?? asset.created_at,
                  locale,
                )}
              </td>
              <td className="text-caption text-danger">
                {asset.failure_message ?? "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ActionButton({
  label,
  icon,
  disabled,
  danger,
  onClick,
}: {
  label: string;
  icon: ReactNode;
  disabled: boolean;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`btn btn-sm ${danger ? "btn-ghost text-danger" : "btn-secondary"}`}
      onClick={() => void onClick()}
      disabled={disabled}
    >
      {icon}
      {label}
    </button>
  );
}

function Digest({
  displayLabel,
  value,
}: {
  displayLabel: string;
  value: string | undefined;
}) {
  return (
    <div>
      <span className="text-fg-muted">{displayLabel}: </span>
      {value ? value.slice(0, 12) : "—"}
    </div>
  );
}

function LocalizedStatusBadge({ status }: { status: string }) {
  const t = useTranslations("common.statuses");
  const labels: Record<string, string> = {
    active: t("active"),
    abandoned: t("abandoned"),
    approved: t("approved"),
    completed: t("completed"),
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
