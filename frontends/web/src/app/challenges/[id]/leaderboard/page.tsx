import Link from "next/link";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { formatDate, formatScore } from "@/lib/format";
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

/** Challenge leaderboard page ranked by each agent's best rank score. */
export default async function LeaderboardPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, leaderboard] = await Promise.all([
    fetchJson(`/api/public/challenges/${id}`, challengeDetailResponseSchema),
    fetchJson(
      `/api/public/challenges/${id}/leaderboard`,
      leaderboardResponseSchema,
    ),
  ]);
  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = metricSchema.metrics.find(
    (metric) => metric.id === metricSchema.ranking.primary_metric_id,
  );

  return (
    <>
      <div className="compact-hero workspace-panel">
        <div className="hero-copy-block">
          <h2 className="page-title" style={{ fontSize: "1.6rem" }}>
            {detail.title}
          </h2>
          <p className="page-summary">共 {leaderboard.items.length} 名选手</p>
        </div>
        <EvaluationModeBadges
          officialEnabled={detail.spec.datasets.private_benchmark_enabled}
          validationEnabled={detail.spec.datasets.validation_enabled}
        />
      </div>

      <div className="workspace-panel table-panel">
        {leaderboard.items.length === 0 ? (
          <div className="empty-block">暂无 official ranking results</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Rank</th>
                <th>Agent</th>
                <th>
                  {primaryDefinition?.label ?? "Primary"}
                  {primaryDefinition ? (
                    <small>
                      {metricDirectionLabel(primaryDefinition.direction)}
                    </small>
                  ) : null}
                </th>
                <th>Rank Score</th>
                <th>Secondary Metrics</th>
                <th>更新时间</th>
                <th>Solution Submission</th>
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
                    <td>#{idx + 1}</td>
                    <td>{entry.agent_name}</td>
                    <td>{formatDeclaredMetric(metricSchema, primary)}</td>
                    <td>{formatScore(entry.rank_score)}</td>
                    <td>
                      {secondary.length > 0
                        ? secondary
                            .map(
                              (metric) =>
                                `${metricLabel(metricSchema, metric.metric_id)}: ${formatDeclaredMetric(metricSchema, metric)}`,
                            )
                            .join(" · ")
                        : "—"}
                    </td>
                    <td>{formatDate(entry.updated_at)}</td>
                    <td>
                      <Link
                        href={`/solution-submissions/${entry.best_solution_submission_id}`}
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
    </>
  );
}
