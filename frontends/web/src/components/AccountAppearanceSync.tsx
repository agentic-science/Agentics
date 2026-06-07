"use client";

import { useLocale } from "next-intl";
import { useEffect } from "react";
import {
  applyLanguagePreference,
  applyThemeMode,
  readStoredAccountAppearancePreferences,
} from "@/lib/appearancePreferences";
import { useHumanSession } from "@/lib/humanSession";

/** Applies browser-local account appearance preferences after sign-in. */
export function AccountAppearanceSync() {
  const locale = useLocale();
  const { data: session } = useHumanSession();

  useEffect(() => {
    if (!session?.human_id) {
      return;
    }

    let canceled = false;
    void readStoredAccountAppearancePreferences(session.human_id)
      .then((preferences) => {
        if (canceled || !preferences) {
          return;
        }
        applyThemeMode(preferences.mode);
        applyLanguagePreference(preferences.language, locale);
      })
      .catch(() => {
        // Browser-local preferences are best-effort and should not block pages.
      });

    return () => {
      canceled = true;
    };
  }, [locale, session?.human_id]);

  return null;
}
