import { Suspense } from "react";
import { GithubSignInCallback } from "@/components/GithubSignInCallback";

/** Renders the GitHub sign-in callback page. */
export default function GithubSignInCallbackPage() {
  return (
    <Suspense>
      <GithubSignInCallback />
    </Suspense>
  );
}
