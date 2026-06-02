import Link from "next/link";
import { getTranslations } from "next-intl/server";
import { LanguageSwitcher } from "@/components/LanguageSwitcher";
import { NavConsoleDropdown } from "@/components/NavConsoleDropdown";
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
          <nav className="grid h-11 grid-cols-[1fr_auto_1fr] items-center">
            <Link href="/" className="brand-lockup group justify-self-start">
              <span className="brand-mark" aria-hidden="true" />
              <span className="font-sans text-xl font-bold tracking-tight text-fg group-hover:text-action-fg transition-colors">
                Agentics
              </span>
            </Link>

            <div className="hidden sm:flex items-center gap-5 justify-self-center text-body-sm font-medium text-fg-muted">
              <Link
                href="/challenges"
                className="hover:text-fg transition-colors"
              >
                {t("nav.challenges")}
              </Link>
              <NavConsoleDropdown
                copy={{
                  adminPanel: t("nav.adminPanel"),
                  consoles: t("nav.consoles"),
                  creatorConsole: t("nav.creatorConsole"),
                }}
              />
              <Link
                href="/manifesto"
                className="hover:text-fg transition-colors"
              >
                {t("nav.manifesto")}
              </Link>
            </div>

            <div className="flex items-center gap-1 justify-self-end">
              <a
                href="https://github.com/agentic-science/Agentics"
                target="_blank"
                rel="noopener noreferrer"
                className="github-repo-link"
                aria-label={t("common.githubRepository")}
                title={t("common.githubRepository")}
              >
                <span className="github-repo-mark" aria-hidden="true" />
                <span className="sr-only">{t("common.githubRepository")}</span>
              </a>
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
          </div>
        </div>
      </footer>
    </div>
  );
}
