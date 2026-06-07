"use client";

import Script from "next/script";
import { useTranslations } from "next-intl";
import { useEffect, useRef, useState } from "react";

const CONSENT_COOKIE_NAME = "agentics_cookie_consent";
const CONSENT_MAX_AGE_SECONDS = 180 * 24 * 60 * 60;
export const COOKIE_SETTINGS_EVENT = "agentics:open-cookie-settings";

type ConsentChoice = "accepted" | "rejected";

declare global {
  interface Window {
    dataLayer?: unknown[];
    gtag?: (...args: unknown[]) => void;
    __agenticsCookieSettingsRequestedAt?: number;
  }
}

export function requestCookieSettings() {
  window.__agenticsCookieSettingsRequestedAt = Date.now();
  window.dispatchEvent(new window.Event(COOKIE_SETTINGS_EVENT));
}

/** Renders analytics consent controls and gates Google Analytics loading. */
export function CookieConsent({
  gaMeasurementId,
}: {
  gaMeasurementId?: string;
}) {
  const t = useTranslations("cookieConsent");
  const [choice, setChoice] = useState<ConsentChoice | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const lastHandledSettingsRequest = useRef(0);

  useEffect(() => {
    setLoaded(true);
    setChoice(readConsentCookie());
  }, []);

  useEffect(() => {
    const openSettings = () => {
      lastHandledSettingsRequest.current = readCookieSettingsRequestMarker();
      setSettingsOpen(true);
    };
    const openSettingsFromClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof window.Element)) {
        return;
      }
      if (!target.closest("[data-cookie-settings-button]")) {
        return;
      }
      event.preventDefault();
      window.__agenticsCookieSettingsRequestedAt = Date.now();
      openSettings();
    };
    window.addEventListener(COOKIE_SETTINGS_EVENT, openSettings);
    document.addEventListener("click", openSettingsFromClick);
    if (
      readCookieSettingsRequestMarker() > lastHandledSettingsRequest.current
    ) {
      openSettings();
    }
    return () => {
      window.removeEventListener(COOKIE_SETTINGS_EVENT, openSettings);
      document.removeEventListener("click", openSettingsFromClick);
    };
  }, []);

  useEffect(() => {
    if (!gaMeasurementId) {
      return;
    }
    (window as unknown as Record<string, unknown>)[
      `ga-disable-${gaMeasurementId}`
    ] = choice !== "accepted";
    if (choice === "rejected") {
      deleteGoogleAnalyticsCookies();
    }
  }, [choice, gaMeasurementId]);

  const saveChoice = (nextChoice: ConsentChoice) => {
    setConsentCookie(nextChoice);
    setChoice(nextChoice);
    if (nextChoice === "rejected") {
      deleteGoogleAnalyticsCookies();
    }
    setSettingsOpen(false);
  };

  const analyticsMeasurementId =
    choice === "accepted" ? gaMeasurementId : undefined;
  const showBanner = loaded && Boolean(gaMeasurementId) && choice === null;

  return (
    <>
      {analyticsMeasurementId ? (
        <GoogleAnalytics id={analyticsMeasurementId} />
      ) : null}
      {showBanner ? (
        <section
          className="cookie-banner"
          aria-labelledby="cookie-banner-title"
        >
          <div>
            <h2 id="cookie-banner-title" className="text-body font-semibold">
              {t("title")}
            </h2>
            <p className="mt-1 text-body-sm text-fg-secondary">{t("body")}</p>
          </div>
          <div className="cookie-banner-actions">
            <button
              className="btn btn-primary"
              onClick={() => saveChoice("accepted")}
              type="button"
            >
              {t("accept")}
            </button>
            <button
              className="btn btn-secondary"
              onClick={() => saveChoice("rejected")}
              type="button"
            >
              {t("reject")}
            </button>
            <button
              className="btn btn-ghost"
              onClick={() => setSettingsOpen(true)}
              type="button"
            >
              {t("manage")}
            </button>
          </div>
        </section>
      ) : null}
      {settingsOpen ? (
        <div className="modal-backdrop" role="presentation">
          <section
            aria-labelledby="cookie-settings-title"
            className="modal-panel"
            role="dialog"
            aria-modal="true"
          >
            <h2 id="cookie-settings-title" className="text-h3 font-semibold">
              {t("modalTitle")}
            </h2>
            <p className="mt-3 text-body-sm text-fg-secondary">
              {gaMeasurementId
                ? t("modalBodyConfigured")
                : t("modalBodyUnavailable")}
            </p>
            {gaMeasurementId ? (
              <p className="mt-3 text-caption text-fg-muted">
                {choice === "accepted" ? t("accepted") : t("rejected")}
              </p>
            ) : null}
            <div className="mt-5 flex flex-wrap justify-end gap-3">
              {gaMeasurementId ? (
                <>
                  <button
                    className="btn btn-primary"
                    onClick={() => saveChoice("accepted")}
                    type="button"
                  >
                    {t("accept")}
                  </button>
                  <button
                    className="btn btn-secondary"
                    onClick={() => saveChoice("rejected")}
                    type="button"
                  >
                    {t("reject")}
                  </button>
                </>
              ) : null}
              <button
                className="btn btn-ghost"
                onClick={() => setSettingsOpen(false)}
                type="button"
              >
                {t("close")}
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </>
  );
}

function GoogleAnalytics({ id }: { id: string }) {
  return (
    <>
      <Script
        id="agentics-ga-loader"
        src={`https://www.googletagmanager.com/gtag/js?id=${id}`}
        strategy="afterInteractive"
      />
      <Script id="agentics-ga-init" strategy="afterInteractive">
        {`
          window.dataLayer = window.dataLayer || [];
          function gtag(){window.dataLayer.push(arguments);}
          gtag('js', new Date());
          gtag('config', '${id}');
        `}
      </Script>
    </>
  );
}

function readConsentCookie(): ConsentChoice | null {
  const value = readCookie(CONSENT_COOKIE_NAME);
  return value === "accepted" || value === "rejected" ? value : null;
}

function readCookieSettingsRequestMarker(): number {
  return window.__agenticsCookieSettingsRequestedAt ?? 0;
}

function readCookie(name: string): string | null {
  const prefix = `${name}=`;
  const pair = document.cookie
    .split(";")
    .map((item) => item.trim())
    .find((item) => item.startsWith(prefix));
  return pair ? decodeURIComponent(pair.slice(prefix.length)) : null;
}

function setConsentCookie(choice: ConsentChoice) {
  // biome-ignore lint/suspicious/noDocumentCookie: consent persistence requires setting a first-party cookie
  document.cookie = `${CONSENT_COOKIE_NAME}=${encodeURIComponent(
    choice,
  )}; Path=/; Max-Age=${CONSENT_MAX_AGE_SECONDS}; SameSite=Lax`;
}

function deleteGoogleAnalyticsCookies() {
  const names = document.cookie
    .split(";")
    .map((pair) => pair.trim().split("=")[0])
    .filter((name) => name === "_ga" || name.startsWith("_ga_"));
  for (const name of names) {
    deleteCookieAcrossDomains(name);
  }
}

function deleteCookieAcrossDomains(name: string) {
  const encodedName = encodeURIComponent(name);
  const expires = `${encodedName}=; Path=/; Max-Age=0; SameSite=Lax`;
  // biome-ignore lint/suspicious/noDocumentCookie: withdrawing analytics consent requires deleting GA cookies
  document.cookie = expires;
  for (const domain of candidateCookieDomains(window.location.hostname)) {
    // biome-ignore lint/suspicious/noDocumentCookie: GA may have been set on a parent domain
    document.cookie = `${expires}; Domain=${domain}`;
  }
}

function candidateCookieDomains(hostname: string): string[] {
  if (
    !hostname ||
    hostname === "localhost" ||
    /^\d+\.\d+\.\d+\.\d+$/.test(hostname)
  ) {
    return [];
  }
  const parts = hostname.split(".");
  const domains = new Set<string>();
  for (let index = 0; index <= parts.length - 2; index += 1) {
    const domain = parts.slice(index).join(".");
    domains.add(domain);
    domains.add(`.${domain}`);
  }
  return [...domains];
}
