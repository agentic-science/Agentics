import { Settings2, Sigma } from "lucide-react";
import { getLocale, getTranslations } from "next-intl/server";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ChallengeLivePanels } from "@/components/ChallengeLivePanels";
import { fetchJson } from "@/lib/api";
import {
  publicVisibilityAllows,
  resultDetailIsPublic,
} from "@/lib/challengeVisibility";
import { metricDirectionLabel } from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  leaderboardResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Renders the challenge page component. */
export default async function ChallengePage({
  params,
}: {
  params: Promise<{ challengeName: string }>;
}) {
  const { challengeName } = await params;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);
  const metricDirectionLabels = {
    maximize: t("challenge.metrics.higherIsBetter"),
    minimize: t("challenge.metrics.lowerIsBetter"),
  };

  const detail = await fetchJson(
    `/api/public/challenges/${challengeName}`,
    challengeDetailResponseSchema,
  );
  if (detail.spec.targets.length === 0) {
    return (
      <div className="card text-center py-12 text-[var(--status-error)]">
        {t("common.error")}: {t("challenge.config.noTargets")}
      </div>
    );
  }
  const defaultTarget = detail.spec.targets[0].name;
  const submissionsArePublic = resultDetailIsPublic(detail.spec);
  const leaderboardIsPublic = publicVisibilityAllows(
    detail.spec.visibility.leaderboard,
    detail.spec,
  );
  const submissionsPromise = submissionsArePublic
    ? fetchJson(
        `/api/public/challenges/${challengeName}/solution-submissions?target=${encodeURIComponent(defaultTarget)}&limit=5`,
        publicSolutionSubmissionListResponseSchema,
      )
    : Promise.resolve({ total_count: 0, items: [] });
  const leaderboardPromise = leaderboardIsPublic
    ? fetchJson(
        `/api/public/challenges/${challengeName}/leaderboard?target=${encodeURIComponent(defaultTarget)}&limit=5`,
        leaderboardResponseSchema,
      )
    : Promise.resolve({
        challenge_name: detail.challenge_name,
        target: defaultTarget,
        items: [],
      });

  const [submissions, leaderboard] = await Promise.all([
    submissionsPromise,
    leaderboardPromise,
  ]);

  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = metricSchema.metrics.find(
    (metric) => metric.name === metricSchema.ranking.primary_metric_name,
  );
  const execution = detail.spec.execution;
  const trustedExecutor =
    execution.mode === "piped_stdio"
      ? execution.interactive_evaluator
      : execution.mode === "coexecuted_benchmark"
        ? execution.coexecuted_evaluator
        : execution.separated_evaluator;
  const trustedExecutorLabel =
    execution.mode === "piped_stdio"
      ? t("challenge.config.interactiveEvaluator")
      : execution.mode === "coexecuted_benchmark"
        ? t("challenge.config.coexecutedEvaluator")
        : t("challenge.config.separatedEvaluator");
  const executionModeLabel =
    execution.mode === "piped_stdio"
      ? t("challenge.config.executionModes.pipedStdio")
      : execution.mode === "coexecuted_benchmark"
        ? t("challenge.config.executionModes.coexecutedBenchmark")
        : t("challenge.config.executionModes.separatedEvaluator");
  const eligibilityLabel =
    detail.spec.eligibility.type === "private_shortlist"
      ? t("challenge.config.eligibilityTypes.privateShortlist")
      : t("challenge.config.eligibilityTypes.open");

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
          <h3 className="text-h3 font-semibold text-[var(--text-primary)] flex items-center gap-2">
            <Settings2 className="w-4 h-4 text-[var(--accent-secondary-text)]" />
            {t("challenge.config.title")}
          </h3>
          <div className="grid grid-cols-2 gap-x-4 gap-y-3">
            <div className="min-w-0">
              <span className="block text-caption text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.manifest")}
              </span>
              <span className="block text-body-sm font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {detail.spec.solution.manifest_file}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-caption text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.executionMode")}
              </span>
              <span className="block text-body-sm font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {executionModeLabel}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-caption text-[var(--text-muted)] uppercase tracking-wide">
                {trustedExecutorLabel}
              </span>
              <span className="block text-body-sm font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {trustedExecutor.command.join(" ")}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-caption text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.eligibility")}
              </span>
              <span className="block text-body-sm font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {eligibilityLabel}
              </span>
            </div>
            <div className="min-w-0">
              <span className="block text-caption text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.rankMetric")}
              </span>
              <span className="block text-body-sm font-mono text-[var(--text-primary)] [overflow-wrap:anywhere]">
                {primaryDefinition?.label ??
                  t("challenge.metrics.fallbackPrimaryMetric")}
              </span>
            </div>
          </div>
          {execution.mode === "coexecuted_benchmark" && (
            <p className="text-body-sm text-[var(--status-warning)] leading-relaxed">
              {t("challenge.config.coexecutedWarning")}
            </p>
          )}
        </div>

        {/* Metrics */}
        <div className="card flex flex-col gap-4">
          <h3 className="text-h3 font-semibold text-[var(--text-primary)] flex items-center gap-2">
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
                  <span className="text-body-sm font-medium text-[var(--text-primary)]">
                    {metric.label}
                  </span>
                  <span className="block text-caption text-[var(--text-muted)]">
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
                  {metric.visibility === "public"
                    ? t("challenge.metrics.visibility.public")
                    : t("challenge.metrics.visibility.official")}
                </span>
              </div>
            ))}
          </div>
        </div>

        <ChallengeLivePanels
          challengeName={challengeName}
          defaultTarget={defaultTarget}
          initialLeaderboard={leaderboard}
          initialSubmissions={submissions}
          labels={{
            empty: t("common.empty"),
            hidden: t("submissions.hidden"),
            latestSubmissions: t("challenge.latestSubmissions"),
            topLeaderboard: t("challenge.topLeaderboard"),
            viewAll: t("challenge.viewAll"),
          }}
          leaderboardIsPublic={leaderboardIsPublic}
          locale={locale}
          metricSchema={metricSchema}
          submissionsArePublic={submissionsArePublic}
        />
      </div>
    </div>
  );
}
