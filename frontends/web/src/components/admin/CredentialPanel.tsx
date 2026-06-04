"use client";

import { LogOut } from "lucide-react";
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
    <div className="card min-w-full px-4 py-3 lg:min-w-[280px] lg:max-w-[340px]">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        {sessionLogin ? (
          <>
            <span className="text-caption text-fg-muted">
              {t(hasAdminRole ? "signedInAs" : "signedInNoAdmin", {
                username: sessionLogin,
              })}
            </span>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={() => void onLogout()}
              disabled={loading}
            >
              <LogOut className="w-3.5 h-3.5" />
              {t("signOut")}
            </button>
          </>
        ) : (
          <>
            <span className="text-caption text-fg-muted">{t("title")}</span>
            <button
              type="button"
              className="btn btn-primary btn-sm"
              aria-label={t("signIn")}
              onClick={() => void onLogin()}
              disabled={loading}
            >
              <span>{loading ? t("loading") : t("signInCompact")}</span>
              <span
                className="github-repo-mark opacity-100"
                aria-hidden="true"
              />
            </button>
          </>
        )}
      </div>
    </div>
  );
}
