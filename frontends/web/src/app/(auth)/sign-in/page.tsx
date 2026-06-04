import { Suspense } from "react";
import { SignInPanel } from "@/components/SignInPanel";

/** Renders the unified sign-in page. */
export default function SignInPage() {
  return (
    <Suspense>
      <SignInPanel />
    </Suspense>
  );
}
