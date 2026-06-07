"use client";

import { Moon, Sun } from "lucide-react";
import { useTranslations } from "next-intl";
import { useEffect, useState } from "react";
import {
  applyThemeMode,
  onThemeModeChanged,
  type ResolvedTheme,
  readGlobalThemeMode,
  resolveThemeMode,
  type ThemeMode,
  updateAccountAppearancePreferences,
} from "@/lib/appearancePreferences";
import { useHumanSession } from "@/lib/humanSession";

/** Renders the theme switcher component. */
export function ThemeSwitcher() {
  const t = useTranslations("theme");
  const [mode, setMode] = useState<ThemeMode>("system");
  const [resolved, setResolved] = useState<ResolvedTheme>("dark");
  const [mounted, setMounted] = useState(false);
  const { data: session } = useHumanSession();

  useEffect(() => {
    setMounted(true);
    const initial = readGlobalThemeMode();
    setMode(initial);
    const r = resolveThemeMode(initial);
    setResolved(r);
    document.documentElement.dataset.theme = r;

    return onThemeModeChanged((nextMode, nextResolved) => {
      setMode(nextMode);
      setResolved(nextResolved);
    });
  }, []);

  useEffect(() => {
    if (!mounted) return;
    const r = resolveThemeMode(mode);
    setResolved(r);
    document.documentElement.dataset.theme = r;
  }, [mode, mounted]);

  useEffect(() => {
    if (!mounted) return;
    if (typeof window.matchMedia !== "function") return;
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
    const next = order[(idx + 1) % order.length];
    setMode(next);
    setResolved(applyThemeMode(next));
    if (session?.human_id) {
      void updateAccountAppearancePreferences(session.human_id, {
        mode: next,
      }).catch(() => {
        // Header preferences are best-effort local browser state.
      });
    }
  };

  if (!mounted) {
    return (
      <button
        type="button"
        className="btn btn-ghost btn-sm"
        aria-label={t("button")}
      >
        <Sun className="w-4 h-4" />
      </button>
    );
  }

  const label = t("current", { mode: t(`mode.${mode}`) });

  return (
    <button
      type="button"
      className="btn btn-ghost btn-sm"
      onClick={cycle}
      aria-label={label}
      title={label}
    >
      {resolved === "dark" ? (
        <Moon className="w-4 h-4" />
      ) : (
        <Sun className="w-4 h-4" />
      )}
    </button>
  );
}
