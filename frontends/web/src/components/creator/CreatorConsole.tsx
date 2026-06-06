"use client";

import { Copy, KeyRound, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";
import { type FormEvent, useEffect, useState } from "react";
import { ExpirationDateTimeField } from "@/components/ExpirationDateTimeField";
import {
  CreatorApiError,
  createCreatorApiToken,
  revokeCreatorApiToken,
} from "@/lib/creatorApi";
import {
  mutateCreatorApiTokens,
  useCreatorApiTokens,
  useCreatorSession,
} from "@/lib/creatorData";
import { utcDateTimeLocalToRfc3339 } from "@/lib/dateTime";
import type { CreatorApiTokenListResponse } from "@/lib/schemas";
import { normalizeTokenLabelForDuplicateCheck } from "@/lib/tokenLabels";

type PendingAction = "createToken" | "revokeToken";
type CreatorApiTokenItem = CreatorApiTokenListResponse["items"][number];

/** Renders the reduced creator console for identity and API-token management. */
export function CreatorConsole() {
  const t = useTranslations("creator");
  const locale = useLocale();
  const [label, setLabel] = useState("");
  const [expiresAt, setExpiresAt] = useState("");
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(
    null,
  );
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const creatorSession = useCreatorSession();
  const creator = creatorSession.session ?? null;
  const hasCreatorAccess =
    creator?.status === "active" &&
    (creator.roles.includes("creator") || creator.roles.includes("admin"));
  const csrfToken = hasCreatorAccess ? (creator?.csrf_token ?? "") : "";
  const creatorHumanId = creator?.human_id;
  const tokenResource = useCreatorApiTokens(hasCreatorAccess, creatorHumanId);

  useEffect(() => {
    const accessScope = `${creatorHumanId ?? "anonymous"}:${hasCreatorAccess ? "creator" : "blocked"}`;
    if (accessScope) {
      setCreatedToken(null);
    }
  }, [creatorHumanId, hasCreatorAccess]);

  const submitToken = async (event: FormEvent) => {
    event.preventDefault();
    if (!hasCreatorAccess || !csrfToken) {
      setError(creatorAccessMessage(creator, creatorAccessMessages(t)));
      return;
    }
    const trimmedLabel = label.trim();
    if (
      tokenResource.tokens?.items.some(
        (token) =>
          token.status === "active" &&
          normalizeTokenLabelForDuplicateCheck(token.label) ===
            normalizeTokenLabelForDuplicateCheck(trimmedLabel),
      )
    ) {
      setError(t("apiTokens.duplicateLabel"));
      return;
    }
    setPendingAction("createToken");
    setError(null);
    setCreatedToken(null);
    const expiresAtRfc3339 = utcDateTimeLocalToRfc3339(expiresAt);
    if (expiresAtRfc3339 === null) {
      setPendingAction(null);
      setError(t("apiTokens.expiresInvalid"));
      return;
    }
    try {
      const response = await createCreatorApiToken(
        {
          label: trimmedLabel,
          ...(expiresAtRfc3339 ? { expires_at: expiresAtRfc3339 } : {}),
        },
        csrfToken,
      );
      setCreatedToken(response.token);
      setLabel("");
      setExpiresAt("");
      await mutateCreatorApiTokens(creatorHumanId);
      setMessage(t("apiTokens.created"));
    } catch (caught) {
      setError(creatorErrorMessage(caught, t("messages.unknown")));
    } finally {
      setPendingAction(null);
    }
  };

  const revokeToken = async (token: CreatorApiTokenItem) => {
    if (!csrfToken) {
      setError(creatorAccessMessage(creator, creatorAccessMessages(t)));
      return;
    }
    setPendingAction("revokeToken");
    setError(null);
    try {
      await revokeCreatorApiToken(token.id, csrfToken);
      await mutateCreatorApiTokens(creatorHumanId);
      setMessage(t("apiTokens.revoked"));
    } catch (caught) {
      setError(creatorErrorMessage(caught, t("messages.unknown")));
    } finally {
      setPendingAction(null);
    }
  };

  const copyCreatedToken = async () => {
    if (!createdToken) {
      return;
    }
    try {
      await navigator.clipboard.writeText(createdToken);
      setCreatedToken(null);
      setMessage(t("apiTokens.copied"));
    } catch {
      setError(t("apiTokens.copyFailed"));
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <section className="card-elevated">
        <span className="badge badge-validation mb-4">
          <KeyRound className="w-3 h-3" />
          {t("hero.badge")}
        </span>
        <h1
          className="text-h1 font-bold leading-h1"
          style={{ fontFamily: "var(--font-sans)" }}
        >
          {t("hero.title")}
        </h1>
        <p className="mt-3 max-w-2xl text-body leading-body text-fg-secondary">
          {t("hero.description")}
        </p>
      </section>

      {error ? (
        <div className="card border-danger/40 text-danger">{error}</div>
      ) : null}
      {message ? (
        <div className="card border-success/30 text-success">{message}</div>
      ) : null}

      <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
        <form className="card space-y-4" onSubmit={submitToken}>
          <div className="flex items-center gap-2">
            <KeyRound className="w-4 h-4 text-action-fg" />
            <h2 className="text-h3 font-semibold">{t("apiTokens.create")}</h2>
          </div>
          <label className="form-field">
            <span>{t("apiTokens.label")}</span>
            <input
              value={label}
              onChange={(event) => setLabel(event.target.value)}
              placeholder={t("apiTokens.labelPlaceholder")}
              disabled={!hasCreatorAccess}
              required
            />
          </label>
          <ExpirationDateTimeField
            label={t("apiTokens.expiresAt")}
            value={expiresAt}
            onChange={setExpiresAt}
            disabled={!hasCreatorAccess}
          />
          <button
            type="submit"
            className="btn btn-primary w-full"
            disabled={!hasCreatorAccess || pendingAction === "createToken"}
          >
            <Plus className="w-4 h-4" />
            {t("apiTokens.createButton")}
          </button>
          {createdToken && hasCreatorAccess ? (
            <div className="rounded-md border border-success/30 p-3 bg-success/5">
              <div className="flex items-center justify-between gap-3">
                <div className="text-caption uppercase tracking-wide text-fg-muted">
                  {t("apiTokens.createdToken")}
                </div>
                <button
                  type="button"
                  className="text-caption text-fg-muted hover:text-fg"
                  onClick={() => setCreatedToken(null)}
                >
                  {t("apiTokens.dismiss")}
                </button>
              </div>
              <div className="mt-2 flex items-center gap-2">
                <code className="min-w-0 flex-1 break-all text-body-sm">
                  {createdToken}
                </code>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => void copyCreatedToken()}
                  aria-label={t("apiTokens.copy")}
                >
                  <Copy className="w-4 h-4" />
                </button>
              </div>
            </div>
          ) : null}
        </form>

        <div className="card overflow-x-auto">
          <div className="flex items-center justify-between gap-4 mb-4">
            <div className="flex items-center gap-2">
              <KeyRound className="w-4 h-4 text-action-fg" />
              <h2 className="text-h3 font-semibold">{t("apiTokens.title")}</h2>
            </div>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void tokenResource.mutate()}
              disabled={!hasCreatorAccess || tokenResource.isLoading}
            >
              <RefreshCw className="w-4 h-4" />
              {t("apiTokens.refresh")}
            </button>
          </div>
          {!hasCreatorAccess ? (
            <div className="empty-state">
              {creatorAccessMessage(creator, creatorAccessMessages(t))}
            </div>
          ) : tokenResource.tokens?.items.length ? (
            <table className="data-table">
              <thead>
                <tr>
                  <th>{t("apiTokens.label")}</th>
                  <th>{t("apiTokens.status")}</th>
                  <th>{t("apiTokens.lastUsed")}</th>
                  <th>{t("apiTokens.expiresAt")}</th>
                  <th>{t("apiTokens.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {tokenResource.tokens.items.map((token) => (
                  <tr key={token.id}>
                    <td>
                      <div className="font-medium">{token.label}</div>
                      <div className="font-mono text-caption text-fg-muted">
                        {token.id}
                      </div>
                    </td>
                    <td>{creatorTokenStatusLabel(token.status, t)}</td>
                    <td>{formatOptionalDate(token.last_used_at, locale)}</td>
                    <td>{formatOptionalDate(token.expires_at, locale)}</td>
                    <td>
                      <button
                        type="button"
                        className="btn btn-secondary"
                        onClick={() => void revokeToken(token)}
                        disabled={
                          token.status !== "active" ||
                          pendingAction === "revokeToken"
                        }
                      >
                        <Trash2 className="w-4 h-4" />
                        {t("apiTokens.revoke")}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <div className="empty-state">{t("apiTokens.empty")}</div>
          )}
        </div>
      </section>
    </div>
  );
}

function creatorTokenStatusLabel(
  status: string,
  t: ReturnType<typeof useTranslations>,
): string {
  if (status === "active") {
    return t("apiTokens.statusActive");
  }
  if (status === "revoked") {
    return t("apiTokens.statusRevoked");
  }
  return status;
}

function formatOptionalDate(
  value: string | null | undefined,
  locale: string,
): string {
  if (!value) {
    return "—";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

/** Normalizes unknown errors into a displayable message. */
function creatorErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof CreatorApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback;
}

type CreatorAccessMessages = {
  signIn: string;
  finishSetup: string;
  accessDenied: string;
};

function creatorAccessMessages(
  t: ReturnType<typeof useTranslations>,
): CreatorAccessMessages {
  return {
    signIn: t("messages.signInBeforeContinue"),
    finishSetup: t("messages.finishSetupBeforeCreator"),
    accessDenied: t("messages.accessDenied"),
  };
}

function creatorAccessMessage(
  creator: { status: string } | null,
  messages: CreatorAccessMessages,
): string {
  if (!creator) {
    return messages.signIn;
  }
  if (creator.status === "setup_required") {
    return messages.finishSetup;
  }
  return messages.accessDenied;
}
