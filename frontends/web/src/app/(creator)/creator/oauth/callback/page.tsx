import { Suspense } from "react";
import { CreatorOAuthCallback } from "@/components/creator/CreatorOAuthCallback";

/** Renders the creator oauth callback page component. */
export default function CreatorOAuthCallbackPage() {
  return (
    <Suspense
      fallback={
        <section className="card-elevated max-w-2xl mx-auto">
          Completing GitHub sign-in.
        </section>
      }
    >
      <CreatorOAuthCallback />
    </Suspense>
  );
}
