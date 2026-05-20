import { Trophy } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { RankBadge } from "@/components/RankBadge";
import { fetchJson } from "@/lib/api";
import { publicVisibilityAllows } from "@/lib/challengeVisibility";
import { formatDate } from "@/lib/format";
import {
  formatDeclaredMetric,
  metricDirectionLabel,
  primaryMetricFromScore,
} from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  leaderboardResponseSchema,
} from "@/lib/schemas";

/** Renders the leaderboard page component. */
export default async function LeaderboardPage({
  params,
  searchParams,
}: {
  params: Promise<{ name: string }>;
  searchParams: Promise<{ target?: string }>;
}) {
  const { name } = await params;
  const { target } = await searchParams;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);
  const metricDirectionLabels = {
    maximize: t("challenge.metrics.higherIsBetter"),
    minimize: t("challenge.metrics.lowerIsBetter"),
  };

  const detail = await fetchJson(
    `/api/public/challenges/${name}`,
    challengeDetailResponseSchema,
  );
  const selectedTarget =
    detail.spec.targets.find((candidate) => candidate.name === target)?.name ??
    (detail.spec.targets.length === 1
      ? detail.spec.targets[0].name
      : undefined);
  const selectedTargetSpec = detail.spec.targets.find(
    (candidate) => candidate.name === selectedTarget,
  );
  if (!selectedTarget) {
    return (
      <div className="flex flex-col gap-6">
        <div className="card">
          <h2
            className="text-[var(--text-h2)] font-semibold text-[var(--text-primary)]"
            style={{ fontFamily: "var(--font-sans)" }}
          >
            {t("leaderboard.title")}
          </h2>
          <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] mt-1">
            {t("leaderboard.selectTarget")}
          </p>
        </div>
        <div className="card flex flex-col gap-4">
          <div className="flex flex-wrap gap-2">
            {detail.spec.targets.map((targetSpec) => (
              <Link
                key={targetSpec.name}
                href={`/challenges/${name}/leaderboard?target=${encodeURIComponent(targetSpec.name)}`}
                className="badge badge-default"
              >
                {targetSpec.name}
              </Link>
            ))}
          </div>
        </div>
      </div>
    );
  }
  const leaderboardVisible = publicVisibilityAllows(
    detail.spec.visibility.leaderboard,
    detail.spec,
  );
  const leaderboard = leaderboardVisible
    ? await fetchJson(
        `/api/public/challenges/${name}/leaderboard?target=${encodeURIComponent(selectedTarget)}&limit=100`,
        leaderboardResponseSchema,
      )
    : {
        challenge_name: detail.name,
        target: selectedTarget,
        items: [],
      };

  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = metricSchema.metrics.find(
    (metric) => metric.name === metricSchema.ranking.primary_metric_name,
  );

  return (
    <div className="flex flex-col gap-6">
      {/* Hero */}
      <div className="card">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div>
            <h2
              className="text-[var(--text-h2)] font-semibold text-[var(--text-primary)]"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {t("leaderboard.title")}
            </h2>
            <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] mt-1">
              {leaderboard.items.length} {t("leaderboard.entries")} ·{" "}
              <span className="font-mono">{selectedTarget}</span>
            </p>
          </div>
          <EvaluationModeBadges
            officialEnabled={detail.spec.datasets.private_benchmark_enabled}
            validationEnabled={selectedTargetSpec?.validation_enabled ?? false}
            validationLabel={t("common.validation")}
            officialLabel={t("common.official")}
            enabledLabel={t("common.enabled")}
            disabledLabel={t("common.disabled")}
          />
        </div>
      </div>

      {detail.spec.targets.length > 1 ? (
        <div className="flex flex-wrap gap-2">
          {detail.spec.targets.map((targetSpec) => (
            <Link
              key={targetSpec.name}
              href={`/challenges/${name}/leaderboard?target=${encodeURIComponent(targetSpec.name)}`}
              className={`badge ${
                targetSpec.name === selectedTarget
                  ? "badge-official"
                  : "badge-default"
              }`}
            >
              {targetSpec.name}
            </Link>
          ))}
        </div>
      ) : null}

      {/* Table */}
      <div className="card overflow-x-auto">
        {!leaderboardVisible ? (
          <div className="empty-state py-12">
            <Trophy className="empty-state-icon" />
            <p className="text-[var(--text-muted)]">
              {t("leaderboard.notPublic")}
            </p>
          </div>
        ) : leaderboard.items.length === 0 ? (
          <div className="empty-state py-12">
            <Trophy className="empty-state-icon" />
            <p className="text-[var(--text-muted)]">{t("leaderboard.empty")}</p>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th className="w-16">{t("leaderboard.rank")}</th>
                <th>{t("leaderboard.agent")}</th>
                <th>
                  {primaryDefinition?.label ?? t("leaderboard.primaryMetric")}
                  {primaryDefinition ? (
                    <span className="block text-[10px] normal-case tracking-normal opacity-70">
                      {metricDirectionLabel(
                        primaryDefinition.direction,
                        metricDirectionLabels,
                      )}
                    </span>
                  ) : null}
                </th>
                <th>{t("leaderboard.rankScore")}</th>
                <th className="hidden md:table-cell">
                  {t("leaderboard.updatedAt")}
                </th>
                <th>{t("leaderboard.submission")}</th>
              </tr>
            </thead>
            <tbody>
              {leaderboard.items.map((entry, idx) => {
                const primary = primaryMetricFromScore(
                  metricSchema,
                  entry.official_score,
                );

                return (
                  <tr key={entry.agent_id}>
                    <td>
                      <RankBadge rank={idx + 1} />
                    </td>
                    <td className="font-medium text-[var(--text-primary)]">
                      {entry.agent_display_name}
                    </td>
                    <td className="font-mono text-[var(--accent-primary-text)]">
                      {formatDeclaredMetric(metricSchema, primary)}
                    </td>
                    <td className="font-mono">{entry.rank_score.toFixed(4)}</td>
                    <td className="hidden md:table-cell text-[var(--text-muted)] text-[var(--text-caption)]">
                      {formatDate(entry.updated_at, locale)}
                    </td>
                    <td>
                      <Link
                        href={`/solution-submissions/${entry.best_solution_submission_id}`}
                        className="font-mono text-[var(--text-body-sm)] text-[var(--accent-secondary-text)] hover:text-[var(--accent-secondary-300)] transition-colors"
                      >
                        {entry.best_solution_submission_id.slice(0, 8)}…
                      </Link>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
