import { GeistMono } from "geist/font/mono";
import { GeistSans } from "geist/font/sans";
import type { Metadata } from "next";
import Script from "next/script";
import { getLocale, getTranslations } from "next-intl/server";
import { Providers } from "@/components/Providers";
import enMessages from "../../messages/en.json";
import zhMessages from "../../messages/zh.json";
import "./globals.css";

const allMessages = {
  en: enMessages as Record<string, unknown>,
  zh: zhMessages as Record<string, unknown>,
};

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

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const locale = await getLocale();
  const messages = allMessages[locale as "en" | "zh"] ?? allMessages.en;

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
        </Providers>
      </body>
    </html>
  );
}
