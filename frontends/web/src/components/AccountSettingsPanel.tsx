"use client";

import { Trash2 } from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useLocale, useTranslations } from "next-intl";
import { type FormEvent, useEffect, useId, useState } from "react";
import { mutate } from "swr";
import {
  type AppearancePreferences,
  applyLanguagePreference,
  applyThemeMode,
  DEFAULT_APPEARANCE_PREFERENCES,
  type LanguagePreference,
  loadAccountAppearancePreferences,
  saveAccountAppearancePreferences,
  type ThemeMode,
} from "@/lib/appearancePreferences";
import { deleteHumanAccount } from "@/lib/authApi";
import { clearCreatorApiTokenCaches } from "@/lib/creatorData";
import { useHumanSession } from "@/lib/humanSession";

/** Renders signed-in account identity and deletion controls. */
export function AccountSettingsPanel() {
  const t = useTranslations("accountSettings");
  const statusT = useTranslations("common.statuses");
  const locale = useLocale();
  const router = useRouter();
  const [confirmation, setConfirmation] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [appearance, setAppearance] = useState<AppearancePreferences>({
    ...DEFAULT_APPEARANCE_PREFERENCES,
  });
  const [appearanceError, setAppearanceError] = useState<string | null>(null);
  const { data: session, isLoading } = useHumanSession();

  useEffect(() => {
    if (!session?.human_id) {
      setAppearance({ ...DEFAULT_APPEARANCE_PREFERENCES });
      return;
    }

    let canceled = false;
    void loadAccountAppearancePreferences(session.human_id)
      .then((preferences) => {
        if (!canceled) {
          setAppearance(preferences);
        }
      })
      .catch(() => {
        if (!canceled) {
          setAppearanceError(t("appearanceSaveFailed"));
        }
      });

    return () => {
      canceled = true;
    };
  }, [session?.human_id, t]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!session?.csrf_token || confirmation !== t("confirmation")) {
      return;
    }
    setPending(true);
    setError(null);
    try {
      await deleteHumanAccount(session.csrf_token);
      await mutate(() => true, undefined, { revalidate: false });
      await clearCreatorApiTokenCaches();
      router.replace("/");
    } catch (e) {
      setError(e instanceof Error ? e.message : t("failed"));
      setPending(false);
    }
  };

  const saveAppearance = async (next: AppearancePreferences) => {
    setAppearance(next);
    setAppearanceError(null);
    if (!session?.human_id) {
      return;
    }
    try {
      await saveAccountAppearancePreferences(session.human_id, next);
    } catch {
      setAppearanceError(t("appearanceSaveFailed"));
    }
  };

  const chooseLanguage = (language: LanguagePreference) => {
    const next = { ...appearance, language };
    void saveAppearance(next).then(() => {
      applyLanguagePreference(language, locale);
    });
  };

  const chooseMode = (mode: ThemeMode) => {
    const next = { ...appearance, mode };
    applyThemeMode(mode);
    void saveAppearance(next);
  };

  return (
    <section className="account-settings max-w-3xl mx-auto">
      <h1 className="text-h1 font-bold leading-h1">{t("title")}</h1>
      {isLoading ? (
        <p className="mt-3 text-body text-fg-secondary">{t("loading")}</p>
      ) : null}
      {!isLoading && !session ? (
        <div className="mt-5 grid gap-4">
          <p className="text-body text-fg-secondary">{t("signInRequired")}</p>
          <Link
            className="btn btn-primary w-fit"
            href="/sign-in?return_to=%2Faccount"
          >
            {t("signIn")}
          </Link>
        </div>
      ) : null}
      {session ? (
        <div className="mt-6 grid gap-6">
          <section className="card-elevated">
            <h2 className="text-h3 font-semibold">{t("identity")}</h2>
            <dl className="account-summary mt-4">
              <div>
                <dt>{t("githubLogin")}</dt>
                <dd>@{session.github_login}</dd>
              </div>
              <div>
                <dt>{t("status")}</dt>
                <dd>{statusT(session.status)}</dd>
              </div>
              <div>
                <dt>{t("roles")}</dt>
                <dd>{session.roles.length ? session.roles.join(", ") : "-"}</dd>
              </div>
            </dl>
          </section>

          <section className="card-elevated">
            <h2 className="text-h3 font-semibold">{t("appearanceTitle")}</h2>
            <div className="mt-4 grid gap-4">
              <SegmentedPreference
                label={t("appearanceLanguage")}
                value={appearance.language}
                options={[
                  { value: "auto", label: t("languageAuto") },
                  { value: "en", label: t("languageEn") },
                  { value: "zh", label: t("languageZh") },
                ]}
                onChange={chooseLanguage}
              />
              <SegmentedPreference
                label={t("appearanceMode")}
                value={appearance.mode}
                options={[
                  { value: "system", label: t("modeSystem") },
                  { value: "light", label: t("modeLight") },
                  { value: "dark", label: t("modeDark") },
                ]}
                onChange={chooseMode}
              />
              {appearanceError ? (
                <p className="text-body-sm text-danger">{appearanceError}</p>
              ) : null}
            </div>
          </section>

          <form className="card-elevated danger-zone" onSubmit={submit}>
            <div className="flex items-center gap-2">
              <Trash2 className="w-4 h-4 text-danger" />
              <h2 className="text-h3 font-semibold">{t("dangerTitle")}</h2>
            </div>
            <p className="mt-3 text-body-sm text-fg-secondary">
              {t("dangerBody")}
            </p>
            <label className="form-field mt-4">
              <span>{t("confirmLabel")}</span>
              <input
                autoComplete="off"
                onChange={(event) => setConfirmation(event.target.value)}
                placeholder={t("confirmation")}
                value={confirmation}
              />
            </label>
            {error ? <p className="text-body-sm text-danger">{error}</p> : null}
            <button
              className="btn btn-danger mt-4"
              disabled={pending || confirmation !== t("confirmation")}
              type="submit"
            >
              {pending ? t("deleting") : t("deleteButton")}
            </button>
          </form>
        </div>
      ) : null}
    </section>
  );
}

interface SegmentedPreferenceProps<T extends string> {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}

/** Renders a compact segmented account preference selector. */
function SegmentedPreference<T extends string>({
  label,
  value,
  options,
  onChange,
}: SegmentedPreferenceProps<T>) {
  const groupName = useId();

  return (
    <div className="appearance-preference-row">
      <span className="form-field-label">{label}</span>
      <div aria-label={label} className="segmented-control" role="radiogroup">
        {options.map((option) => (
          <label
            className="segmented-control-option"
            data-active={value === option.value ? "true" : undefined}
            key={option.value}
          >
            <input
              checked={value === option.value}
              className="sr-only"
              name={groupName}
              onChange={() => onChange(option.value)}
              type="radio"
            />
            {option.label}
          </label>
        ))}
      </div>
    </div>
  );
}
