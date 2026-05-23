import { getTranslations } from "next-intl/server";
import { Suspense } from "react";
import { CreatorOAuthCallback } from "@/components/creator/CreatorOAuthCallback";

/** Renders the creator oauth callback page component. */
export default async function CreatorOAuthCallbackPage() {
  const t = await getTranslations("creator.oauth");

  return (
    <Suspense
      fallback={
        <section className="card-elevated max-w-2xl mx-auto">
          {t("fallback")}
        </section>
      }
    >
      <CreatorOAuthCallback />
    </Suspense>
  );
}
