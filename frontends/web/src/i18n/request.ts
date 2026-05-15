import { cookies, headers } from "next/headers";
import { getRequestConfig } from "next-intl/server";

const locales = ["en", "zh"] as const;
/** Describes the locale shape used by this module. */
type Locale = (typeof locales)[number];
const defaultLocale: Locale = "en";

/** Resolves locale from request context. */
function resolveLocale(
  acceptLang: string | null,
  cookieLocale: string | null,
): Locale {
  // Cookie takes precedence
  if (cookieLocale && locales.includes(cookieLocale as Locale)) {
    return cookieLocale as Locale;
  }

  // Fall back to Accept-Language header
  if (acceptLang) {
    const preferred = acceptLang.split(",")[0].trim().toLowerCase();
    for (const locale of locales) {
      if (preferred.startsWith(locale)) {
        return locale;
      }
    }
  }

  return defaultLocale;
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
