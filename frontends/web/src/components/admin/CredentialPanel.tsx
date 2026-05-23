"use client";

import { KeyRound, RefreshCw } from "lucide-react";
import { useTranslations } from "next-intl";
import type { AdminCredentials } from "@/lib/adminApi";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

/** Renders the credential panel component. */
export function CredentialPanel({
  credentials,
  sessionUsername,
  onChange,
  onLogin,
  onLogout,
  loading,
}: {
  credentials: AdminCredentials;
  sessionUsername: string | null;
  onChange: (credentials: AdminCredentials) => void;
  onLogin: AdminRefresh;
  onLogout: () => Promise<void>;
  loading: boolean;
}) {
  const t = useTranslations("admin.auth");

  return (
    <div className="card min-w-full lg:min-w-[360px] lg:max-w-[420px]">
      <div className="flex items-center gap-2 mb-4">
        <KeyRound className="w-4 h-4 text-[var(--accent-primary-text)]" />
        <h2 className="text-[var(--text-h3)] font-semibold">{t("title")}</h2>
      </div>
      <div className="grid grid-cols-1 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            {t("username")}
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={credentials.username}
            onChange={(event) =>
              onChange({ ...credentials, username: event.target.value })
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            {t("password")}
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            type="password"
            value={credentials.password}
            onChange={(event) =>
              onChange({ ...credentials, password: event.target.value })
            }
          />
        </label>
      </div>
      <div className="mt-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
          {sessionUsername
            ? t("signedInAs", { username: sessionUsername })
            : t("cookieNote")}
        </span>
        <div className="flex gap-2">
          {sessionUsername ? (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void onLogout()}
              disabled={loading}
            >
              {t("signOut")}
            </button>
          ) : null}
          {sessionUsername ? null : (
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void onLogin()}
              disabled={loading}
            >
              <RefreshCw className="w-4 h-4" />
              {loading ? t("loading") : t("signIn")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
