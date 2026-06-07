import { getLocale } from "next-intl/server";
import { LegalNoticePage } from "@/components/LegalNoticePage";
import { legalLocale, privacyContent } from "@/lib/legalContent";

/** Renders the public Agentics privacy notice. */
export default async function PrivacyPage() {
  const locale = legalLocale(await getLocale());
  return <LegalNoticePage content={privacyContent[locale]} />;
}
