"use client";

import { KeyRound } from "lucide-react";
import { type FormEvent, useState } from "react";
import { StatusBadge } from "@/components/admin/StatusBadge";
import { AdminApiError, adminFetchJson } from "@/lib/adminApi";
import { formatDate } from "@/lib/format";
import {
  type CreatePioneerCodeRequest,
  createPioneerCodeRequestSchema,
  type PioneerCodeDetailResponse,
  type PioneerCodeListResponse,
  pioneerCodeDetailResponseSchema,
  revokePioneerCodeResponseSchema,
} from "@/lib/schemas";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;
const PIONEER_LABEL_PATTERN = /^[a-z0-9_]{1,6}$/;
const PIONEER_CODE_PATTERN = /^([a-z0-9_]{1,6}-)?[0-9a-f]{8}$/;
const RFC3339_PATTERN =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;

export function PioneerCodePanel({
  csrfToken,
  pioneerCodes,
  onRefresh,
  onError,
  onMessage,
}: {
  csrfToken: string;
  pioneerCodes: PioneerCodeListResponse;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}) {
  const [form, setForm] = useState({
    label: "",
    code: "",
    note: "",
    maxUses: "1",
    expiresAt: "",
  });
  const [detail, setDetail] = useState<PioneerCodeDetailResponse | null>(null);

  /** Creates a pioneer code using the current form state. */
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      onError("Sign in before creating pioneer codes.");
      return;
    }

    if (!/^-?\d+$/.test(form.maxUses.trim())) {
      onError("max_uses must be an integer.");
      return;
    }
    const maxUses = Number.parseInt(form.maxUses, 10);
    if (maxUses === 0 || maxUses < -1) {
      onError("max_uses must be -1 or a positive integer.");
      return;
    }
    const label = form.label.trim();
    if (label && !PIONEER_LABEL_PATTERN.test(label)) {
      onError("label must be at most 6 lowercase letters, digits, or _.");
      return;
    }
    const code = form.code.trim();
    if (code && !PIONEER_CODE_PATTERN.test(code)) {
      onError(
        "code must be 8 lowercase hex chars or <label>-<8 lowercase hex chars>.",
      );
      return;
    }
    const expiresAt = form.expiresAt.trim();
    if (
      expiresAt &&
      (!RFC3339_PATTERN.test(expiresAt) || Number.isNaN(Date.parse(expiresAt)))
    ) {
      onError("expires_at must be an RFC3339 timestamp.");
      return;
    }

    const parsedRequest = createPioneerCodeRequestSchema.safeParse({
      max_uses: maxUses,
      ...(label ? { label } : {}),
      ...(code ? { code } : {}),
      ...(form.note ? { note: form.note } : {}),
      ...(expiresAt ? { expires_at: expiresAt } : {}),
    });
    if (!parsedRequest.success) {
      onError(
        parsedRequest.error.issues[0]?.message ??
          "Invalid pioneer code request.",
      );
      return;
    }
    const request: CreatePioneerCodeRequest = parsedRequest.data;

    try {
      const created = await adminFetchJson(
        "/admin/pioneer-codes",
        pioneerCodeDetailResponseSchema,
        csrfToken,
        { method: "POST", body: JSON.stringify(request) },
      );
      setDetail(created);
      setForm({ label: "", code: "", note: "", maxUses: "1", expiresAt: "" });
      onMessage(`Created pioneer code ${created.code.code_display}.`);
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e));
    }
  };

  /** Loads usage detail for one pioneer code. */
  const inspect = async (id: string) => {
    if (!csrfToken) {
      onError("Sign in before inspecting pioneer codes.");
      return;
    }
    try {
      setDetail(
        await adminFetchJson(
          `/admin/pioneer-codes/${encodeURIComponent(id)}`,
          pioneerCodeDetailResponseSchema,
          csrfToken,
        ),
      );
    } catch (e) {
      onError(adminErrorMessage(e));
    }
  };

  /** Revokes one pioneer code and disables agents created through it. */
  const revoke = async (id: string) => {
    if (!csrfToken) {
      onError("Sign in before revoking pioneer codes.");
      return;
    }
    const code = pioneerCodes.items.find((item) => item.id === id);
    const affectedUses = code?.use_count ?? detail?.uses.length ?? 0;
    const display = code?.code_display ?? detail?.code.code_display ?? id;
    if (
      !window.confirm(
        `Revoke ${display} and disable ${affectedUses} created agents?`,
      )
    ) {
      return;
    }
    try {
      const response = await adminFetchJson(
        `/admin/pioneer-codes/${encodeURIComponent(id)}/revoke`,
        revokePioneerCodeResponseSchema,
        csrfToken,
        { method: "POST" },
      );
      onMessage(
        `Revoked ${response.revoked_agent_count} agents and ${response.revoked_token_count} tokens.`,
      );
      await onRefresh({ quiet: true });
      await inspect(id);
    } catch (e) {
      onError(adminErrorMessage(e));
    }
  };

  return (
    <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
      <form className="card flex flex-col gap-4" onSubmit={submit}>
        <div className="flex items-center gap-2">
          <KeyRound className="w-4 h-4 text-[var(--accent-primary-text)]" />
          <h2 className="text-[var(--text-h3)] font-semibold">
            Create pioneer code
          </h2>
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Label
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={form.label}
            maxLength={6}
            placeholder="jack"
            onChange={(event) =>
              setForm((current) => ({ ...current, label: event.target.value }))
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Code
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 font-mono text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={form.code}
            placeholder="auto-generate when blank"
            onChange={(event) =>
              setForm((current) => ({ ...current, code: event.target.value }))
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Max uses
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={form.maxUses}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                maxUses: event.target.value,
              }))
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Expires at
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={form.expiresAt}
            placeholder="2026-06-01T00:00:00Z"
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                expiresAt: event.target.value,
              }))
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Note
          </span>
          <textarea
            className="min-h-24 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={form.note}
            onChange={(event) =>
              setForm((current) => ({ ...current, note: event.target.value }))
            }
          />
        </label>
        <button type="submit" className="btn btn-primary" disabled={!csrfToken}>
          <KeyRound className="w-4 h-4" />
          Create code
        </button>
      </form>

      <div className="flex flex-col gap-5">
        <div className="card overflow-x-auto">
          <div className="flex items-center justify-between gap-4 mb-4">
            <h2 className="text-[var(--text-h3)] font-semibold">
              Pioneer codes
            </h2>
            <span className="badge badge-default">
              {pioneerCodes.items.length} rows
            </span>
          </div>
          {pioneerCodes.items.length === 0 ? (
            <div className="empty-state">
              Create or refresh pioneer codes to review beta access.
            </div>
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>Code</th>
                  <th>Uses</th>
                  <th>Status</th>
                  <th>Note</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {pioneerCodes.items.map((code) => (
                  <tr key={code.id}>
                    <td className="font-mono">{code.code_display}</td>
                    <td>
                      {code.use_count}/
                      {code.max_uses === -1 ? "unlimited" : code.max_uses}
                    </td>
                    <td>
                      <StatusBadge status={code.status} />
                    </td>
                    <td>{code.note || "—"}</td>
                    <td>
                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          className="btn btn-secondary btn-sm"
                          onClick={() => void inspect(code.id)}
                        >
                          Inspect
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm text-[var(--status-error)]"
                          disabled={code.status === "revoked"}
                          onClick={() => void revoke(code.id)}
                        >
                          Revoke
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {detail ? (
          <div className="card">
            <div className="flex items-center justify-between gap-4 mb-4">
              <h2 className="text-[var(--text-h3)] font-semibold">
                {detail.code.code_display}
              </h2>
              <span className="badge badge-default">
                {detail.uses.length} created agents
              </span>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 mb-5 text-[var(--text-body-sm)]">
              <div>
                <span className="text-[var(--text-muted)]">Note</span>
                <div>{detail.code.note || "—"}</div>
              </div>
              <div>
                <span className="text-[var(--text-muted)]">Created</span>
                <div>{formatDate(detail.code.created_at)}</div>
              </div>
            </div>
            {detail.uses.length === 0 ? (
              <div className="empty-state">No agents have used this code.</div>
            ) : (
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Agent</th>
                    <th>Kind</th>
                    <th>Used</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.uses.map((usage) => (
                    <tr key={`${usage.agent_id}-${usage.used_at}`}>
                      <td>
                        <div>{usage.agent_display_name}</div>
                        <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                          {usage.agent_id}
                        </div>
                      </td>
                      <td>{usage.registration_kind}</td>
                      <td>{formatDate(usage.used_at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        ) : null}
      </div>
    </section>
  );
}
/** Normalizes unknown pioneer-code admin errors into a displayable message. */
function adminErrorMessage(error: unknown): string {
  if (error instanceof AdminApiError) {
    if (error.status === 401) {
      return "Access denied. Check the admin username and password.";
    }
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unknown admin console error.";
}
