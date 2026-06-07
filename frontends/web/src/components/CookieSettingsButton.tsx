"use client";

import { useTranslations } from "next-intl";
import { requestCookieSettings } from "@/components/CookieConsent";

/** Opens the global cookie settings dialog from footer links. */
export function CookieSettingsButton() {
  const t = useTranslations("common");

  return (
    <button
      className="footer-link"
      data-cookie-settings-button
      onClick={requestCookieSettings}
      type="button"
    >
      {t("cookieSettings")}
    </button>
  );
}
