import { NextIntlClientProvider } from "next-intl";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import messages from "../../messages/en.json";
import { ensureDomEnvironment } from "../test/dom";
import { COOKIE_SETTINGS_EVENT, CookieConsent } from "./CookieConsent";
import { CookieSettingsButton } from "./CookieSettingsButton";

ensureDomEnvironment();
const { cleanup, fireEvent, render, waitFor } = await import(
  "@testing-library/react"
);

describe("CookieConsent", () => {
  beforeEach(() => {
    clearKnownCookies();
    window.__agenticsCookieSettingsRequestedAt = undefined;
  });

  afterEach(() => {
    cleanup();
    clearKnownCookies();
    window.__agenticsCookieSettingsRequestedAt = undefined;
  });

  it("does not render a banner or GA script when analytics is unconfigured", async () => {
    const view = renderCookieConsent();

    await waitFor(() => {
      expect(view.queryByText("Analytics cookies")).toBeNull();
    });
    expect(
      document.querySelector("script[src*='googletagmanager']"),
    ).toBeNull();
  });

  it("loads Google Analytics only after opt-in", async () => {
    const view = renderCookieConsent("G-ABC123XYZ");

    expect(await view.findByText("Analytics cookies")).toBeTruthy();
    expect(
      document.querySelector("script[src*='googletagmanager']"),
    ).toBeNull();

    fireEvent.click(view.getByRole("button", { name: "Accept analytics" }));

    await waitFor(() => {
      expect(
        document.querySelector("script[src*='googletagmanager']"),
      ).toBeTruthy();
    });
    expect(document.cookie).toContain("agentics_cookie_consent=accepted");
  });

  it("rejects analytics and deletes visible GA cookies after withdrawal", async () => {
    setTestCookie("agentics_cookie_consent=accepted; Path=/");
    setTestCookie("_ga=GA1.1.123; Path=/");
    setTestCookie("_ga_ABC123XYZ=GS1.1.456; Path=/");
    const view = renderCookieConsent("G-ABC123XYZ");

    await waitFor(() => {
      expect(
        document.querySelector("script[src*='googletagmanager']"),
      ).toBeTruthy();
    });
    window.dispatchEvent(new window.Event(COOKIE_SETTINGS_EVENT));
    fireEvent.click(
      await view.findByRole("button", { name: "Reject analytics" }),
    );

    await waitFor(() => {
      expect(document.cookie).toContain("agentics_cookie_consent=rejected");
    });
    expect(document.cookie).not.toContain("_ga=");
    expect(document.cookie).not.toContain("_ga_ABC123XYZ=");
    expect(
      (window as unknown as Record<string, unknown>)["ga-disable-G-ABC123XYZ"],
    ).toBe(true);
  });

  it("opens settings when the footer request happens before consent mounts", async () => {
    const footer = renderWithMessages(<CookieSettingsButton />);

    fireEvent.click(footer.getByRole("button", { name: "Cookie settings" }));
    cleanup();

    const view = renderCookieConsent("G-ABC123XYZ");

    expect(
      await view.findByRole("dialog", { name: "Cookie settings" }),
    ).toBeTruthy();
  });

  it("opens settings from a server-rendered footer button marker", async () => {
    const view = renderWithMessages(
      <>
        <button data-cookie-settings-button type="button">
          Cookie settings
        </button>
        <CookieConsent gaMeasurementId="G-ABC123XYZ" />
      </>,
    );

    fireEvent.click(view.getByRole("button", { name: "Cookie settings" }));

    expect(
      await view.findByRole("dialog", { name: "Cookie settings" }),
    ).toBeTruthy();
  });
});

function renderCookieConsent(gaMeasurementId?: string) {
  return renderWithMessages(
    <CookieConsent gaMeasurementId={gaMeasurementId} />,
  );
}

function renderWithMessages(children: ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages} timeZone="UTC">
      {children}
    </NextIntlClientProvider>,
  );
}

function clearKnownCookies() {
  for (const name of ["agentics_cookie_consent", "_ga", "_ga_ABC123XYZ"]) {
    setTestCookie(`${name}=; Path=/; Max-Age=0`);
  }
}

function setTestCookie(cookie: string) {
  // biome-ignore lint/suspicious/noDocumentCookie: tests need to seed browser cookie state
  document.cookie = cookie;
}
