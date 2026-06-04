import { Suspense } from "react";
import { AccountSetupPanel } from "@/components/AccountSetupPanel";

/** Renders the human account setup page. */
export default function AccountSetupPage() {
  return (
    <Suspense>
      <AccountSetupPanel />
    </Suspense>
  );
}
