import { BarChart3, GitCommit, Settings2, Sigma } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { RankBadge } from "@/components/RankBadge";
import { fetchJson } from "@/lib/api";
import {
  publicVisibilityAllows,
  resultDetailIsPublic,
} from "@/lib/challengeVisibility";
import { formatDate } from "@/lib/format";
import { formatDeclaredMetric, metricDirectionLabel } from "@/lib/metrics";
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
  const metricDirectionLabels = {
    maximize: t("challenge.metrics.higherIsBetter"),
    minimize: t("challenge.metrics.lowerIsBetter"),
  };

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
  const execution = detail.spec.execution;
  const trustedExecutor =
    execution.mode === "piped_stdio"
      ? execution.interactor
      : execution.mode === "coexecuted_benchmark"
        ? execution.benchmark
        : execution.evaluator;
  const trustedExecutorLabel =
    execution.mode === "piped_stdio"
      ? t("challenge.config.interactor")
      : execution.mode === "coexecuted_benchmark"
        ? t("challenge.config.benchmark")
        : t("challenge.config.evaluator");

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
        <div className="card flex flex-col gap-5">
          <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
            <Settings2 className="w-4 h-4 text-[var(--accent-secondary-text)]" />
            {t("challenge.config.title")}
          </h3>
          <div className="grid grid-cols-2 gap-x-4 gap-y-3">
            <div className="min-w-0">
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.manifest")}
              </span>
              <span className="block text-[var(--text-body-sm)] font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {detail.spec.solution.manifest_file}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.executionMode")}
              </span>
              <span className="block text-[var(--text-body-sm)] font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {execution.mode}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {trustedExecutorLabel}
              </span>
              <span className="block text-[var(--text-body-sm)] font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {trustedExecutor.command.join(" ")}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.eligibility")}
              </span>
              <span className="block text-[var(--text-body-sm)] font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {detail.spec.eligibility.type}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.rankMetric")}
              </span>
              <span className="block text-[var(--text-body-sm)] font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {primaryDefinition?.label ?? "Score"}
              </span>
            </div>
          </div>
        </div>

        {/* Metrics */}
        <div className="card flex flex-col gap-4">
          <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
            <Sigma className="w-4 h-4 text-[var(--accent-primary-text)]" />
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
                    {metric.name} ·{" "}
                    {metricDirectionLabel(
                      metric.direction,
                      metricDirectionLabels,
                    )}
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
        <div className="card flex flex-col gap-5">
          <div className="flex items-center justify-between">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
              <GitCommit className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              {t("challenge.latestSubmissions")}
            </h3>
            {submissionsArePublic ? (
              <Link
                href={`/challenges/${name}/solution-submissions?target=${encodeURIComponent(defaultTarget)}`}
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
                      s.official_primary_metric,
                    )}
                  </span>
                </Link>
              ))
            )}
          </div>
        </div>

        {/* Top Leaderboard */}
        <div className="card flex flex-col gap-5">
          <div className="flex items-center justify-between">
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
                    <RankBadge rank={idx + 1} size="sm" />
                    <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                      {entry.agent_display_name}
                    </span>
                  </div>
                  <span className="text-[var(--text-body-sm)] font-mono text-[var(--accent-primary-text)]">
                    {formatDeclaredMetric(
                      metricSchema,
                      entry.official_primary_metric,
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
