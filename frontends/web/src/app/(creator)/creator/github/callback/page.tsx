import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { CreatorGithubSignInCallback } from "@/components/creator/CreatorGithubSignInCallback";

/** Renders the creator GitHub sign-in callback page component. */
export default async function CreatorGithubSignInCallbackPage() {
  const t = await getTranslations("creator.githubSignIn");

  return (
    <Suspense
      fallback={
        <section className="card-elevated max-w-2xl mx-auto">
          {t("fallback")}
        </section>
      }
    >
      <CreatorGithubSignInCallback />
    </Suspense>
  );
}
