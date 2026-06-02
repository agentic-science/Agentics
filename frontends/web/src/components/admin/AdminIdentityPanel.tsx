"use client";

import { KeyRound, ShieldCheck, UserCog } from "lucide-react";
import { useTranslations } from "next-intl";
import { type FormEvent, useState } from "react";
import { adminErrorMessage } from "@/components/admin/errors";
import { StatusBadge } from "@/components/admin/StatusBadge";
import {
  createAdminServiceToken,
  grantHumanAdminRole,
  revokeAdminServiceToken,
  revokeHumanAdminRole,
} from "@/lib/adminApi";
import { formatDate } from "@/lib/format";
import type {
  AdminHumanListResponse,
  AdminServiceTokenListResponse,
} from "@/lib/schemas";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

export function AdminIdentityPanel({
  csrfToken,
  currentHumanId,
  humans,
  serviceTokens,
  locale,
  onRefresh,
  onSessionChanged,
  onError,
  onMessage,
}: {
  csrfToken: string;
  currentHumanId: string | null;
  humans: AdminHumanListResponse;
  serviceTokens: AdminServiceTokenListResponse;
  locale: string;
  onRefresh: AdminRefresh;
  onSessionChanged: () => Promise<void>;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}) {
  const t = useTranslations("admin.identity");
  const common = useTranslations("common");
  const [label, setLabel] = useState("");
  const [expiresAt, setExpiresAt] = useState("");
  const [createdToken, setCreatedToken] = useState<string | null>(null);

  const submitToken = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      onError(t("signIn"));
      return;
    }
    try {
      const response = await createAdminServiceToken(
        {
          label: label.trim(),
          ...(expiresAt.trim() ? { expires_at: expiresAt.trim() } : {}),
        },
        csrfToken,
      );
      setCreatedToken(response.token);
      setLabel("");
      setExpiresAt("");
      onMessage(t("tokenCreated", { label: response.token_record.label }));
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

  const setAdminRole = async (humanId: string, grant: boolean) => {
    if (!csrfToken) {
      onError(t("signIn"));
      return;
    }
    try {
      if (grant) {
        await grantHumanAdminRole(humanId, csrfToken);
      } else {
        await revokeHumanAdminRole(humanId, csrfToken);
      }
      onMessage(t(grant ? "adminGranted" : "adminRevoked"));
      await onRefresh({ quiet: true });
      await onSessionChanged();
    } catch (e) {
      onError(
        adminErrorMessage(e, {
          accessDenied: t("accessDenied"),
          unknown: t("unknown"),
        }),
      );
    }
  };

  const revokeToken = async (tokenId: string) => {
    if (!csrfToken) {
      onError(t("signIn"));
      return;
    }
    if (!window.confirm(t("revokeTokenConfirm"))) {
      return;
    }
    try {
      await revokeAdminServiceToken(tokenId, csrfToken);
      onMessage(t("tokenRevoked"));
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
  const activeAdminCount = humans.items.filter(
    (human) => human.status === "active" && human.roles.includes("admin"),
  ).length;

  return (
    <section className="grid grid-cols-1 xl:grid-cols-2 gap-6">
      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <div className="flex items-center gap-2">
            <UserCog className="w-4 h-4 text-action-fg" />
            <h2 className="text-h3 font-semibold">{t("humansTitle")}</h2>
          </div>
          <span className="badge badge-default">
            {common("rows", { count: humans.items.length })}
          </span>
        </div>
        {humans.items.length === 0 ? (
          <div className="empty-state">{t("humansEmpty")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("human")}</th>
                <th>{t("roles")}</th>
                <th>{t("status")}</th>
                <th>{t("created")}</th>
                <th>{t("actions")}</th>
              </tr>
            </thead>
            <tbody>
              {humans.items.map((human) => {
                const isAdmin = human.roles.includes("admin");
                const revokeDisabled =
                  isAdmin &&
                  (human.human_id === currentHumanId || activeAdminCount <= 1);
                return (
                  <tr key={human.human_id}>
                    <td>
                      <div className="font-medium">@{human.github_login}</div>
                      <div className="font-mono text-caption text-fg-muted">
                        {human.human_id}
                      </div>
                    </td>
                    <td className="font-mono">{human.roles.join(", ")}</td>
                    <td>
                      <StatusBadge status={human.status} />
                    </td>
                    <td>{formatDate(human.created_at, locale)}</td>
                    <td>
                      <button
                        type="button"
                        className={
                          isAdmin ? "btn btn-secondary" : "btn btn-primary"
                        }
                        disabled={revokeDisabled}
                        onClick={() =>
                          void setAdminRole(human.human_id, !isAdmin)
                        }
                      >
                        <ShieldCheck className="w-4 h-4" />
                        {isAdmin ? t("revokeAdmin") : t("grantAdmin")}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <div className="card">
        <div className="flex items-center gap-2 mb-4">
          <KeyRound className="w-4 h-4 text-action-fg" />
          <h2 className="text-h3 font-semibold">{t("tokensTitle")}</h2>
        </div>
        <form className="grid grid-cols-1 gap-3 mb-5" onSubmit={submitToken}>
          <label className="flex flex-col gap-1">
            <span className="text-caption uppercase tracking-wide text-fg-muted">
              {t("label")}
            </span>
            <input
              className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
              value={label}
              onChange={(event) => setLabel(event.target.value)}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-caption uppercase tracking-wide text-fg-muted">
              {t("expiresAt")}
            </span>
            <input
              className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
              value={expiresAt}
              placeholder="2026-06-01T00:00:00Z"
              onChange={(event) => setExpiresAt(event.target.value)}
            />
          </label>
          <button type="submit" className="btn btn-primary justify-self-start">
            {t("createToken")}
          </button>
        </form>
        {createdToken ? (
          <div className="mb-5 rounded-control border border-warning/40 bg-warning/10 p-3">
            <div className="text-caption uppercase tracking-wide text-fg-muted">
              {t("createdToken")}
            </div>
            <div className="mt-1 font-mono text-body-sm break-all">
              {createdToken}
            </div>
          </div>
        ) : null}
        {serviceTokens.items.length === 0 ? (
          <div className="empty-state">{t("tokensEmpty")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("token")}</th>
                <th>{t("status")}</th>
                <th>{t("lastUsed")}</th>
                <th>{t("actions")}</th>
              </tr>
            </thead>
            <tbody>
              {serviceTokens.items.map((token) => (
                <tr key={token.id}>
                  <td>
                    <div className="font-medium">{token.label}</div>
                    <div className="font-mono text-caption text-fg-muted">
                      {token.id}
                    </div>
                  </td>
                  <td>
                    <StatusBadge status={token.status} />
                  </td>
                  <td>
                    {token.last_used_at
                      ? formatDate(token.last_used_at, locale)
                      : common("none")}
                  </td>
                  <td>
                    {token.status === "active" ? (
                      <button
                        type="button"
                        className="btn btn-secondary"
                        onClick={() => void revokeToken(token.id)}
                      >
                        {t("revokeToken")}
                      </button>
                    ) : (
                      common("none")
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}
