export type LanguagePreference = "auto" | "en" | "zh";
export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export interface AppearancePreferences {
  language: LanguagePreference;
  mode: ThemeMode;
}

export const DEFAULT_APPEARANCE_PREFERENCES: AppearancePreferences = {
  language: "auto",
  mode: "system",
};

export const GLOBAL_THEME_STORAGE_KEY = "agentics-theme";
export const THEME_MODE_CHANGED_EVENT = "agentics-theme-mode-changed";

const ACCOUNT_APPEARANCE_STORAGE_PREFIX = "agentics-account-appearance:v1:";
const LOCALE_COOKIE_NAME = "agentics-locale";
const LOCALE_COOKIE_MAX_AGE_SECONDS = 31_536_000;

/** Builds the account-scoped appearance preference storage key. */
export async function accountAppearanceStorageKey(
  humanId: string,
): Promise<string> {
  return `${ACCOUNT_APPEARANCE_STORAGE_PREFIX}${await sha256Hex(humanId)}`;
}

/** Reads account preferences, defaulting when storage is absent or invalid. */
export async function loadAccountAppearancePreferences(
  humanId: string,
): Promise<AppearancePreferences> {
  return (
    (await readStoredAccountAppearancePreferences(humanId)) ?? {
      ...DEFAULT_APPEARANCE_PREFERENCES,
    }
  );
}

/** Reads account preferences only when a valid stored object exists. */
export async function readStoredAccountAppearancePreferences(
  humanId: string,
): Promise<AppearancePreferences | null> {
  const key = await accountAppearanceStorageKey(humanId);
  const raw = window.localStorage.getItem(key);
  if (!raw) {
    return null;
  }

  try {
    return parseAppearancePreferences(JSON.parse(raw));
  } catch {
    return null;
  }
}

/** Saves account preferences under the account-scoped hashed storage key. */
export async function saveAccountAppearancePreferences(
  humanId: string,
  preferences: AppearancePreferences,
): Promise<AppearancePreferences> {
  const normalized = parseAppearancePreferences(preferences);
  const key = await accountAppearanceStorageKey(humanId);
  window.localStorage.setItem(key, JSON.stringify(normalized));
  return normalized;
}

/** Merges and persists one preference patch. */
export async function updateAccountAppearancePreferences(
  humanId: string,
  patch: Partial<AppearancePreferences>,
): Promise<AppearancePreferences> {
  const current = await loadAccountAppearancePreferences(humanId);
  return saveAccountAppearancePreferences(humanId, { ...current, ...patch });
}

/** Reads the global theme mode used by early page initialization. */
export function readGlobalThemeMode(): ThemeMode {
  return parseThemeMode(window.localStorage.getItem(GLOBAL_THEME_STORAGE_KEY));
}

/** Resolves the currently visible theme for a theme mode. */
export function resolveThemeMode(mode: ThemeMode): ResolvedTheme {
  if (mode === "system") {
    return prefersDarkColorScheme() ? "dark" : "light";
  }
  return mode;
}

/** Applies theme mode to the existing global theme storage and document state. */
export function applyThemeMode(mode: ThemeMode): ResolvedTheme {
  const normalized = parseThemeMode(mode);
  const resolved = resolveThemeMode(normalized);
  window.localStorage.setItem(GLOBAL_THEME_STORAGE_KEY, normalized);
  document.documentElement.dataset.theme = resolved;
  dispatchThemeModeChanged(normalized, resolved);
  return resolved;
}

/** Applies language preference through the existing locale cookie contract. */
export function applyLanguagePreference(
  preference: LanguagePreference,
  currentLocale: string,
  reload: () => void = () => window.location.reload(),
): boolean {
  const normalized = parseLanguagePreference(preference);
  if (normalized === "auto") {
    const hadLocaleCookie = readCookie(LOCALE_COOKIE_NAME) !== null;
    clearLocaleCookie();
    if (hadLocaleCookie) {
      reload();
      return true;
    }
    return false;
  }

  setLocaleCookie(normalized);
  if (normalizeLocale(currentLocale) !== normalized) {
    reload();
    return true;
  }
  return false;
}

/** Subscribes to theme changes made through this module. */
export function onThemeModeChanged(
  listener: (mode: ThemeMode, resolved: ResolvedTheme) => void,
): () => void {
  const handler = (event: Event) => {
    if (
      event instanceof window.CustomEvent &&
      isThemeModeDetail(event.detail)
    ) {
      listener(event.detail.mode, event.detail.resolved);
      return;
    }
    const mode = readGlobalThemeMode();
    listener(mode, resolveThemeMode(mode));
  };
  window.addEventListener(THEME_MODE_CHANGED_EVENT, handler);
  return () => window.removeEventListener(THEME_MODE_CHANGED_EVENT, handler);
}

function parseAppearancePreferences(value: unknown): AppearancePreferences {
  if (typeof value !== "object" || value === null) {
    return { ...DEFAULT_APPEARANCE_PREFERENCES };
  }
  const record = value as Record<string, unknown>;
  return {
    language: parseLanguagePreference(record.language),
    mode: parseThemeMode(record.mode),
  };
}

function parseLanguagePreference(value: unknown): LanguagePreference {
  return value === "en" || value === "zh" || value === "auto"
    ? value
    : DEFAULT_APPEARANCE_PREFERENCES.language;
}

function parseThemeMode(value: unknown): ThemeMode {
  return value === "dark" || value === "light" || value === "system"
    ? value
    : DEFAULT_APPEARANCE_PREFERENCES.mode;
}

function prefersDarkColorScheme(): boolean {
  return (
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
}

function normalizeLocale(locale: string): "en" | "zh" {
  return locale.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function setLocaleCookie(locale: "en" | "zh") {
  // biome-ignore lint/suspicious/noDocumentCookie: intentional locale preference cookie
  document.cookie = `${LOCALE_COOKIE_NAME}=${locale}; path=/; max-age=${LOCALE_COOKIE_MAX_AGE_SECONDS}; SameSite=Lax`;
}

function clearLocaleCookie() {
  // biome-ignore lint/suspicious/noDocumentCookie: intentional locale preference cookie
  document.cookie = `${LOCALE_COOKIE_NAME}=; path=/; max-age=0; SameSite=Lax`;
}

function readCookie(name: string): string | null {
  const prefix = `${name}=`;
  for (const part of document.cookie.split(";")) {
    const trimmed = part.trim();
    if (trimmed.startsWith(prefix)) {
      return trimmed.slice(prefix.length);
    }
  }
  return null;
}

function dispatchThemeModeChanged(mode: ThemeMode, resolved: ResolvedTheme) {
  window.dispatchEvent(
    new window.CustomEvent(THEME_MODE_CHANGED_EVENT, {
      detail: { mode, resolved },
    }),
  );
}

function isThemeModeDetail(
  value: unknown,
): value is { mode: ThemeMode; resolved: ResolvedTheme } {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const detail = value as Record<string, unknown>;
  return (
    (detail.mode === "system" ||
      detail.mode === "light" ||
      detail.mode === "dark") &&
    (detail.resolved === "light" || detail.resolved === "dark")
  );
}

async function sha256Hex(value: string): Promise<string> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    throw new Error("Web Crypto SHA-256 is required for appearance storage");
  }
  const digest = await subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}
