"use client";

import { GitPullRequest, LoaderCircle } from "lucide-react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useEffect, useState } from "react";
import {
  CreatorApiError,
  completeGithubLogin,
  storeCreatorCsrfToken,
} from "@/lib/creatorApi";

export function CreatorOAuthCallback() {
  const searchParams = useSearchParams();
  const [status, setStatus] = useState<"loading" | "success" | "error">(
    "loading",
  );
  const [message, setMessage] = useState("Completing GitHub sign-in.");

  useEffect(() => {
    const code = searchParams.get("code");
    const state = searchParams.get("state");
    if (!code || !state) {
      setStatus("error");
      setMessage("GitHub did not return the required OAuth code and state.");
      return;
    }

    void completeGithubLogin(code, state)
      .then((session) => {
        storeCreatorCsrfToken(session.csrf_token);
        setStatus("success");
        setMessage(`Signed in as ${session.github_login}.`);
      })
      .catch((error) => {
        setStatus("error");
        setMessage(oauthErrorMessage(error));
      });
  }, [searchParams]);

  return (
    <section className="card-elevated max-w-2xl mx-auto">
      <span className="badge badge-validation mb-4">
        <GitPullRequest className="w-3 h-3" />
        Creator OAuth
      </span>
      <h1
        className="text-[var(--text-h1)] font-bold leading-[var(--leading-h1)]"
        style={{ fontFamily: "var(--font-serif)" }}
      >
        GitHub sign-in
      </h1>
      <p className="mt-3 text-[var(--text-body)] text-[var(--text-secondary)]">
        {message}
      </p>
      <div className="mt-6 flex items-center gap-3">
        {status === "loading" ? (
          <LoaderCircle className="w-5 h-5 animate-spin text-[var(--accent-primary-text)]" />
        ) : null}
        {status !== "loading" ? (
          <Link href="/creator" className="btn btn-primary">
            Return to creator console
          </Link>
        ) : null}
      </div>
    </section>
  );
}

function oauthErrorMessage(error: unknown): string {
  if (error instanceof CreatorApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "GitHub OAuth failed.";
}
