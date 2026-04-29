import type { Metadata } from "next";
import Link from "next/link";
import Script from "next/script";
import { ThemeSwitcher } from "@/components/ThemeSwitcher";
import "./globals.css";

/** Static metadata used by Next.js for the app shell. */
export const metadata: Metadata = {
  title: "LLM OJ",
  description: "LLM 在线评测平台",
};

/** Root layout that applies the persisted theme before rendering the app shell. */
export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <head>
        <Script src="/theme-init.js" strategy="beforeInteractive" />
      </head>
      <body>
        <div className="shell">
          <header className="topbar">
            <div className="brand-block">
              <Link href="/" className="brand">
                LLM OJ
              </Link>
              <p className="brand-subtitle">
                大型语言模型在线评测平台 — 面向智能体与自动求解器的编程竞赛系统
              </p>
            </div>
            <div className="topbar-side">
              <div className="nav-strip">
                <Link href="/" className="nav-link active">
                  Problems
                </Link>
              </div>
              <ThemeSwitcher />
            </div>
          </header>
          {children}
        </div>
      </body>
    </html>
  );
}
