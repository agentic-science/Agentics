"use client";

import { GitPullRequest, KeyRound, LogOut } from "lucide-react";
import { useTranslations } from "next-intl";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

/** Renders the credential panel component. */
export function CredentialPanel({
  sessionLogin,
  hasAdminRole,
  onLogin,
  onLogout,
  loading,
}: {
  sessionLogin: string | null;
  hasAdminRole: boolean;
  onLogin: AdminRefresh;
  onLogout: () => Promise<void>;
  loading: boolean;
}) {
  const t = useTranslations("admin.auth");

  return (
    <div className="card min-w-full lg:min-w-[360px] lg:max-w-[420px]">
      <div className="flex items-center gap-2 mb-4">
        <KeyRound className="w-4 h-4 text-action-fg" />
        <h2 className="text-h3 font-semibold">{t("title")}</h2>
      </div>
      <div className="mt-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <span className="text-caption text-fg-muted">
          {sessionLogin
            ? t(hasAdminRole ? "signedInAs" : "signedInNoAdmin", {
                username: sessionLogin,
              })
            : t("cookieNote")}
        </span>
        <div className="flex gap-2">
          {sessionLogin ? (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void onLogout()}
              disabled={loading}
            >
              <LogOut className="w-4 h-4" />
              {t("signOut")}
            </button>
          ) : null}
          {sessionLogin ? null : (
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void onLogin()}
              disabled={loading}
            >
              <GitPullRequest className="w-4 h-4" />
              {loading ? t("loading") : t("signIn")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
