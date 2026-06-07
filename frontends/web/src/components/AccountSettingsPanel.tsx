"use client";

import { Trash2 } from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { type FormEvent, useState } from "react";
import useSWR, { mutate } from "swr";
import {
  deleteHumanAccount,
  getHumanSession,
  HUMAN_SESSION_CACHE_KEY,
} from "@/lib/authApi";
import { clearCreatorApiTokenCaches } from "@/lib/creatorData";
import type { HumanSessionResponse } from "@/lib/schemas";

/** Renders signed-in account identity and deletion controls. */
export function AccountSettingsPanel() {
  const t = useTranslations("accountSettings");
  const statusT = useTranslations("common.statuses");
  const router = useRouter();
  const [confirmation, setConfirmation] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { data: session, isLoading } = useSWR<HumanSessionResponse>(
    HUMAN_SESSION_CACHE_KEY,
    getHumanSession,
    { shouldRetryOnError: false },
  );

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!session?.csrf_token || confirmation !== t("confirmation")) {
      return;
    }
    setPending(true);
    setError(null);
    try {
      await deleteHumanAccount(session.csrf_token);
      await mutate(() => true, undefined, { revalidate: false });
      await clearCreatorApiTokenCaches();
      router.replace("/");
    } catch (e) {
      setError(e instanceof Error ? e.message : t("failed"));
      setPending(false);
    }
  };

  return (
    <section className="account-settings max-w-3xl mx-auto">
      <h1 className="text-h1 font-bold leading-h1">{t("title")}</h1>
      {isLoading ? (
        <p className="mt-3 text-body text-fg-secondary">{t("loading")}</p>
      ) : null}
      {!isLoading && !session ? (
        <div className="mt-5 grid gap-4">
          <p className="text-body text-fg-secondary">{t("signInRequired")}</p>
          <Link
            className="btn btn-primary w-fit"
            href="/sign-in?return_to=%2Faccount"
          >
            {t("signIn")}
          </Link>
        </div>
      ) : null}
      {session ? (
        <div className="mt-6 grid gap-6">
          <section className="card-elevated">
            <h2 className="text-h3 font-semibold">{t("identity")}</h2>
            <dl className="account-summary mt-4">
              <div>
                <dt>{t("githubLogin")}</dt>
                <dd>@{session.github_login}</dd>
              </div>
              <div>
                <dt>{t("githubUserId")}</dt>
                <dd>{session.github_user_id}</dd>
              </div>
              <div>
                <dt>{t("status")}</dt>
                <dd>{statusT(session.status)}</dd>
              </div>
              <div>
                <dt>{t("roles")}</dt>
                <dd>{session.roles.length ? session.roles.join(", ") : "-"}</dd>
              </div>
            </dl>
          </section>

          <form className="card-elevated danger-zone" onSubmit={submit}>
            <div className="flex items-center gap-2">
              <Trash2 className="w-4 h-4 text-danger" />
              <h2 className="text-h3 font-semibold">{t("dangerTitle")}</h2>
            </div>
            <p className="mt-3 text-body-sm text-fg-secondary">
              {t("dangerBody")}
            </p>
            <label className="form-field mt-4">
              <span>{t("confirmLabel")}</span>
              <input
                autoComplete="off"
                onChange={(event) => setConfirmation(event.target.value)}
                value={confirmation}
              />
            </label>
            {error ? <p className="text-body-sm text-danger">{error}</p> : null}
            <button
              className="btn btn-danger mt-4"
              disabled={pending || confirmation !== t("confirmation")}
              type="submit"
            >
              {pending ? t("deleting") : t("deleteButton")}
            </button>
          </form>
        </div>
      ) : null}
    </section>
  );
}
