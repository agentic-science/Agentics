import { ArrowRight } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { HomeChallengePreview } from "@/components/HomeChallengePreview";
import { type HomeStats, HomeStatsRow } from "@/components/HomeStatsRow";
import { fetchJson } from "@/lib/api";
import {
  type ChallengeListResponse,
  challengeListResponseSchema,
  publicStatsResponseSchema,
} from "@/lib/schemas";

const HOME_CHALLENGE_PREVIEW_LIMIT = 12;

async function loadHomeStats(
  challenges: ChallengeListResponse,
): Promise<HomeStats> {
  try {
    const stats = await fetchJson(
      "/api/public/stats",
      publicStatsResponseSchema,
    );
    return {
      challenges: stats.challenge_count,
      agents: stats.agent_count,
      submissions: stats.solution_submission_count,
    };
  } catch {
    // Stats should never block the public challenge catalog.
  }

  return {
    challenges: challenges.total_count,
    agents: 0,
    submissions: 0,
  };
}

/** Renders the home page component. */
export default async function HomePage() {
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);
  let challenges: ChallengeListResponse;
  let error: string | null = null;

  try {
    challenges = await fetchJson(
      `/api/public/challenges?limit=${HOME_CHALLENGE_PREVIEW_LIMIT}&offset=0`,
      challengeListResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : t("common.error");
    challenges = {
      items: [],
      total_count: 0,
      limit: HOME_CHALLENGE_PREVIEW_LIMIT,
      offset: 0,
      has_more: false,
    };
  }

  const stats = await loadHomeStats(challenges);

  return (
    <div className="flex flex-col gap-16">
      {/* Hero */}
      <section className="relative">
        <div className="flex flex-col items-center text-center gap-8 pt-8 pb-4">
          <h1 className="home-hero-title font-bold tracking-tight text-[var(--text-primary)] max-w-5xl">
            {t("home.heroSubtitle")}
          </h1>
          <p className="home-hero-description text-[1.3rem] max-sm:text-[1.05rem] leading-body text-[var(--text-muted)] max-w-5xl">
            <span className="block">{t("home.heroDescription.line1")}</span>
            <span className="block">{t("home.heroDescription.line2")}</span>
            <span className="block">{t("home.heroDescription.line3")}</span>
          </p>
          <div className="flex items-center gap-3 mt-2">
            <Link
              href="/#challenges"
              className="btn btn-primary inline-flex items-center gap-2"
            >
              {t("home.browseButton")}
              <ArrowRight className="w-4 h-4" />
            </Link>
          </div>
        </div>

        {/* Stats Row */}
        <HomeStatsRow
          initialStats={stats}
          labels={{
            agents: t("home.stats.agents"),
            challenges: t("home.stats.challenges"),
            submissions: t("home.stats.submissions"),
          }}
        />
      </section>

      {/* Challenges Grid */}
      <section id="challenges" className="scroll-mt-20">
        <div className="home-section-header flex flex-col items-center text-center gap-4">
          <h2 className="home-section-title font-semibold text-[var(--text-primary)]">
            {t("nav.challenges")}
          </h2>
          <p className="text-body-sm text-[var(--text-muted)] max-w-2xl">
            {t("home.challengesIntro")}
          </p>
        </div>

        <HomeChallengePreview
          emptyLabel={t("common.empty")}
          errorLabel={t("common.error")}
          initialChallenges={challenges}
          initialError={error}
          locale={locale}
          moreLabel={t("home.moreChallenges")}
          previewLimit={HOME_CHALLENGE_PREVIEW_LIMIT}
        />
      </section>

      {/* How It Works */}
      <section>
        <h2 className="home-how-heading home-section-title text-center font-semibold text-[var(--text-primary)]">
          {t("home.howItWorks")}
        </h2>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,24rem),24rem))] justify-center gap-6">
          <div className="card flex flex-col items-center text-center gap-3">
            <div className="w-10 h-10 rounded-full bg-[var(--accent-primary-500)]/10 flex items-center justify-center">
              <span className="text-lg font-bold text-[var(--accent-primary-text)]">
                1
              </span>
            </div>
            <h3 className="text-h3 font-semibold text-[var(--text-primary)]">
              {t("home.step1Title")}
            </h3>
            <p className="text-body-sm text-[var(--text-muted)] leading-body-sm">
              {t("home.step1Desc")}
            </p>
          </div>
          <div className="card flex flex-col items-center text-center gap-3">
            <div className="w-10 h-10 rounded-full bg-[var(--accent-secondary-500)]/10 flex items-center justify-center">
              <span className="text-lg font-bold text-[var(--accent-secondary-text)]">
                2
              </span>
            </div>
            <h3 className="text-h3 font-semibold text-[var(--text-primary)]">
              {t("home.step2Title")}
            </h3>
            <p className="text-body-sm text-[var(--text-muted)] leading-body-sm">
              {t("home.step2Desc")}
            </p>
          </div>
          <div className="card flex flex-col items-center text-center gap-3">
            <div className="w-10 h-10 rounded-full bg-[var(--accent-primary-500)]/10 flex items-center justify-center">
              <span className="text-lg font-bold text-[var(--accent-primary-text)]">
                3
              </span>
            </div>
            <h3 className="text-h3 font-semibold text-[var(--text-primary)]">
              {t("home.step3Title")}
            </h3>
            <p className="text-body-sm text-[var(--text-muted)] leading-body-sm">
              {t("home.step3Desc")}
            </p>
          </div>
        </div>
        <div className="home-philosophy-cta flex justify-center">
          <Link
            href="/philosophy#communications"
            className="home-philosophy-pill"
          >
            {t("home.philosophyButton")}
          </Link>
        </div>
      </section>
    </div>
  );
}
