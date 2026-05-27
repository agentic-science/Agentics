import Link from "next/link";
import { getTranslations } from "next-intl/server";
import { LanguageSwitcher } from "@/components/LanguageSwitcher";
import { ThemeSwitcher } from "@/components/ThemeSwitcher";

/** Renders the creator layout component. */
export default async function CreatorLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const t = await getTranslations("creator.layout");

  return (
    <div className="site-shell">
      <header className="glass sticky top-0 z-50">
        <div className="site-header-main">
          <nav className="flex items-center justify-between h-11">
            <Link href="/creator" className="flex items-center gap-2 group">
              <span className="font-sans text-xl font-bold tracking-tight text-fg group-hover:text-action-fg transition-colors">
                {t("brand")}
              </span>
              <span className="w-1.5 h-1.5 rounded-full bg-action" />
            </Link>

            <div className="hidden sm:flex items-center gap-1">
              <Link
                href="/"
                className="px-3 py-1.5 rounded-panel text-sm font-medium text-fg-secondary hover:text-fg hover:bg-surface-2 transition-colors"
              >
                {t("observer")}
              </Link>
              <Link
                href="/admin"
                className="px-3 py-1.5 rounded-panel text-sm font-medium text-fg-secondary hover:text-fg hover:bg-surface-2 transition-colors"
              >
                {t("admin")}
              </Link>
            </div>

            <div className="flex items-center gap-3">
              <LanguageSwitcher />
              <ThemeSwitcher />
            </div>
          </nav>
        </div>
      </header>

      <main className="site-main">{children}</main>
    </div>
  );
}
