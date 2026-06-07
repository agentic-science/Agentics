import { NextIntlClientProvider } from "next-intl";
import { afterEach, describe, expect, it, vi } from "vitest";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { SiteChrome } from "./SiteChrome";

vi.mock("next-intl/server", () => ({
  getTranslations: async () => (key: string) =>
    key.split(".").reduce((value: unknown, part) => {
      if (typeof value === "object" && value !== null && part in value) {
        return (value as Record<string, unknown>)[part];
      }
      return key;
    }, messages as unknown) as string,
}));

vi.mock("@/components/AccountMenu", () => ({
  AccountMenu: () => <span data-testid="account-menu" />,
}));

vi.mock("@/components/LanguageSwitcher", () => ({
  LanguageSwitcher: () => <span data-testid="language-switcher" />,
}));

vi.mock("@/components/ThemeSwitcher", () => ({
  ThemeSwitcher: () => <span data-testid="theme-switcher" />,
}));

ensureDomEnvironment();
const { cleanup, render } = await import("@testing-library/react");

describe("SiteChrome", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders public legal footer links", async () => {
    const tree = await SiteChrome({ children: <div>content</div> });
    const view = render(
      <NextIntlClientProvider locale="en" messages={messages} timeZone="UTC">
        {tree}
      </NextIntlClientProvider>,
    );

    expect(
      view.getByRole("link", { name: "Privacy" }).getAttribute("href"),
    ).toBe("/privacy");
    expect(
      view.getByRole("link", { name: "Cookies" }).getAttribute("href"),
    ).toBe("/cookies");
    expect(view.getByRole("button", { name: "Cookie settings" })).toBeTruthy();
  });
});
