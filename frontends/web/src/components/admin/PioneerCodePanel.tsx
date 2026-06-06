"use client";

import { Check, Copy, KeyRound } from "lucide-react";
import { useTranslations } from "next-intl";
import { type FormEvent, useState } from "react";
import { adminErrorMessage } from "@/components/admin/errors";
import { StatusBadge } from "@/components/admin/StatusBadge";
import { ExpirationDateTimeField } from "@/components/ExpirationDateTimeField";
import { adminFetchJson } from "@/lib/adminApi";
import { utcDateTimeLocalToRfc3339 } from "@/lib/dateTime";
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
  const t = useTranslations("admin.pioneer");
  const common = useTranslations("common");
  const [form, setForm] = useState({
    label: "",
    note: "",
    maxUses: "1",
    expiresAt: "",
  });
  const [detail, setDetail] = useState<PioneerCodeDetailResponse | null>(null);
  const [copiedCode, setCopiedCode] = useState<string | null>(null);

  /** Copies one generated pioneer code to the operator clipboard. */
  const copyCode = async (code: string) => {
    try {
      await writeClipboardText(code);
      setCopiedCode(code);
      window.setTimeout(() => {
        setCopiedCode((current) => (current === code ? null : current));
      }, 1500);
    } catch {
      onError(t("copyFailed"));
    }
  };

  /** Creates a pioneer code using the current form state. */
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      onError(t("signInCreate"));
      return;
    }

    if (!/^-?\d+$/.test(form.maxUses.trim())) {
      onError(t("maxUsesInteger"));
      return;
    }
    const maxUses = Number.parseInt(form.maxUses, 10);
    if (maxUses === 0 || maxUses < -1) {
      onError(t("maxUsesRange"));
      return;
    }
    const label = form.label.trim();
    if (label && !PIONEER_LABEL_PATTERN.test(label)) {
      onError(t("labelInvalid"));
      return;
    }
    const expiresAt = form.expiresAt.trim();
    const expiresAtRfc3339 = utcDateTimeLocalToRfc3339(expiresAt);
    if (expiresAtRfc3339 === null) {
      onError(t("expiresInvalid"));
      return;
    }

    const parsedRequest = createPioneerCodeRequestSchema.safeParse({
      max_uses: maxUses,
      ...(label ? { label } : {}),
      ...(form.note ? { note: form.note } : {}),
      ...(expiresAtRfc3339 ? { expires_at: expiresAtRfc3339 } : {}),
    });
    if (!parsedRequest.success) {
      onError(parsedRequest.error.issues[0]?.message ?? t("invalidRequest"));
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
      setForm({ label: "", note: "", maxUses: "1", expiresAt: "" });
      onMessage(t("createdMessage", { code: created.code.code_display }));
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(
        adminErrorMessage(e, {
          accessDenied: t("accessDenied"),
          unknown: t("unknown"),
        }),
      );
    }
  };

  /** Loads usage detail for one pioneer code. */
  const inspect = async (id: string) => {
    if (!csrfToken) {
      onError(t("signInInspect"));
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
      onError(
        adminErrorMessage(e, {
          accessDenied: t("accessDenied"),
          unknown: t("unknown"),
        }),
      );
    }
  };

  /** Revokes one pioneer code and rescinds account setup created through it. */
  const revoke = async (id: string) => {
    if (!csrfToken) {
      onError(t("signInRevoke"));
      return;
    }
    const code = pioneerCodes.items.find((item) => item.id === id);
    const affectedUses = code?.use_count ?? detail?.uses.length ?? 0;
    const display = code?.code_display ?? detail?.code.code_display ?? id;
    if (
      !window.confirm(
        t("revokeConfirm", { code: display, count: affectedUses }),
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
        t("revokedMessage", {
          humans: response.revoked_human_count,
          sessions: response.revoked_human_session_count,
          adminTokens: response.revoked_admin_service_token_count,
          creatorTokens: response.revoked_creator_api_token_count,
          agents: response.revoked_agent_count,
          tokens: response.revoked_token_count,
        }),
      );
      await onRefresh({ quiet: true });
      await inspect(id);
    } catch (e) {
      onError(
        adminErrorMessage(e, {
          accessDenied: t("accessDenied"),
          unknown: t("unknown"),
        }),
      );
    }
  };

  return (
    <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
      <form className="card flex flex-col gap-4" onSubmit={submit}>
        <div className="flex items-center gap-2">
          <KeyRound className="w-4 h-4 text-action-fg" />
          <h2 className="text-h3 font-semibold">{t("createTitle")}</h2>
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-caption uppercase tracking-wide text-fg-muted">
            {t("label")}
          </span>
          <input
            className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
            value={form.label}
            maxLength={6}
            placeholder="jack"
            onChange={(event) =>
              setForm((current) => ({ ...current, label: event.target.value }))
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-caption uppercase tracking-wide text-fg-muted">
            {t("maxUses")}
          </span>
          <input
            className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
            value={form.maxUses}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                maxUses: event.target.value,
              }))
            }
          />
        </label>
        <ExpirationDateTimeField
          label={t("expiresAt")}
          value={form.expiresAt}
          onChange={(expiresAt) =>
            setForm((current) => ({
              ...current,
              expiresAt,
            }))
          }
        />
        <label className="flex flex-col gap-1">
          <span className="text-caption uppercase tracking-wide text-fg-muted">
            {t("note")}
          </span>
          <textarea
            className="min-h-24 rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
            value={form.note}
            onChange={(event) =>
              setForm((current) => ({ ...current, note: event.target.value }))
            }
          />
        </label>
        <button type="submit" className="btn btn-primary" disabled={!csrfToken}>
          <KeyRound className="w-4 h-4" />
          {t("create")}
        </button>
      </form>

      <div className="flex flex-col gap-5">
        <div className="card overflow-x-auto">
          <div className="flex items-center justify-between gap-4 mb-4">
            <h2 className="text-h3 font-semibold">{t("title")}</h2>
            <span className="badge badge-default">
              {common("rows", { count: pioneerCodes.items.length })}
            </span>
          </div>
          {pioneerCodes.items.length === 0 ? (
            <div className="empty-state">{t("empty")}</div>
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>{t("code")}</th>
                  <th>{t("uses")}</th>
                  <th>{t("status")}</th>
                  <th>{t("note")}</th>
                  <th>{t("actions")}</th>
                </tr>
              </thead>
              <tbody>
                {pioneerCodes.items.map((code) => (
                  <tr key={code.id}>
                    <td>
                      <div className="flex items-center gap-2">
                        <span className="font-mono">{code.code_display}</span>
                        <CopyPioneerCodeButton
                          code={code.code_display}
                          copied={copiedCode === code.code_display}
                          copyLabel={t("copyCode", {
                            code: code.code_display,
                          })}
                          copiedLabel={t("copiedCode", {
                            code: code.code_display,
                          })}
                          onCopy={copyCode}
                        />
                      </div>
                    </td>
                    <td>
                      {code.use_count}/
                      {code.max_uses === -1
                        ? common("unlimited")
                        : code.max_uses}
                    </td>
                    <td>
                      <LocalizedStatusBadge status={code.status} />
                    </td>
                    <td>{code.note || "—"}</td>
                    <td>
                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          className="btn btn-secondary btn-sm"
                          onClick={() => void inspect(code.id)}
                        >
                          {t("inspect")}
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm text-danger"
                          disabled={code.status === "revoked"}
                          onClick={() => void revoke(code.id)}
                        >
                          {t("revoke")}
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
              <div className="flex items-center gap-2">
                <h2 className="text-h3 font-semibold">
                  {detail.code.code_display}
                </h2>
                <CopyPioneerCodeButton
                  code={detail.code.code_display}
                  copied={copiedCode === detail.code.code_display}
                  copyLabel={t("copyCode", {
                    code: detail.code.code_display,
                  })}
                  copiedLabel={t("copiedCode", {
                    code: detail.code.code_display,
                  })}
                  onCopy={copyCode}
                />
              </div>
              <span className="badge badge-default">
                {t("createdSubjects", { count: detail.uses.length })}
              </span>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 mb-5 text-body-sm">
              <div>
                <span className="text-fg-muted">{t("note")}</span>
                <div>{detail.code.note || "—"}</div>
              </div>
              <div>
                <span className="text-fg-muted">{t("created")}</span>
                <div>{formatDate(detail.code.created_at)}</div>
              </div>
            </div>
            {detail.uses.length === 0 ? (
              <div className="empty-state">{t("noUses")}</div>
            ) : (
              <table className="data-table">
                <thead>
                  <tr>
                    <th>{t("subject")}</th>
                    <th>{t("kind")}</th>
                    <th>{t("used")}</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.uses.map((usage) => {
                    const subjectId =
                      usage.subject_kind === "human"
                        ? usage.human_id
                        : usage.agent_id;
                    const subjectName =
                      usage.subject_kind === "human"
                        ? (usage.human_github_login ?? usage.human_id)
                        : (usage.agent_display_name ?? usage.agent_id);
                    return (
                      <tr
                        key={`${usage.subject_kind}-${subjectId ?? usage.used_at}-${usage.used_at}`}
                      >
                        <td>
                          <div>{subjectName ?? "—"}</div>
                          <div className="font-mono text-caption text-fg-muted">
                            {subjectId ?? "—"}
                          </div>
                        </td>
                        <td>
                          <div>{usage.registration_kind}</div>
                          <div className="text-caption text-fg-muted">
                            {usage.subject_kind === "human"
                              ? t("human")
                              : t("agent")}
                          </div>
                        </td>
                        <td>{formatDate(usage.used_at)}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </div>
        ) : null}
      </div>
    </section>
  );
}

function CopyPioneerCodeButton({
  code,
  copied,
  copyLabel,
  copiedLabel,
  onCopy,
}: {
  code: string;
  copied: boolean;
  copyLabel: string;
  copiedLabel: string;
  onCopy: (code: string) => Promise<void>;
}) {
  return (
    <button
      type="button"
      className="btn btn-ghost btn-sm px-2"
      aria-label={copied ? copiedLabel : copyLabel}
      title={copied ? copiedLabel : copyLabel}
      onClick={() => void onCopy(code)}
    >
      {copied ? (
        <Check className="w-3.5 h-3.5" aria-hidden="true" />
      ) : (
        <Copy className="w-3.5 h-3.5" aria-hidden="true" />
      )}
    </button>
  );
}

async function writeClipboardText(text: string) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Fall back for local HTTP pages or browsers that block Clipboard API writes.
    }
  }

  const textArea = document.createElement("textarea");
  textArea.value = text;
  textArea.setAttribute("readonly", "true");
  textArea.style.position = "fixed";
  textArea.style.opacity = "0";
  document.body.append(textArea);
  textArea.select();
  const copied = document.execCommand("copy");
  textArea.remove();
  if (!copied) {
    throw new Error("clipboard copy failed");
  }
}

/** Renders a localized status badge for known pioneer-code statuses. */
function LocalizedStatusBadge({ status }: { status: string }) {
  const t = useTranslations("common.statuses");
  const labels: Record<string, string> = {
    active: t("active"),
    revoked: t("revoked"),
  };
  return <StatusBadge status={status}>{labels[status] ?? status}</StatusBadge>;
}
