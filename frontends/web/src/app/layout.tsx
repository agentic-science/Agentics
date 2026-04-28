import type { Metadata } from "next";
import Link from "next/link";
import { ThemeSwitcher } from "@/components/ThemeSwitcher";
import "./globals.css";

export const metadata: Metadata = {
  title: "LLM OJ",
  description: "LLM 在线评测平台",
};

const themeScript = `
  (function() {
    const mode = localStorage.getItem('llm-oj-theme-mode') || 'system';
    const theme = mode === 'system'
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : mode;
    document.documentElement.dataset.theme = theme;
  })();
`;

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
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
