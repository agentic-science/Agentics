"use client";

import {
  CheckCircle2,
  GitPullRequest,
  RefreshCw,
  RotateCcw,
  Send,
  Trash2,
  XCircle,
} from "lucide-react";
import { Fragment, type ReactNode, useState } from "react";
import type { ZodType } from "zod";
import {
  ConsoleSectionTitle as SectionTitle,
  ConsoleTextInput as TextInput,
} from "@/components/ConsolePrimitives";
import {
  AdminApiError,
  adminFetchJson,
  listAdminChallengeDraftPrivateAssets,
} from "@/lib/adminApi";
import { formatDate } from "@/lib/format";
import {
  type AdminChallengePrivateAssetListResponse,
  type ChallengeDraftListItem,
  challengeDraftCleanupResponseSchema,
  challengeDraftResponseSchema,
  type ReviewChallengeDraftRequest,
  reviewChallengeDraftRequestSchema,
  type ValidateChallengeDraftRequest,
  validateChallengeDraftRequestSchema,
} from "@/lib/schemas";
import { StatusBadge } from "./StatusBadge";

type AdminChallengePrivateAssetResponse =
  AdminChallengePrivateAssetListResponse["items"][number];

/** Describes the refresh options shape used by this module. */
type RefreshOptions = { quiet?: boolean };
/** Describes the admin refresh shape used by this module. */
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

