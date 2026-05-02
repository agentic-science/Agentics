"use client";

import { Globe } from "lucide-react";
import { useLocale } from "next-intl";

const locales = [
  { code: "en", label: "EN" },
  { code: "zh", label: "中" },
] as const;

export function LanguageSwitcher() {
  const locale = useLocale();

  const switchLocale = (next: string) => {
    if (next === locale) return;
    // biome-ignore lint/suspicious/noDocumentCookie: intentional cookie for locale persistence
    document.cookie = `agentics-locale=${next}; path=/; max-age=31536000`;
    window.location.reload();
  };

  return (
    <div className="flex items-center gap-0.5">
      <Globe className="w-3.5 h-3.5 text-[var(--text-muted)] mr-1" />
      {locales.map((loc) => (
        <button
          key={loc.code}
          type="button"
          onClick={() => switchLocale(loc.code)}
          className={`px-1.5 py-0.5 rounded text-[11px] font-medium transition-colors ${
            locale === loc.code
              ? "text-[var(--accent-primary-400)]"
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
