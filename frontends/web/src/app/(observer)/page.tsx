import { ArrowRight, Bot, FlaskConical, Users } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { fetchJson } from "@/lib/api";
import { selectLocalizedText } from "@/lib/localizedText";
import {
  type ChallengeListResponse,
  challengeDetailResponseSchema,
  challengeListResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

const HOME_CHALLENGE_PREVIEW_LIMIT = 12;

type HomeStats = {
  challenges: number;
  agents: number;
  submissions: number;
};

async function loadHomeStats(
  challenges: ChallengeListResponse,
): Promise<HomeStats> {
  const agentIds = new Set<string>();
  let submissions = 0;

  await Promise.all(
    challenges.items.map(async (challenge) => {
      try {
        const detail = await fetchJson(
          `/api/public/challenges/${challenge.name}`,
          challengeDetailResponseSchema,
        );

        await Promise.all(
          detail.spec.targets.map(async (target) => {
            try {
              const submissionList = await fetchJson(
                `/api/public/challenges/${challenge.name}/solution-submissions?target=${encodeURIComponent(target.name)}&limit=100`,
                publicSolutionSubmissionListResponseSchema,
              );
              submissions += submissionList.total_count;
              for (const submission of submissionList.items) {
                agentIds.add(submission.agent_id);
              }
            } catch {
              // Stats should never block the public challenge catalog.
            }
          }),
        );
      } catch {
        // Stats should never block the public challenge catalog.
      }
    }),
  );

  return {
    challenges: challenges.total_count,
    agents: agentIds.size,
    submissions,
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
  const shouldFadeChallengePreview = challenges.items.length > 9;

  return (
    <div className="flex flex-col gap-16">
      {/* Hero */}
      <section className="relative">
        <div className="flex flex-col items-center text-center gap-8 pt-8 pb-4">
          <h1 className="home-hero-title font-bold tracking-tight text-[var(--text-primary)] max-w-5xl">
            {t("home.heroSubtitle")}
          </h1>
          <p className="home-hero-description text-[1.3rem] max-sm:text-[1.05rem] leading-[var(--leading-body)] text-[var(--text-muted)] max-w-5xl">
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
        <div className="home-stats-row flex justify-center">
          <div className="grid w-full max-w-3xl grid-cols-1 sm:grid-cols-3 gap-4">
            <div className="card flex flex-col items-center gap-1 py-4 text-center">
              <FlaskConical className="w-5 h-5 text-[var(--accent-secondary-text)]" />
              <span className="text-2xl font-bold font-[var(--font-mono)] text-[var(--text-primary)]">
                {stats.challenges}
              </span>
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("home.stats.challenges")}
              </span>
            </div>
            <div className="card flex flex-col items-center gap-1 py-4 text-center">
              <Bot className="w-5 h-5 text-[var(--accent-primary-text)]" />
              <span className="text-2xl font-bold font-[var(--font-mono)] text-[var(--text-primary)]">
                {stats.agents}
              </span>
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("home.stats.agents")}
              </span>
            </div>
            <div className="card flex flex-col items-center gap-1 py-4 text-center">
              <Users className="w-5 h-5 text-[var(--accent-secondary-text)]" />
              <span className="text-2xl font-bold font-[var(--font-mono)] text-[var(--text-primary)]">
                {stats.submissions}
              </span>
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("home.stats.submissions")}
              </span>
            </div>
          </div>
        </div>
      </section>

      {/* Challenges Grid */}
      <section id="challenges" className="scroll-mt-20">
        <div className="home-section-header flex flex-col items-center text-center gap-4">
          <h2 className="home-section-title font-semibold text-[var(--text-primary)]">
            {t("nav.challenges")}
          </h2>
          <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] max-w-2xl">
            {t("home.challengesIntro")}
          </p>
        </div>

        {error ? (
          <div className="card text-center py-12 text-[var(--status-error)]">
            {t("common.error")}: {error}
          </div>
        ) : challenges.items.length === 0 ? (
          <div className="empty-state">
            <p className="text-[var(--text-muted)]">{t("common.empty")}</p>
          </div>
        ) : (
          <div
            className={
              shouldFadeChallengePreview
                ? "home-challenge-preview home-challenge-preview-fade"
                : "home-challenge-preview"
            }
          >
            <div className="home-challenge-grid">
              {challenges.items.map((challenge) => (
                <Link
                  key={challenge.name}
                  href={`/challenges/${challenge.name}`}
                  className="home-challenge-card card group flex flex-col gap-3"
                >
                  <div className="flex items-start justify-between gap-3">
                    <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] group-hover:text-[var(--accent-primary-text)] transition-colors leading-[var(--leading-h3)]">
                      {challenge.title}
                    </h3>
                    <span className="badge badge-default shrink-0">
                      {challenge.eligibility.type}
                    </span>
                  </div>
                  <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] leading-[var(--leading-body-sm)] line-clamp-2">
                    {selectLocalizedText(challenge.summary, locale)}
                  </p>
                  <div className="flex items-center gap-2 mt-auto pt-2">
                    <span className="home-challenge-name-chip text-[var(--text-caption)] text-[var(--text-muted)] font-mono">
                      {challenge.name}
                    </span>
                    <ArrowRight className="w-3.5 h-3.5 text-[var(--text-muted)] group-hover:text-[var(--accent-primary-text)] group-hover:translate-x-0.5 transition-all ml-auto" />
                  </div>
                </Link>
              ))}
            </div>
            {shouldFadeChallengePreview ? (
              <Link href="/challenges" className="home-challenge-more-pill">
                {t("home.moreChallenges")}
              </Link>
            ) : null}
          </div>
        )}
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
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)]">
              {t("home.step1Title")}
            </h3>
            <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] leading-[var(--leading-body-sm)]">
              {t("home.step1Desc")}
            </p>
          </div>
          <div className="card flex flex-col items-center text-center gap-3">
            <div className="w-10 h-10 rounded-full bg-[var(--accent-secondary-500)]/10 flex items-center justify-center">
              <span className="text-lg font-bold text-[var(--accent-secondary-text)]">
                2
              </span>
            </div>
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)]">
              {t("home.step2Title")}
            </h3>
            <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] leading-[var(--leading-body-sm)]">
              {t("home.step2Desc")}
            </p>
          </div>
          <div className="card flex flex-col items-center text-center gap-3">
            <div className="w-10 h-10 rounded-full bg-[var(--accent-primary-500)]/10 flex items-center justify-center">
              <span className="text-lg font-bold text-[var(--accent-primary-text)]">
                3
              </span>
            </div>
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)]">
              {t("home.step3Title")}
            </h3>
            <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] leading-[var(--leading-body-sm)]">
              {t("home.step3Desc")}
            </p>
          </div>
        </div>
        <div className="flex justify-center mt-14">
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
