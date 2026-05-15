import { Trophy } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { publicVisibilityAllows } from "@/lib/challengeVisibility";
import { formatDate } from "@/lib/format";
import {
  formatDeclaredMetric,
  metricDirectionLabel,
  metricLabel,
  primaryMetric,
} from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  leaderboardResponseSchema,
} from "@/lib/schemas";

export default async function LeaderboardPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>;
  searchParams: Promise<{ target?: string }>;
}) {
  const { id } = await params;
  const { target } = await searchParams;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);

  const detail = await fetchJson(
    `/api/public/challenges/${id}`,
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
            style={{ fontFamily: "var(--font-serif)" }}
          >
            {t("leaderboard.title")}
          </h2>
          <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] mt-1">
            Select an explicit target.
          </p>
        </div>
        <div className="card flex flex-col gap-4">
          <div className="flex flex-wrap gap-2">
            {detail.spec.targets.map((targetSpec) => (
              <Link
                key={targetSpec.name}
                href={`/challenges/${id}/leaderboard?target=${encodeURIComponent(targetSpec.name)}`}
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
        `/api/public/challenges/${id}/leaderboard?target=${encodeURIComponent(selectedTarget)}&limit=100`,
        leaderboardResponseSchema,
      )
    : {
        challenge_id: detail.id,
        target: selectedTarget,
        items: [],
      };

  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = metricSchema.metrics.find(
    (metric) => metric.id === metricSchema.ranking.primary_metric_id,
  );

  return (
    <div className="flex flex-col gap-6">
      {/* Hero */}
      <div className="card">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div>
            <h2
              className="text-[var(--text-h2)] font-semibold text-[var(--text-primary)]"
              style={{ fontFamily: "var(--font-serif)" }}
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
              href={`/challenges/${id}/leaderboard?target=${encodeURIComponent(targetSpec.name)}`}
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
              Leaderboard is not public.
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
                      {metricDirectionLabel(primaryDefinition.direction)}
                    </span>
                  ) : null}
                </th>
                <th>{t("leaderboard.rankScore")}</th>
                <th className="hidden lg:table-cell">
                  {t("leaderboard.secondaryMetrics")}
                </th>
                <th className="hidden md:table-cell">
                  {t("leaderboard.updatedAt")}
                </th>
                <th>{t("leaderboard.submission")}</th>
              </tr>
            </thead>
            <tbody>
              {leaderboard.items.map((entry, idx) => {
                const primary = primaryMetric(
                  metricSchema,
                  entry.aggregate_metrics,
                );
                const secondary = entry.aggregate_metrics.filter(
                  (metric) =>
                    metric.metric_id !== metricSchema.ranking.primary_metric_id,
                );

                return (
                  <tr key={entry.agent_id}>
                    <td>
                      <span
                        className={`inline-flex items-center justify-center w-7 h-7 rounded-full text-xs font-bold ${
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
                    </td>
                    <td className="font-medium text-[var(--text-primary)]">
                      {entry.agent_name}
                    </td>
                    <td className="font-mono text-[var(--accent-primary-text)]">
                      {formatDeclaredMetric(metricSchema, primary)}
                    </td>
                    <td className="font-mono">{entry.rank_score.toFixed(4)}</td>
                    <td className="hidden lg:table-cell text-[var(--text-muted)] text-[var(--text-caption)]">
                      {secondary.length > 0
                        ? secondary
                            .map(
                              (metric) =>
                                `${metricLabel(metricSchema, metric.metric_id)}: ${formatDeclaredMetric(metricSchema, metric)}`,
                            )
                            .join(" · ")
                        : "—"}
                    </td>
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
