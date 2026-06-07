import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ensureDomEnvironment } from "../test/dom";
import {
  accountAppearanceStorageKey,
  applyLanguagePreference,
  applyThemeMode,
  GLOBAL_THEME_STORAGE_KEY,
  loadAccountAppearancePreferences,
  saveAccountAppearancePreferences,
} from "./appearancePreferences";

ensureDomEnvironment();

const humanId = "11111111-1111-4111-8111-111111111111";

describe("appearancePreferences", () => {
  beforeEach(() => {
    window.localStorage.clear();
    // biome-ignore lint/suspicious/noDocumentCookie: test resets the locale preference cookie contract
    document.cookie = "agentics-locale=; path=/; max-age=0";
    Object.defineProperty(window, "matchMedia", {
      value: vi.fn(() => ({
        matches: true,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
      configurable: true,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("uses a hashed account storage key", async () => {
    const key = await accountAppearanceStorageKey(humanId);

    expect(key).toMatch(/^agentics-account-appearance:v1:[0-9a-f]{64}$/);
    expect(key).not.toContain(humanId);
  });

  it("defaults missing or invalid account preferences", async () => {
    expect(await loadAccountAppearancePreferences(humanId)).toEqual({
      language: "auto",
      mode: "system",
    });

    const key = await accountAppearanceStorageKey(humanId);
    window.localStorage.setItem(key, "{not valid json");

    expect(await loadAccountAppearancePreferences(humanId)).toEqual({
      language: "auto",
      mode: "system",
    });
  });

  it("saves only account appearance preferences under the hashed key", async () => {
    await saveAccountAppearancePreferences(humanId, {
      language: "zh",
      mode: "light",
    });

    const keys = Array.from(
      { length: window.localStorage.length },
      (_, index) => window.localStorage.key(index),
    );
    expect(keys).toHaveLength(1);
    expect(keys[0]).not.toContain(humanId);
    expect(keys[0]).not.toContain("123");
    expect(
      JSON.parse(window.localStorage.getItem(keys[0] ?? "") ?? "{}"),
    ).toEqual({
      language: "zh",
      mode: "light",
    });
  });

  it("applies theme mode through the existing global theme key", () => {
    const resolved = applyThemeMode("system");

    expect(resolved).toBe("dark");
    expect(window.localStorage.getItem(GLOBAL_THEME_STORAGE_KEY)).toBe(
      "system",
    );
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("sets and clears locale cookies for language preferences", () => {
    const reload = vi.fn();

    expect(applyLanguagePreference("zh", "en", reload)).toBe(true);
    expect(document.cookie).toContain("agentics-locale=zh");
    expect(reload).toHaveBeenCalledTimes(1);

    reload.mockClear();
    expect(applyLanguagePreference("auto", "zh", reload)).toBe(true);
    expect(document.cookie).not.toContain("agentics-locale=");
    expect(reload).toHaveBeenCalledTimes(1);
  });
});
