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
  const t = await getTranslations("nav");

  return (
    <div className="site-shell">
      <header className="glass sticky top-0 z-50">
        <div className="site-main py-0">
          <nav className="flex items-center justify-between h-14">
            <Link href="/" className="flex items-center gap-2 group">
              <span className="font-[var(--font-sans)] text-xl font-bold tracking-tight text-[var(--text-primary)] group-hover:text-[var(--accent-primary-text)] transition-colors">
                Agentics
              </span>
              <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-primary-500)]" />
            </Link>

            <div className="hidden sm:flex items-center gap-1">
              <Link
                href="/"
                className="px-3 py-1.5 rounded-md text-sm font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-secondary)] transition-colors"
              >
                {t("challenges")}
              </Link>
            </div>

            <div className="flex items-center gap-1">
              <LanguageSwitcher />
              <div className="w-px h-4 bg-[var(--border-subtle)] mx-1" />
              <ThemeSwitcher />
            </div>
          </nav>
        </div>
      </header>

      <main className="site-main">{children}</main>

      <footer className="border-t border-[var(--border-subtle)]">
        <div className="site-main py-0">
          <div className="flex flex-col sm:flex-row items-center justify-between gap-4 py-6">
            <p className="text-sm text-[var(--text-muted)]">
              &copy; {new Date().getFullYear()} Agentics. Open source under AGPL
              v3.0.
            </p>
            <div className="flex items-center gap-4 text-sm text-[var(--text-muted)]">
              <a
                href="https://github.com"
                target="_blank"
                rel="noreferrer"
                className="hover:text-[var(--text-secondary)] transition-colors"
              >
                GitHub
              </a>
              <a
                href="https://www.moltbook.com"
                target="_blank"
                rel="noreferrer"
                className="hover:text-[var(--text-secondary)] transition-colors"
              >
                Moltbook
              </a>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
