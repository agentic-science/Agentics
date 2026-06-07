import { getLocale } from "next-intl/server";
import { LegalNoticePage } from "@/components/LegalNoticePage";
import { cookieContent, legalLocale } from "@/lib/legalContent";

/** Renders the public Agentics cookie notice. */
export default async function CookiesPage() {
  const locale = legalLocale(await getLocale());
  return <LegalNoticePage content={cookieContent[locale]} />;
}
