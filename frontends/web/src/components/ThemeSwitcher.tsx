"use client";

import { useEffect, useState } from "react";

type ThemeMode = "system" | "light" | "dark";
type Theme = "light" | "dark";

function getResolvedTheme(mode: ThemeMode): Theme {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return mode;
}

/**
 * Persisted client-side theme state.
 *
 * The hook resolves `system` to a concrete light/dark value so CSS can depend
 * on a stable `data-theme` attribute.
 */
export function useTheme(): [ThemeMode, Theme, (mode: ThemeMode) => void] {
  const [mode, setMode] = useState<ThemeMode>("system");
  const [resolved, setResolved] = useState<Theme>("light");

  useEffect(() => {
    const stored = localStorage.getItem(
      "llm-oj-theme-mode",
    ) as ThemeMode | null;
    const initial = stored ?? "system";
    setMode(initial);
    setResolved(getResolvedTheme(initial));
  }, []);

  useEffect(() => {
    const resolved = getResolvedTheme(mode);
    setResolved(resolved);
    document.documentElement.dataset.theme = resolved;
    localStorage.setItem("llm-oj-theme-mode", mode);
  }, [mode]);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      if (mode === "system") {
        setResolved(mq.matches ? "dark" : "light");
        document.documentElement.dataset.theme = mq.matches ? "dark" : "light";
      }
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [mode]);

  return [mode, resolved, setMode];
}

/** Three-way theme control for system, light, and dark modes. */
export function ThemeSwitcher() {
  const [mode, , setMode] = useTheme();

  return (
    <div className="theme-switcher">
      {(["system", "light", "dark"] as ThemeMode[]).map((m) => (
        <button
          key={m}
          type="button"
          className={mode === m ? "is-active" : ""}
          onClick={() => setMode(m)}
        >
          {m === "system" ? "系统" : m === "light" ? "浅色" : "深色"}
        </button>
      ))}
    </div>
  );
}
