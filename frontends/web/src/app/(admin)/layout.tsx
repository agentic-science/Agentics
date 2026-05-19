import Link from "next/link";
import { ThemeSwitcher } from "@/components/ThemeSwitcher";

/** Renders the admin layout component. */
export default function AdminLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <div className="site-shell">
      <header className="glass sticky top-0 z-50">
        <div className="site-header-main">
          <nav className="flex items-center justify-between h-11">
            <Link href="/admin" className="flex items-center gap-2 group">
              <span className="font-[var(--font-sans)] text-xl font-bold tracking-tight text-[var(--text-primary)] group-hover:text-[var(--accent-primary-text)] transition-colors">
                Agentics Admin
              </span>
              <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent-primary-500)]" />
            </Link>

            <div className="hidden sm:flex items-center gap-1">
              <Link
                href="/"
                className="px-3 py-1.5 rounded-md text-sm font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-secondary)] transition-colors"
              >
                Observer
              </Link>
            </div>

            <div className="flex items-center">
              <ThemeSwitcher />
            </div>
          </nav>
        </div>
      </header>

      <main className="site-main">{children}</main>
    </div>
  );
}
