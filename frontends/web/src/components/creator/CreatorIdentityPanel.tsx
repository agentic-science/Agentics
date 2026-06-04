"use client";

import { KeyRound, RefreshCw } from "lucide-react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import type { HumanSessionResponse } from "@/lib/schemas";

/** Renders the creator identity panel component. */
export function CreatorIdentityPanel({
  creator,
  loading,
  onRefresh,
}: {
  creator: HumanSessionResponse | null;
  loading: boolean;
  onRefresh: () => Promise<void>;
}) {
  const t = useTranslations("creator.identity");
  const hasCreatorAccess =
    creator?.status === "active" &&
    (creator.roles.includes("creator") || creator.roles.includes("admin"));

  return (
    <div className="card min-w-full lg:min-w-[360px] lg:max-w-[420px]">
      <div className="flex items-center gap-2 mb-4">
        <KeyRound className="w-4 h-4 text-action-fg" />
        <h2 className="text-h3 font-semibold">{t("title")}</h2>
      </div>
      {creator ? (
        <div className="space-y-3">
          <div>
            <div className="text-caption uppercase tracking-wide text-fg-muted">
              {t("githubAccount")}
            </div>
            <div className="font-mono text-body-sm">
              {creator.github_login} · {creator.github_user_id}
            </div>
          </div>
          <div>
            <div className="text-caption uppercase tracking-wide text-fg-muted">
              {t("status")}
            </div>
            <div className="font-mono text-body-sm">{creator.status}</div>
          </div>
          <div>
            <div className="text-caption uppercase tracking-wide text-fg-muted">
              {t("humanId")}
            </div>
            <div className="font-mono text-caption text-fg-muted break-all">
              {creator.human_id}
            </div>
          </div>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => void onRefresh()}
            disabled={loading}
          >
            <RefreshCw className="w-4 h-4" />
            {t("refresh")}
          </button>
          {!hasCreatorAccess && creator.status === "setup_required" ? (
            <Link
              className="btn btn-primary"
              href="/account/setup?return_to=/creator"
            >
              {t("finishSetup")}
            </Link>
          ) : null}
          {!hasCreatorAccess && creator.status !== "setup_required" ? (
            <p className="text-body-sm text-warning">{t("accessDenied")}</p>
          ) : null}
        </div>
      ) : (
        <div className="space-y-4">
          <p className="text-body-sm text-fg-secondary">
            {t("githubSignInRequired")}
          </p>
          <Link className="btn btn-primary" href="/sign-in?return_to=/creator">
            {t("signIn")}
          </Link>
        </div>
      )}
    </div>
  );
}