/** Describes the challenge draft review panel props shape used by this module. */
interface ChallengeDraftReviewPanelProps {
  csrfToken: string;
  drafts: ChallengeDraftListItem[];
  locale: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

/** Renders the challenge draft review panel component. */
export function ChallengeDraftReviewPanel({
  csrfToken,
  drafts,
  locale,
  onRefresh,
  onError,
  onMessage,
}: ChallengeDraftReviewPanelProps) {
  const [repositoryPath, setRepositoryPath] = useState(
    "challenge-repos/agentics-challenges",
  );
  const [reviewMessage, setReviewMessage] = useState("");
  const [busyDraftId, setBusyDraftId] = useState<string | null>(null);
  const [expandedDraftId, setExpandedDraftId] = useState<string | null>(null);
  const [assetRowsByDraftId, setAssetRowsByDraftId] = useState<
    Record<string, AdminChallengePrivateAssetListResponse>
  >({});
  const [loadingAssetsDraftId, setLoadingAssetsDraftId] = useState<
    string | null
  >(null);

  /** Loads private asset lifecycle rows for one draft on demand. */
  const toggleAssetRows = async (draftId: string) => {
    if (expandedDraftId === draftId) {
      setExpandedDraftId(null);
      return;
    }
    setExpandedDraftId(draftId);
    if (assetRowsByDraftId[draftId] || !csrfToken) {
      return;
    }

    setLoadingAssetsDraftId(draftId);
    try {
      const rows = await listAdminChallengeDraftPrivateAssets(
        draftId,
        csrfToken,
      );
      setAssetRowsByDraftId((current) => ({ ...current, [draftId]: rows }));
    } catch (e) {
      onError(adminErrorMessage(e));
    } finally {
      setLoadingAssetsDraftId(null);
    }
  };

  /** Runs draft action and refreshes affected data. */
  const runDraftAction = async (
    draft: ChallengeDraftListItem,
    action: "validate" | "approve" | "publish" | "reject" | "abandon",
  ) => {
    const draftId = draft.id;
    if (!csrfToken) {
      onError("Sign in before reviewing challenge drafts.");
      return;
    }
    if (
      (action === "validate" || action === "publish") &&
      !repositoryPath.trim()
    ) {
      onError("Repository path is required for validation and publish.");
      return;
    }
    if (!confirmDraftAction(draftId, action)) {
      return;
    }

    setBusyDraftId(draftId);
    try {
      const body: ReviewChallengeDraftRequest | ValidateChallengeDraftRequest =
        action === "validate" || action === "publish"
          ? parseAdminDraftMutationRequest(
              validateChallengeDraftRequestSchema,
              { repository_path: repositoryPath.trim() },
              "Invalid repository path.",
            )
          : parseAdminDraftMutationRequest(
              reviewChallengeDraftRequestSchema,
              {
                message: draftReviewMessage(action, reviewMessage),
                expected_validation_bundle_sha256:
                  action === "approve"
                    ? draft.validation_bundle_sha256
                    : undefined,
              },
              "Invalid review message.",
            );
      const response = await adminFetchJson(
        `/admin/challenge-drafts/${encodeURIComponent(draftId)}/${action}`,
        challengeDraftResponseSchema,
        csrfToken,
        {
          method: "POST",
          body: JSON.stringify(body),
        },
      );
      onError(null);
      onMessage(`Draft ${response.id.slice(0, 8)} ${action} completed.`);
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e));
    } finally {
      setBusyDraftId(null);
    }
  };

  /** Cleans up drafts through the admin API. */
  const cleanupDrafts = async () => {
    if (!csrfToken) {
      onError("Sign in before cleaning up stale drafts.");
      return;
    }
    if (
      !window.confirm(
        "Clean up stale challenge drafts and delete expired private assets?",
      )
    ) {
      return;
    }

    setBusyDraftId("cleanup");
    try {
      const response = await adminFetchJson(
        "/admin/challenge-drafts/cleanup",
        challengeDraftCleanupResponseSchema,
        csrfToken,
        { method: "POST" },
      );
      onError(null);
      onMessage(
        `Cleanup abandoned ${response.abandoned_drafts} drafts and purged ${response.purged_private_assets} assets.`,
      );
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e));
    } finally {
      setBusyDraftId(null);
    }
  };

  return (
    <section className="grid grid-cols-1 gap-6">
      <div className="card">
        <div className="flex flex-col lg:flex-row lg:items-end justify-between gap-5">
          <div>
            <SectionTitle
              icon={<GitPullRequest className="w-4 h-4" />}
              title="Challenge draft review"
            />
            <p className="mt-2 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
              Validate GitHub-backed drafts, freeze approved review digests, and
              publish immutable challenge contracts.
            </p>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-[minmax(260px,1fr)_minmax(200px,280px)_auto] gap-3 w-full lg:w-auto">
            <TextInput
              label="Repository path"
              value={repositoryPath}
              onChange={setRepositoryPath}
            />
            <TextInput
              label="Review message"
              value={reviewMessage}
              onChange={setReviewMessage}
            />
            <button
              type="button"
              className="btn btn-secondary self-end"
              onClick={() => void cleanupDrafts()}
              disabled={!csrfToken || busyDraftId === "cleanup"}
            >
              <Trash2 className="w-4 h-4" />
              Cleanup stale
            </button>
          </div>
        </div>
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <span className="badge badge-default">{drafts.length} rows</span>
        </div>
        {drafts.length === 0 ? (
          <div className="empty-state">No drafts loaded.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Draft</th>
                <th>Status</th>
                <th>Creator</th>
                <th>Digests</th>
                <th>Assets</th>
                <th>Updated</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {drafts.map((draft) => {
                const busy = busyDraftId === draft.id;
                const assetRows = assetRowsByDraftId[draft.id];
                const assetWarning = draftAssetWarning(draft, assetRows);
                return (
                  <Fragment key={draft.id}>
                    <tr>
                      <td>
                        <div className="font-medium">
                          {draft.manifest.title}
                        </div>
                        <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                          {draft.challenge_name} · {draft.request}
                        </div>
                        <a
                          href={draft.pr_url}
                          target="_blank"
                          rel="noreferrer"
                          className="text-[var(--text-caption)] text-[var(--accent-secondary-text)] hover:underline"
                        >
                          PR #{draft.pr_number}
                        </a>
                      </td>
                      <td>
                        <StatusBadge status={draft.status} />
                      </td>
                      <td>
                        <div className="font-mono">
                          {draft.creator_github_login}
                        </div>
                        <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                          {draft.creator_github_user_id}
                        </div>
                      </td>
                      <td className="font-mono text-[var(--text-caption)]">
                        <Digest
                          label="manifest"
                          value={draft.manifest_sha256}
                        />
                        <Digest
                          label="validated"
                          value={draft.validation_bundle_sha256}
                        />
                        <Digest
                          label="approved"
                          value={draft.approved_bundle_sha256}
                        />
                      </td>
                      <td>
                        <div className="font-mono">
                          {draft.private_assets.length} active
                        </div>
                        <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                          {draft.manifest.private_assets.length} declared
                        </div>
                        {assetWarning ? (
                          <div className="mt-1 text-[var(--text-caption)] text-[var(--status-warning)]">
                            {assetWarning}
                          </div>
                        ) : null}
                        <button
                          type="button"
                          className="mt-2 text-[var(--text-caption)] text-[var(--accent-secondary-text)] hover:underline"
                          onClick={() => void toggleAssetRows(draft.id)}
                          disabled={!csrfToken}
                        >
                          {expandedDraftId === draft.id
                            ? "Hide lifecycle"
                            : "Inspect lifecycle"}
                        </button>
                      </td>
                      <td className="text-[var(--text-muted)]">
                        {formatDate(draft.updated_at, locale)}
                      </td>
                      <td>
                        <div className="flex flex-wrap gap-2">
                          <ActionButton
                            label="Validate"
                            icon={<RefreshCw className="w-3 h-3" />}
                            disabled={busy || !csrfToken || !!assetWarning}
                            onClick={() =>
                              void runDraftAction(draft, "validate")
                            }
                          />
                          <ActionButton
                            label="Approve"
                            icon={<CheckCircle2 className="w-3 h-3" />}
                            disabled={
                              busy ||
                              !csrfToken ||
                              !!assetWarning ||
                              !draft.validation_bundle_sha256
                            }
                            onClick={() =>
                              void runDraftAction(draft, "approve")
                            }
                          />
                          <ActionButton
                            label="Publish"
                            icon={<Send className="w-3 h-3" />}
                            disabled={busy || !csrfToken || !!assetWarning}
                            onClick={() =>
                              void runDraftAction(draft, "publish")
                            }
                          />
                          <ActionButton
                            label="Reject"
                            icon={<XCircle className="w-3 h-3" />}
                            disabled={busy || !csrfToken}
                            onClick={() => void runDraftAction(draft, "reject")}
                            danger
                          />
                          <ActionButton
                            label="Abandon"
                            icon={<RotateCcw className="w-3 h-3" />}
                            disabled={busy || !csrfToken}
                            onClick={() =>
                              void runDraftAction(draft, "abandon")
                            }
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
        )}
      </div>
    </section>
  );
}

/** Parses generated request schemas before sending admin draft mutations. */
function parseAdminDraftMutationRequest<T>(
  schema: ZodType<T>,
  value: unknown,
  fallbackMessage: string,
): T {
  const parsed = schema.safeParse(value);
  if (!parsed.success) {
    throw new AdminApiError(
      400,
      parsed.error.issues[0]?.message ?? fallbackMessage,
    );
  }
  return parsed.data;
}

/** Returns an explicit review message that matches the selected action. */
function draftReviewMessage(
  action: "approve" | "reject" | "abandon",
  input: string,
): string {
  const message = input.trim();
  if (message) {
    return message;
  }

  switch (action) {
    case "approve":
      return "approved";
    case "reject":
      return "rejected";
    case "abandon":
      return "abandoned";
  }
}

/** Requires an explicit browser confirmation before high-impact draft actions. */
function confirmDraftAction(
  draftId: string,
  action: "validate" | "approve" | "publish" | "reject" | "abandon",
): boolean {
  const shortId = draftId.slice(0, 8);
  switch (action) {
    case "validate":
      return true;
    case "approve":
      return window.confirm(`Approve draft ${shortId}?`);
    case "publish":
      return window.confirm(`Publish draft ${shortId} as a live challenge?`);
    case "reject":
      return window.confirm(`Reject draft ${shortId}?`);
    case "abandon":
      return window.confirm(`Abandon draft ${shortId}?`);
  }
}

/** Returns a blocking private-asset lifecycle warning for review actions. */
function draftAssetWarning(
  draft: ChallengeDraftListItem,
  lifecycleRows: AdminChallengePrivateAssetListResponse | undefined,
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
    return `Missing required asset: ${missing.join(", ")}`;
  }

  const nonActiveRequired = lifecycleRows?.items
    .filter((asset) => asset.required && asset.status !== "active")
    .map((asset) => `${asset.asset_name} (${asset.status})`);
  if (nonActiveRequired && nonActiveRequired.length > 0) {
    return `Required asset not active: ${nonActiveRequired.join(", ")}`;
  }

  return null;
}

/** Renders the admin-only private asset lifecycle table. */
function PrivateAssetLifecycleTable({
  assets,
  loading,
  locale,
}: {
  assets: AdminChallengePrivateAssetResponse[];
  loading: boolean;
  locale: string;
}) {
  if (loading) {
    return (
      <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] p-4 text-[var(--text-muted)]">
        Loading private asset lifecycle rows...
      </div>
    );
  }
  if (assets.length === 0) {
    return (
      <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] p-4 text-[var(--text-muted)]">
        No private asset lifecycle rows.
      </div>
    );
  }

  return (
    <div className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] p-3 overflow-x-auto">
      <table className="data-table">
        <thead>
          <tr>
            <th>Asset</th>
            <th>Status</th>
            <th>Required</th>
            <th>Size</th>
            <th>Digest</th>
            <th>Storage</th>
            <th>Updated</th>
            <th>Failure</th>
          </tr>
        </thead>
        <tbody>
          {assets.map((asset) => (
            <tr key={asset.id}>
              <td>
                <div className="font-mono">{asset.asset_name}</div>
                <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                  {asset.kind}
                </div>
              </td>
              <td>
                <StatusBadge status={asset.status} />
              </td>
              <td>{asset.required ? "yes" : "no"}</td>
              <td className="font-mono">{asset.size_bytes}</td>
              <td className="font-mono text-[var(--text-caption)]">
                {asset.sha256.slice(0, 12)}
              </td>
              <td className="font-mono text-[var(--text-caption)]">
                <div>{asset.storage_key}</div>
                {asset.temporary_storage_key ? (
                  <div className="text-[var(--text-muted)]">
                    temp {asset.temporary_storage_key}
                  </div>
                ) : null}
              </td>
              <td className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {formatDate(
                  asset.activated_at ?? asset.failed_at ?? asset.created_at,
                  locale,
                )}
              </td>
              <td className="text-[var(--text-caption)] text-[var(--status-error)]">
                {asset.failure_message ?? "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Renders the action button component. */
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
      className={`btn btn-sm ${danger ? "btn-ghost text-[var(--status-error)]" : "btn-secondary"}`}
      onClick={() => void onClick()}
      disabled={disabled}
    >
      {icon}
      {label}
    </button>
  );
}

/** Renders the digest component. */
function Digest({
  label,
  value,
}: {
  label: string;
  value: string | undefined;
}) {
  return (
    <div>
      <span className="text-[var(--text-muted)]">{label}: </span>
      {value ? value.slice(0, 12) : "—"}
    </div>
  );
}

/** Normalizes unknown errors into a displayable message. */
function adminErrorMessage(error: unknown): string {
  if (error instanceof AdminApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unknown draft review error.";
}
