"use client";

import { NextIntlClientProvider } from "next-intl";
import type { ReactNode } from "react";

/** Describes the providers props shape used by this module. */
interface ProvidersProps {
  children: ReactNode;
  locale: string;
  messages: Record<string, unknown>;
}

/** Renders the providers component. */
export function Providers({ children, locale, messages }: ProvidersProps) {
  return (
    <NextIntlClientProvider locale={locale} messages={messages} timeZone="UTC">
      {children}
    </NextIntlClientProvider>
  );
}
