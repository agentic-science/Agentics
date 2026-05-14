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
import { type ReactNode, useState } from "react";
import { AdminApiError, adminFetchJson } from "@/lib/adminApi";
import { formatDate } from "@/lib/format";
import {
  type ChallengeDraftListItem,
  challengeDraftCleanupResponseSchema,
  challengeDraftResponseSchema,
} from "@/lib/schemas";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

interface ChallengeDraftReviewPanelProps {
  csrfToken: string;
  drafts: ChallengeDraftListItem[];
  locale: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

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
  const [reviewMessage, setReviewMessage] = useState("approved");
  const [busyDraftId, setBusyDraftId] = useState<string | null>(null);

  const runDraftAction = async (
    draftId: string,
    action: "validate" | "approve" | "publish" | "reject" | "abandon",
  ) => {
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

    setBusyDraftId(draftId);
    try {
      const body =
        action === "validate" || action === "publish"
          ? { repository_path: repositoryPath.trim() }
          : { message: reviewMessage.trim() };
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

  const cleanupDrafts = async () => {
    if (!csrfToken) {
      onError("Sign in before cleaning up stale drafts.");
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
                return (
                  <tr key={draft.id}>
                    <td>
                      <div className="font-medium">{draft.manifest.title}</div>
                      <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                        {draft.challenge_id} · {draft.request}
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
                      <Digest label="manifest" value={draft.manifest_sha256} />
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
                        {draft.private_assets.length} uploaded
                      </div>
                      <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                        {draft.manifest.private_assets.length} declared
                      </div>
                    </td>
                    <td className="text-[var(--text-muted)]">
                      {formatDate(draft.updated_at, locale)}
                    </td>
                    <td>
                      <div className="flex flex-wrap gap-2">
                        <ActionButton
                          label="Validate"
                          icon={<RefreshCw className="w-3 h-3" />}
                          disabled={busy || !csrfToken}
                          onClick={() =>
                            void runDraftAction(draft.id, "validate")
                          }
                        />
                        <ActionButton
                          label="Approve"
                          icon={<CheckCircle2 className="w-3 h-3" />}
                          disabled={busy || !csrfToken}
                          onClick={() =>
                            void runDraftAction(draft.id, "approve")
                          }
                        />
                        <ActionButton
                          label="Publish"
                          icon={<Send className="w-3 h-3" />}
                          disabled={busy || !csrfToken}
                          onClick={() =>
                            void runDraftAction(draft.id, "publish")
                          }
                        />
                        <ActionButton
                          label="Reject"
                          icon={<XCircle className="w-3 h-3" />}
                          disabled={busy || !csrfToken}
                          onClick={() =>
                            void runDraftAction(draft.id, "reject")
                          }
                          danger
                        />
                        <ActionButton
                          label="Abandon"
                          icon={<RotateCcw className="w-3 h-3" />}
                          disabled={busy || !csrfToken}
                          onClick={() =>
                            void runDraftAction(draft.id, "abandon")
                          }
                        />
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

function SectionTitle({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <h2 className="flex items-center gap-2 text-[var(--text-h3)] font-semibold">
      <span className="text-[var(--accent-secondary-text)]">{icon}</span>
      {title}
    </h2>
  );
}

function TextInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
        {label}
      </span>
      <input
        className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
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
      className={`btn btn-sm ${danger ? "btn-ghost text-[var(--status-error)]" : "btn-secondary"}`}
      onClick={() => void onClick()}
      disabled={disabled}
    >
      {icon}
      {label}
    </button>
  );
}

function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const className =
    normalized === "published" ||
    normalized === "approved" ||
    normalized === "validated"
      ? "badge-success"
      : normalized === "rejected"
        ? "badge-error"
        : normalized === "draft"
          ? "badge-warning"
          : "badge-default";

  return <span className={`badge ${className}`}>{status}</span>;
}

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

function adminErrorMessage(error: unknown): string {
  if (error instanceof AdminApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unknown draft review error.";
}
