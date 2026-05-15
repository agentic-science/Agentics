"use client";

import { Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";

/** Describes the theme mode shape used by this module. */
type ThemeMode = "system" | "light" | "dark";
/** Describes the resolved theme shape used by this module. */
type ResolvedTheme = "light" | "dark";

/** Fetches resolved theme for the requested UI scope. */
function getResolvedTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return mode;
}

/** Renders the theme switcher component. */
export function ThemeSwitcher() {
  const [mode, setMode] = useState<ThemeMode>("system");
  const [resolved, setResolved] = useState<ResolvedTheme>("dark");
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
    const stored = localStorage.getItem("agentics-theme") as ThemeMode | null;
    const initial = stored ?? "system";
    setMode(initial);
    const r = getResolvedTheme(initial);
    setResolved(r);
    document.documentElement.dataset.theme = r;
  }, []);

  useEffect(() => {
    if (!mounted) return;
    const r = getResolvedTheme(mode);
    setResolved(r);
    document.documentElement.dataset.theme = r;
    localStorage.setItem("agentics-theme", mode);
  }, [mode, mounted]);

  useEffect(() => {
    if (!mounted) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    /** Handles handler user interaction. */
    const handler = () => {
      if (mode === "system") {
        const r = mq.matches ? "dark" : "light";
        setResolved(r);
        document.documentElement.dataset.theme = r;
      }
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [mode, mounted]);

  /** Cycles through the supported theme modes. */
  const cycle = () => {
    const order: ThemeMode[] = ["system", "light", "dark"];
    const idx = order.indexOf(mode);
    setMode(order[(idx + 1) % order.length]);
  };

  if (!mounted) {
    return (
      <button type="button" className="btn btn-ghost btn-sm" aria-label="Theme">
        <Sun className="w-4 h-4" />
      </button>
    );
  }

  return (
    <button
      type="button"
      className="btn btn-ghost btn-sm"
      onClick={cycle}
      aria-label={`Theme: ${mode}`}
      title={`Theme: ${mode}`}
    >
      {resolved === "dark" ? (
        <Moon className="w-4 h-4" />
      ) : (
        <Sun className="w-4 h-4" />
      )}
    </button>
  );
}
