"use client";

import { GitPullRequest, LoaderCircle } from "lucide-react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
import { useEffect, useRef, useState } from "react";
import { CreatorApiError, completeGithubLogin } from "@/lib/creatorApi";

/** Renders the creator oauth callback component. */
export function CreatorOAuthCallback() {
  const t = useTranslations("creator.oauth");
  const searchParams = useSearchParams();
  const started = useRef(false);
  const [status, setStatus] = useState<"loading" | "success" | "error">(
    "loading",
  );
  const [message, setMessage] = useState(t("fallback"));

  useEffect(() => {
    if (started.current) {
      return;
    }
    started.current = true;
    const code = searchParams.get("code");
    const state = searchParams.get("state");
    window.history.replaceState(null, "", window.location.pathname);
    if (!code || !state) {
      setStatus("error");
      setMessage(t("missingCodeState"));
      return;
    }

    void completeGithubLogin(code, state)
      .then((session) => {
        setStatus("success");
        setMessage(t("signedIn", { login: session.github_login }));
      })
      .catch((error) => {
        setStatus("error");
        setMessage(oauthErrorMessage(error, t("failed")));
      });
  }, [searchParams, t]);

  return (
    <section className="card-elevated max-w-2xl mx-auto">
      <span className="badge badge-validation mb-4">
        <GitPullRequest className="w-3 h-3" />
        {t("badge")}
      </span>
      <h1
        className="text-h1 font-bold leading-h1"
        style={{ fontFamily: "var(--font-sans)" }}
      >
        {t("title")}
      </h1>
      <p className="mt-3 text-body text-fg-secondary">{message}</p>
      <div className="mt-6 flex items-center gap-3">
        {status === "loading" ? (
          <LoaderCircle className="w-5 h-5 animate-spin text-action-fg" />
        ) : null}
        {status !== "loading" ? (
          <Link href="/creator" className="btn btn-primary">
            {t("return")}
          </Link>
        ) : null}
      </div>
    </section>
  );
}

/** Normalizes unknown errors into a displayable message. */
function oauthErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof CreatorApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback;
}
