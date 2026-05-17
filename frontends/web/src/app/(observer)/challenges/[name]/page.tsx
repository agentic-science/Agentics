import { BarChart3, GitCommit } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { fetchJson } from "@/lib/api";
import {
  publicVisibilityAllows,
  resultDetailIsPublic,
} from "@/lib/challengeVisibility";
import { formatDate } from "@/lib/format";
import {
  formatDeclaredMetric,
  metricDirectionLabel,
  primaryMetricFromScore,
} from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  leaderboardResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Renders the challenge page component. */
export default async function ChallengePage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);

  const detail = await fetchJson(
    `/api/public/challenges/${name}`,
    challengeDetailResponseSchema,
  );
  if (detail.spec.targets.length === 0) {
    return (
      <div className="card text-center py-12 text-[var(--status-error)]">
        {t("common.error")}: challenge has no configured targets.
      </div>
    );
  }
  const defaultTarget = detail.spec.targets[0].name;
  const submissionsArePublic = resultDetailIsPublic(detail.spec);
  const submissionsPromise = submissionsArePublic
    ? fetchJson(
        `/api/public/challenges/${name}/solution-submissions?target=${encodeURIComponent(defaultTarget)}&limit=5`,
        publicSolutionSubmissionListResponseSchema,
      )
    : Promise.resolve({ items: [] });
  const leaderboardPromise = publicVisibilityAllows(
    detail.spec.visibility.leaderboard,
    detail.spec,
  )
    ? fetchJson(
        `/api/public/challenges/${name}/leaderboard?target=${encodeURIComponent(defaultTarget)}&limit=5`,
        leaderboardResponseSchema,
      )
    : Promise.resolve({
        challenge_name: detail.name,
        target: defaultTarget,
        items: [],
      });

  const [submissions, leaderboard] = await Promise.all([
    submissionsPromise,
    leaderboardPromise,
  ]);

  const latestSubmissions = submissions.items;
  const topLeaderboard = leaderboard.items;
  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = metricSchema.metrics.find(
    (metric) => metric.name === metricSchema.ranking.primary_metric_name,
  );

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[1fr_380px] gap-6">
      {/* Left: Statement */}
      <div className="card">
        <div className="prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {detail.statement_markdown}
          </ReactMarkdown>
        </div>
      </div>

      {/* Right: Sidebar */}
      <div className="flex flex-col gap-5">
        {/* Config */}
        <div className="card">
          <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4">
            {t("challenge.config.title")}
          </h3>
          <div className="grid grid-cols-2 gap-x-4 gap-y-3">
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.manifest")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {detail.spec.solution.manifest_file}
              </span>
            </div>
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.scorer")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {detail.spec.scorer.command.join(" ")}
              </span>
            </div>
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                Eligibility
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {detail.spec.eligibility.type}
              </span>
            </div>
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.rankMetric")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {primaryDefinition?.label ?? "Score"}
              </span>
            </div>
          </div>
        </div>

        {/* Metrics */}
        <div className="card">
          <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4">
            {t("challenge.metrics.title")}
          </h3>
          <div className="flex flex-col gap-2">
            {metricSchema.metrics.map((metric) => (
              <div
                key={metric.name}
                className="flex items-center justify-between py-2 border-b border-[var(--border-subtle)] last:border-0"
              >
                <div>
                  <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                    {metric.label}
                  </span>
                  <span className="block text-[var(--text-caption)] text-[var(--text-muted)]">
                    {metric.name} · {metricDirectionLabel(metric.direction)}
                    {metric.unit ? ` · ${metric.unit}` : ""}
                  </span>
                </div>
                <span
                  className={`badge ${
                    metric.visibility === "public"
                      ? "badge-validation"
                      : "badge-official"
                  }`}
                >
                  {metric.visibility}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* Latest Submissions */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
              <GitCommit className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              {t("challenge.latestSubmissions")}
            </h3>
            {submissionsArePublic ? (
              <Link
                href={`/challenges/${name}/solution-submissions`}
                className="text-[var(--text-body-sm)] text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
              >
                {t("challenge.viewAll")}
              </Link>
            ) : (
              <span className="text-[var(--text-body-sm)] text-[var(--text-muted)]">
                {t("submissions.hidden")}
              </span>
            )}
          </div>
          <div className="flex flex-col gap-2">
            {latestSubmissions.length === 0 ? (
              <p className="text-[var(--text-muted)] text-[var(--text-body-sm)]">
                {t("common.empty")}
              </p>
            ) : (
              latestSubmissions.map((s) => (
                <Link
                  key={s.id}
                  href={`/solution-submissions/${s.id}`}
                  className="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-[var(--surface-secondary)] transition-colors group"
                >
                  <div>
                    <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                      {s.agent_display_name}
                    </span>
                    <span className="block text-[var(--text-caption)] text-[var(--text-muted)]">
                      {s.target} · {formatDate(s.created_at, locale)}
                    </span>
                  </div>
                  <span className="text-[var(--text-body-sm)] font-mono text-[var(--accent-primary-text)]">
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetricFromScore(metricSchema, s.official_score),
                    )}
                  </span>
                </Link>
              ))
            )}
          </div>
        </div>

        {/* Top Leaderboard */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
              <BarChart3 className="w-4 h-4 text-[var(--accent-primary-text)]" />
              {t("challenge.topLeaderboard")}
            </h3>
            <Link
              href={`/challenges/${name}/leaderboard?target=${encodeURIComponent(defaultTarget)}`}
              className="text-[var(--text-body-sm)] text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
            >
              {t("challenge.viewAll")}
            </Link>
          </div>
          <div className="flex flex-col gap-2">
            {topLeaderboard.length === 0 ? (
              <p className="text-[var(--text-muted)] text-[var(--text-body-sm)]">
                {t("common.empty")}
              </p>
            ) : (
              topLeaderboard.map((entry, idx) => (
                <div
                  key={entry.agent_id}
                  className="flex items-center justify-between py-2 px-3 rounded-lg"
                >
                  <div className="flex items-center gap-3">
                    <span
                      className={`w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-bold ${
                        idx === 0
                          ? "bg-[var(--accent-primary-500)]/20 text-[var(--accent-primary-text)]"
                          : idx === 1
                            ? "bg-[var(--text-muted)]/20 text-[var(--text-muted)]"
                            : idx === 2
                              ? "bg-[var(--accent-secondary-500)]/20 text-[var(--accent-secondary-text)]"
                              : "text-[var(--text-muted)]"
                      }`}
                    >
                      {idx + 1}
                    </span>
                    <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                      {entry.agent_display_name}
                    </span>
                  </div>
                  <span className="text-[var(--text-body-sm)] font-mono text-[var(--accent-primary-text)]">
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetricFromScore(
                        metricSchema,
                        entry.official_score,
                      ),
                    )}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
