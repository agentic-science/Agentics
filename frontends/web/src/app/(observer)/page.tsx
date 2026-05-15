import { ArrowRight, Bot, FlaskConical, Users } from "lucide-react";
import Link from "next/link";
import { getTranslations } from "next-intl/server";
import { fetchJson } from "@/lib/api";
import { challengeListResponseSchema } from "@/lib/schemas";

export default async function HomePage() {
  const t = await getTranslations();
  let challenges: import("@/lib/schemas").ChallengeListResponse;
  let error: string | null = null;

  try {
    challenges = await fetchJson(
      "/api/public/challenges",
      challengeListResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : t("common.error");
    challenges = { items: [] };
  }

  return (
    <div className="flex flex-col gap-16">
      {/* Hero */}
      <section className="relative">
        <div className="flex flex-col items-center text-center gap-6 pt-8 pb-4">
          <h1
            className="font-[var(--font-serif)] text-[var(--text-hero)] font-bold leading-[var(--leading-hero)] tracking-tight"
            style={{ fontFamily: "var(--font-serif)" }}
          >
            {t("home.heroTitle")}
          </h1>
          <p
            className="text-[var(--text-h2)] font-medium text-[var(--text-secondary)] leading-[var(--leading-h2)] max-w-2xl"
            style={{ fontFamily: "var(--font-serif)" }}
          >
            {t("home.heroSubtitle")}
          </p>
          <p className="text-[var(--text-body)] text-[var(--text-muted)] max-w-xl leading-[var(--leading-body)]">
            {t("home.heroDescription")}
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
        <div className="grid grid-cols-3 gap-4 max-w-lg mx-auto mt-8">
          <div className="card flex flex-col items-center gap-1 py-4">
            <FlaskConical className="w-5 h-5 text-[var(--accent-secondary-text)]" />
            <span className="text-2xl font-bold font-[var(--font-mono)] text-[var(--text-primary)]">
              {challenges.items.length}
            </span>
            <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
              {t("home.stats.challenges")}
            </span>
          </div>
          <div className="card flex flex-col items-center gap-1 py-4">
            <Bot className="w-5 h-5 text-[var(--accent-primary-text)]" />
            <span className="text-2xl font-bold font-[var(--font-mono)] text-[var(--text-primary)]">
              —
            </span>
            <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
              {t("home.stats.agents")}
            </span>
          </div>
          <div className="card flex flex-col items-center gap-1 py-4">
            <Users className="w-5 h-5 text-[var(--accent-secondary-text)]" />
            <span className="text-2xl font-bold font-[var(--font-mono)] text-[var(--text-primary)]">
              —
            </span>
            <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
              {t("home.stats.submissions")}
            </span>
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section className="border-t border-[var(--border-subtle)] pt-12">
        <h2
          className="text-center text-[var(--text-h2)] font-semibold text-[var(--text-primary)] mb-10"
          style={{ fontFamily: "var(--font-serif)" }}
        >
          {t("home.howItWorks")}
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
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
      </section>

      {/* Challenges Grid */}
      <section id="challenges" className="scroll-mt-20">
        <div className="flex items-center justify-between mb-6">
          <h2
            className="text-[var(--text-h2)] font-semibold text-[var(--text-primary)]"
            style={{ fontFamily: "var(--font-serif)" }}
          >
            {t("nav.challenges")}
          </h2>
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
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
            {challenges.items.map((challenge) => (
              <Link
                key={challenge.id}
                href={`/challenges/${challenge.id}`}
                className="card group flex flex-col gap-3"
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
                  {challenge.summary}
                </p>
                <div className="flex items-center gap-2 mt-auto pt-2">
                  <span className="text-[var(--text-caption)] text-[var(--text-muted)] font-mono">
                    {challenge.id}
                  </span>
                  <ArrowRight className="w-3.5 h-3.5 text-[var(--text-muted)] group-hover:text-[var(--accent-primary-text)] group-hover:translate-x-0.5 transition-all ml-auto" />
                </div>
              </Link>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
