"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
import { useState } from "react";
import { startGithubLogin } from "@/lib/authApi";
import { useHumanSession } from "@/lib/humanSession";

/** Renders the unified human sign-in page panel. */
export function SignInPanel() {
  const t = useTranslations("auth.signIn");
  const searchParams = useSearchParams();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { data: session } = useHumanSession();
  const returnTo = normalizedReturnTo(searchParams.get("return_to"));
  const active = session?.status === "active";
  const admin = active && session.roles.includes("admin");
  const creator =
    active &&
    (session.roles.includes("creator") || session.roles.includes("admin"));

  const start = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await startGithubLogin(returnTo);
      window.location.href = response.authorization_url;
    } catch (e) {
      setError(e instanceof Error ? e.message : t("failed"));
      setLoading(false);
    }
  };

  return (
    <section className="card-elevated max-w-2xl mx-auto">
      <h1
        className="text-h1 font-bold leading-h1"
        style={{ fontFamily: "var(--font-sans)" }}
      >
        {t("title")}
      </h1>
      <p className="mt-3 text-body text-fg-secondary">
        {session ? (
          t("signedIn", { login: session.github_login })
        ) : (
          <>
            {t("bodyBeforeEmail")}
            <a
              className="text-action-fg underline underline-offset-2 transition-colors hover:text-fg"
              href="mailto:agentics@reify.ing"
            >
              {t("email")}
            </a>
            {t("bodyAfterEmail")}
          </>
        )}
      </p>
      {error ? <p className="mt-4 text-body-sm text-danger">{error}</p> : null}
      {!session ? (
        <p className="mt-4 text-body-sm text-fg-muted">{t("githubNotice")}</p>
      ) : null}
      <div className="mt-6 flex flex-wrap items-center gap-3">
        {!session ? (
          <>
            <button
              aria-label={t("buttonAria")}
              className="btn btn-primary"
              disabled={loading}
              onClick={() => void start()}
              type="button"
            >
              {loading ? t("redirecting") : t("button")}
              <span aria-hidden="true" className="github-button-mark" />
            </button>
            <Link className="btn btn-secondary" href={returnTo}>
              {t("cancel")}
            </Link>
          </>
        ) : null}
        {session?.status === "setup_required" ? (
          <Link
            className="btn btn-primary"
            href={`/account/setup?return_to=${encodeURIComponent(returnTo)}`}
          >
            {t("finishSetup")}
          </Link>
        ) : null}
        {creator ? (
          <Link className="btn btn-secondary" href="/creator">
            {t("creatorConsole")}
          </Link>
        ) : null}
        {admin ? (
          <Link className="btn btn-secondary" href="/admin">
            {t("adminPanel")}
          </Link>
        ) : null}
      </div>
    </section>
  );
}

function normalizedReturnTo(value: string | null): string {
  if (value === null) {
    return "/";
  }
  if (!value.startsWith("/") || value.startsWith("//")) {
    return "/";
  }
  return value;
}
