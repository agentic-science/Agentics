import { cookies, headers } from "next/headers";
import { getRequestConfig } from "next-intl/server";

const locales = ["en", "zh"] as const;
/** Describes the locale shape used by this module. */
type Locale = (typeof locales)[number];
const defaultLocale: Locale = "en";

/** Resolves locale from request context. */
export function resolveLocale(
  acceptLang: string | null,
  cookieLocale: string | null,
): Locale {
  // Cookie takes precedence
  if (cookieLocale && locales.includes(cookieLocale as Locale)) {
    return cookieLocale as Locale;
  }

  if (acceptLang) {
    for (const preferred of parseAcceptLanguage(acceptLang)) {
      for (const locale of locales) {
        if (
          preferred.tag === locale ||
          preferred.tag.startsWith(`${locale}-`)
        ) {
          return locale;
        }
      }
    }
  }

  return defaultLocale;
}

interface LanguagePreference {
  tag: string;
  q: number;
  index: number;
}

function parseAcceptLanguage(value: string): LanguagePreference[] {
  return value
    .split(",")
    .map((entry, index): LanguagePreference | null => {
      const [rawTag, ...rawParams] = entry.trim().split(";");
      const tag = rawTag.trim().toLowerCase();
      if (!tag || tag === "*") {
        return null;
      }
      let q = 1;
      for (const rawParam of rawParams) {
        const [rawName, rawValue] = rawParam.split("=");
        if (rawName?.trim().toLowerCase() !== "q") {
          continue;
        }
        const parsed = Number.parseFloat(rawValue?.trim() ?? "");
        q = Number.isFinite(parsed) && parsed >= 0 && parsed <= 1 ? parsed : 0;
      }
      return { tag, q, index };
    })
    .filter(
      (entry): entry is LanguagePreference => entry !== null && entry.q > 0,
    )
    .sort((a, b) => {
      if (a.q !== b.q) {
        return b.q - a.q;
      }
      return a.index - b.index;
    });
}

export default getRequestConfig(async () => {
  const cookieStore = await cookies();
  const headersList = await headers();

  const locale = resolveLocale(
    headersList.get("accept-language"),
    cookieStore.get("agentics-locale")?.value ?? null,
  );

  return {
    locale,
    messages: (await import(`../../messages/${locale}.json`)).default,
    timeZone: "UTC",
  };
});
