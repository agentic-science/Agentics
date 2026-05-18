"use client";

import { Globe } from "lucide-react";
import { useLocale } from "next-intl";

const locales = [
  { code: "en", label: "EN" },
  { code: "zh", label: "中" },
] as const;

/** Renders the language switcher component. */
export function LanguageSwitcher() {
  const locale = useLocale();

  /** Navigates to the current route under the selected locale. */
  const switchLocale = (next: string) => {
    if (next === locale) return;
    // biome-ignore lint/suspicious/noDocumentCookie: intentional cookie for locale persistence
    document.cookie = `agentics-locale=${next}; path=/; max-age=31536000`;
    window.location.reload();
  };

  return (
    <div className="flex items-center gap-2">
      <Globe className="w-3.5 h-3.5 text-[var(--text-muted)]" />
      {locales.map((loc) => (
        <button
          key={loc.code}
          type="button"
          onClick={() => switchLocale(loc.code)}
          className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
            locale === loc.code
              ? "text-[var(--accent-primary-text)]"
              : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
          }`}
          aria-label={`Switch to ${loc.code}`}
        >
          {loc.label}
        </button>
      ))}
    </div>
  );
}
