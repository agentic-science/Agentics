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
  ChallengeDraftListItem,
} from "@/lib/schemas";
import { StatusBadge } from "./StatusBadge";

type AdminChallengePrivateAssetResponse =
  AdminChallengePrivateAssetListResponse["items"][number];
type DraftAction = "validate" | "approve" | "publish" | "reject" | "abandon";
type DraftReviewTranslator = ReturnType<typeof useTranslations>;

interface ChallengeDraftReviewTableProps {
  drafts: ChallengeDraftListItem[];
  locale: string;
  csrfToken: string;
  busyDraftId: string | null;
  expandedDraftId: string | null;
  assetRowsByDraftId: Record<string, AdminChallengePrivateAssetListResponse>;
  loadingAssetsDraftId: string | null;
  onToggleAssetRows: (draftId: string) => Promise<void>;
  onRunDraftAction: (
    draft: ChallengeDraftListItem,
    action: DraftAction,
  ) => Promise<void>;
}

/** Renders the admin challenge-draft review table. */
export function ChallengeDraftReviewTable({
  drafts,
  locale,
  csrfToken,
  busyDraftId,
  expandedDraftId,
  assetRowsByDraftId,
  loadingAssetsDraftId,
  onToggleAssetRows,
  onRunDraftAction,
}: ChallengeDraftReviewTableProps) {
  const t = useTranslations("admin.draftReview");

  if (drafts.length === 0) {
    return <div className="empty-state">{t("noDrafts")}</div>;
  }

  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>{t("draft")}</th>
          <th>{t("status")}</th>
          <th>{t("creator")}</th>
          <th>{t("digests")}</th>
          <th>{t("assets")}</th>
          <th>{t("updated")}</th>
          <th>{t("actions")}</th>
        </tr>
      </thead>
      <tbody>
        {drafts.map((draft) => {
          const busy = busyDraftId === draft.id;
          const assetRows = assetRowsByDraftId[draft.id];
          const assetWarning = draftAssetWarning(draft, assetRows, t);
          return (
            <Fragment key={draft.id}>
              <tr>
                <td>
                  <div className="font-medium">{draft.manifest.title}</div>
                  <div className="font-mono text-caption text-fg-muted">
                    {draft.challenge_name} · {draft.request}
                  </div>
                  <a
                    href={draft.pr_url}
                    target="_blank"
                    rel="noreferrer"
                    className="text-caption text-data hover:underline"
                  >
                    PR #{draft.pr_number}
                  </a>
                </td>
                <td>
                  <LocalizedStatusBadge status={draft.status} />
                </td>
                <td>
                  <div className="font-mono">{draft.creator_github_login}</div>
                  <div className="text-caption text-fg-muted">
                    {draft.creator_github_user_id}
                  </div>
                </td>
                <td className="font-mono text-caption">
                  <Digest
                    displayLabel={t("manifestDigest")}
                    value={draft.manifest_sha256}
                  />
                  <Digest
                    displayLabel={t("validatedDigest")}
                    value={draft.validation_bundle_sha256}
                  />
                  <Digest
                    displayLabel={t("approvedDigest")}
                    value={draft.approved_bundle_sha256}
                  />
                </td>
                <td>
                  <div className="font-mono">
                    {t("activeAssets", {
                      count: draft.private_assets.length,
                    })}
                  </div>
                  <div className="text-caption text-fg-muted">
                    {t("declaredAssets", {
                      count: draft.manifest.private_assets.length,
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
                    onClick={() => void onToggleAssetRows(draft.id)}
                    disabled={!csrfToken}
                  >
                    {expandedDraftId === draft.id
                      ? t("hideLifecycle")
                      : t("inspectLifecycle")}
                  </button>
                </td>
                <td className="text-fg-muted">
                  {formatDate(draft.updated_at, locale)}
                </td>
                <td>
                  <div className="flex flex-wrap gap-2">
                    <ActionButton
                      label={t("validate")}
                      icon={<RefreshCw className="w-3 h-3" />}
                      disabled={busy || !csrfToken || !!assetWarning}
                      onClick={() => void onRunDraftAction(draft, "validate")}
                    />
                    <ActionButton
                      label={t("approve")}
                      icon={<CheckCircle2 className="w-3 h-3" />}
                      disabled={
                        busy ||
                        !csrfToken ||
                        !!assetWarning ||
                        !draft.validation_bundle_sha256
                      }
                      onClick={() => void onRunDraftAction(draft, "approve")}
                    />
                    <ActionButton
                      label={t("publish")}
                      icon={<Send className="w-3 h-3" />}
                      disabled={busy || !csrfToken || !!assetWarning}
                      onClick={() => void onRunDraftAction(draft, "publish")}
                    />
                    <ActionButton
                      label={t("reject")}
                      icon={<XCircle className="w-3 h-3" />}
                      disabled={busy || !csrfToken}
                      onClick={() => void onRunDraftAction(draft, "reject")}
                      danger
                    />
                    <ActionButton
                      label={t("abandon")}
                      icon={<RotateCcw className="w-3 h-3" />}
                      disabled={busy || !csrfToken}
                      onClick={() => void onRunDraftAction(draft, "abandon")}
                    />
                  </div>
                </td>
              </tr>
              {expandedDraftId === draft.id ? (
                <tr>
                  <td colSpan={7}>
                    <PrivateAssetLifecycleTable
                      assets={assetRows?.items ?? []}
                      loading={loadingAssetsDraftId === draft.id}
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

function draftAssetWarning(
  draft: ChallengeDraftListItem,
  lifecycleRows: AdminChallengePrivateAssetListResponse | undefined,
  t: DraftReviewTranslator,
): string | null {
  const activeNames = new Set(
    draft.private_assets.map((asset) => asset.asset_name),
  );
  const requiredManifestNames = draft.manifest.private_assets
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
  const t = useTranslations("admin.draftReview");
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
    draft: t("draft"),
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
