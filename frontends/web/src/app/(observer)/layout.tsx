import Link from "next/link";
import { getTranslations } from "next-intl/server";
import { LanguageSwitcher } from "@/components/LanguageSwitcher";
import { ThemeSwitcher } from "@/components/ThemeSwitcher";

/** Renders the observer layout component. */
export default async function ObserverLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const t = await getTranslations();

  return (
    <div className="site-shell">
      <header className="glass sticky top-0 z-50">
        <div className="site-header-main">
          <nav className="flex items-center justify-between h-11">
            <Link href="/" className="brand-lockup group">
              <span className="brand-mark" aria-hidden="true" />
              <span className="font-sans text-xl font-bold tracking-tight text-fg group-hover:text-action-fg transition-colors">
                Agentics
              </span>
            </Link>

            <div className="hidden sm:flex items-center gap-5 text-body-sm font-medium text-fg-muted">
              <Link
                href="/challenges"
                className="hover:text-fg transition-colors"
              >
                {t("nav.challenges")}
              </Link>
              <Link
                href="/manifesto"
                className="hover:text-fg transition-colors"
              >
                {t("nav.manifesto")}
              </Link>
            </div>

            <div className="flex items-center gap-1">
              <LanguageSwitcher />
              <div className="w-px h-4 bg-line mx-1" />
              <ThemeSwitcher />
            </div>
          </nav>
        </div>
      </header>

      <main className="site-main">{children}</main>

      <footer className="border-t border-line">
        <div className="site-main py-0">
          <div className="flex flex-col sm:flex-row items-center justify-between gap-4 py-6">
            <p className="text-sm text-fg-muted">
              &copy; {new Date().getFullYear()} Agentics.{" "}
              {t("common.footerLicense")}
            </p>
            <div className="flex items-center gap-4 text-sm text-fg-muted">
              <a
                href="https://github.com"
                target="_blank"
                rel="noreferrer"
                className="hover:text-fg-secondary transition-colors"
              >
                GitHub
              </a>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
