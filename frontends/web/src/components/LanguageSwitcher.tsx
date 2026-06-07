"use client";

import { Globe } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";
import {
  applyLanguagePreference,
  type LanguagePreference,
  updateAccountAppearancePreferences,
} from "@/lib/appearancePreferences";
import { useHumanSession } from "@/lib/humanSession";

const locales = [
  { code: "en", label: "EN" },
  { code: "zh", label: "中" },
] as const;

/** Renders the language switcher component. */
export function LanguageSwitcher() {
  const locale = useLocale();
  const t = useTranslations("language");
  const { data: session } = useHumanSession();

  /** Navigates to the current route under the selected locale. */
  const switchLocale = async (next: LanguagePreference) => {
    if (session?.human_id) {
      await updateAccountAppearancePreferences(session.human_id, {
        language: next,
      }).catch(() => {
        // Header preferences are best-effort local browser state.
      });
    }
    applyLanguagePreference(next, locale);
  };

  return (
    <div className="flex items-center gap-2">
      <Globe className="w-3.5 h-3.5 text-fg-muted" />
      {locales.map((loc) => (
        <button
          key={loc.code}
          type="button"
          onClick={() => void switchLocale(loc.code)}
          className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
            locale === loc.code
              ? "text-action-fg"
              : "text-fg-muted hover:text-fg-secondary"
          }`}
          aria-label={t("switchTo", { locale: loc.label })}
        >
          {loc.label}
        </button>
      ))}
    </div>
  );
}
