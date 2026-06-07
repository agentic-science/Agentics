import { Suspense } from "react";
import { AccountSettingsPanel } from "@/components/AccountSettingsPanel";

/** Renders the human account settings page. */
export default function AccountPage() {
  return (
    <Suspense>
      <AccountSettingsPanel />
    </Suspense>
  );
}
