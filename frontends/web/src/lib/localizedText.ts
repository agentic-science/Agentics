/** Text with required English and Chinese variants from backend contracts. */
export type LocalizedText = {
  en: string;
  zh: string;
};

/** Selects localized text with an English fallback for unsupported locales. */
export function selectLocalizedText(
  value: LocalizedText,
  locale: string,
): string {
  if (locale.startsWith("zh")) {
    return value.zh || value.en;
  }
  return value.en || value.zh;
}
