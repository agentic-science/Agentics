import { describe, expect, it } from "vitest";
import { resolveLocale } from "./request";

describe("resolveLocale", () => {
  it("prefers a valid locale cookie over Accept-Language", () => {
    expect(resolveLocale("en-US,zh;q=0.9", "zh")).toBe("zh");
  });

  it("uses the highest-priority supported Accept-Language entry", () => {
    expect(resolveLocale("fr-CA,zh;q=0.9,en;q=0.8", null)).toBe("zh");
  });

  it("matches supported locale region tags", () => {
    expect(resolveLocale("zh-CN,en-US;q=0.8", null)).toBe("zh");
    expect(resolveLocale("en-GB,zh-CN;q=0.8", null)).toBe("en");
  });

  it("ignores malformed q-values and falls back to English", () => {
    expect(resolveLocale("zh;q=oops,fr-CA;q=0.9", null)).toBe("en");
  });

  it("defaults to English without a supported header or cookie", () => {
    expect(resolveLocale(null, null)).toBe("en");
    expect(resolveLocale("fr-CA,ja;q=0.8", "de")).toBe("en");
  });
});
