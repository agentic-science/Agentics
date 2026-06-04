"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
import { type FormEvent, useState } from "react";
import useSWR, { mutate } from "swr";
import {
  completeHumanSetup,
  getHumanSession,
  HUMAN_SESSION_CACHE_KEY,
} from "@/lib/authApi";
import type { HumanSessionResponse } from "@/lib/schemas";

/** Renders the signed-in human setup page. */
export function AccountSetupPanel() {
  const t = useTranslations("auth.setup");
  const searchParams = useSearchParams();
  const returnTo = normalizedReturnTo(searchParams.get("return_to"));
  const [pioneerCode, setPioneerCode] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { data: session, isLoading } = useSWR<HumanSessionResponse>(
    HUMAN_SESSION_CACHE_KEY,
    getHumanSession,
    { shouldRetryOnError: false },
  );

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!session?.csrf_token) {
      setError(t("signInRequired"));
      return;
    }
    setPending(true);
    setError(null);
    try {
      const response = await completeHumanSetup(
        pioneerCode.trim(),
        session.csrf_token,
      );
      await mutate(HUMAN_SESSION_CACHE_KEY, response.session, {
        revalidate: false,
      });
      window.location.href = returnTo;
    } catch (e) {
      setError(e instanceof Error ? e.message : t("failed"));
      setPending(false);
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
      {isLoading ? (
        <p className="mt-3 text-body text-fg-secondary">{t("loading")}</p>
      ) : null}
      {!isLoading && !session ? (
        <div className="mt-5 flex flex-col gap-4">
          <p className="text-body text-fg-secondary">{t("signInRequired")}</p>
          <Link
            className="btn btn-primary w-fit"
            href={`/sign-in?return_to=${encodeURIComponent(
              `/account/setup?return_to=${encodeURIComponent(returnTo)}`,
            )}`}
          >
            {t("signIn")}
          </Link>
        </div>
      ) : null}
      {session?.status === "active" ? (
        <div className="mt-5 flex flex-col gap-4">
          <p className="text-body text-fg-secondary">
            {t("complete", { login: session.github_login })}
          </p>
          <Link className="btn btn-secondary w-fit" href="/creator">
            {t("creatorConsole")}
          </Link>
        </div>
      ) : null}
      {session?.status === "setup_required" ? (
        <form className="mt-5 grid gap-4" onSubmit={submit}>
          <div className="grid gap-3 text-body text-fg-secondary">
            <p>{t("body", { login: session.github_login })}</p>
            <p>
              {t("earlyAccessBeforeEmail")}
              <a
                className="text-action-fg underline underline-offset-2 transition-colors hover:text-fg"
                href="mailto:agentics@reify.ing"
              >
                {t("email")}
              </a>
              {t("earlyAccessAfterEmail")}
            </p>
          </div>
          <label className="flex flex-col gap-1">
            <span className="text-caption uppercase tracking-wide text-fg-muted">
              {t("pioneerCode")}
            </span>
            <input
              autoComplete="off"
              className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
              onChange={(event) => setPioneerCode(event.target.value)}
              value={pioneerCode}
            />
          </label>
          {error ? <p className="text-body-sm text-danger">{error}</p> : null}
          <div className="flex flex-wrap items-center gap-3">
            <button
              className="btn btn-primary"
              disabled={pending}
              type="submit"
            >
              {pending ? t("saving") : t("submit")}
            </button>
            <Link className="btn btn-secondary" href="/">
              {t("skip")}
            </Link>
          </div>
        </form>
      ) : null}
    </section>
  );
}

function normalizedReturnTo(value: string | null): string {
  if (value === null) {
    return "/creator";
  }
  if (!value.startsWith("/") || value.startsWith("//")) {
    return "/creator";
  }
  return value;
}
