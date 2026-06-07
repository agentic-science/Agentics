import { GeistMono } from "geist/font/mono";
import { GeistSans } from "geist/font/sans";
import type { Metadata } from "next";
import Script from "next/script";
import { getLocale, getTranslations } from "next-intl/server";
import { CookieConsent } from "@/components/CookieConsent";
import { Providers } from "@/components/Providers";
import { loadAgenticsWebEnv } from "@/lib/env";
import enMessages from "../../messages/en.json";
import zhMessages from "../../messages/zh.json";
import "./globals.css";

const allMessages = {
  en: enMessages as Record<string, unknown>,
  zh: zhMessages as Record<string, unknown>,
};

/** Handles generate metadata behavior for this module. */
export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations("meta");
  return {
    title: t("title"),
    description: t("site_description"),
    openGraph: {
      title: t("title"),
      description: t("site_description"),
      type: "website",
    },
  };
}

/** Renders the root layout component. */
export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const locale = await getLocale();
  const messages = allMessages[locale as "en" | "zh"] ?? allMessages.en;
  const { gaMeasurementId } = loadAgenticsWebEnv();

  return (
    <html
      lang={locale}
      className={`${GeistSans.variable} ${GeistMono.variable}`}
      suppressHydrationWarning
    >
      <head>
        <Script src="/theme-init.js" strategy="beforeInteractive" />
      </head>
      <body>
        <Providers locale={locale} messages={messages}>
          {children}
          <CookieConsent gaMeasurementId={gaMeasurementId} />
        </Providers>
      </body>
    </html>
  );
}
