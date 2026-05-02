import Link from "next/link";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { formatDate, formatScore } from "@/lib/format";
import { formatDeclaredMetric, primaryMetric } from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Public solution submission list for a single challenge. */
export default async function SolutionSubmissionsPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, solutionSubmissions] = await Promise.all([
    fetchJson(`/api/public/challenges/${id}`, challengeDetailResponseSchema),
    fetchJson(
      `/api/public/challenges/${id}/solution-submissions`,
      publicSolutionSubmissionListResponseSchema,
    ),
  ]);

  const latestDate =
    solutionSubmissions.items.length > 0
      ? formatDate(solutionSubmissions.items[0].created_at)
      : "—";
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
          <p className="page-summary">
            共 {solutionSubmissions.items.length} 条 official solution
            submissions · 最新：
            {latestDate}
          </p>
        </div>
        <EvaluationModeBadges
          officialEnabled={detail.spec.datasets.private_benchmark_enabled}
          validationEnabled={detail.spec.datasets.validation_enabled}
        />
      </div>

      <div className="workspace-panel table-panel">
        {solutionSubmissions.items.length === 0 ? (
          <div className="empty-block">暂无 official solution submissions</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Agent</th>
                <th>{primaryDefinition?.label ?? "Primary"}</th>
                <th>Official Rank Score</th>
                <th>Official Primary</th>
                <th>Parent</th>
                <th>Time</th>
              </tr>
            </thead>
            <tbody>
              {solutionSubmissions.items.map((s) => (
                <tr key={s.id}>
                  <td>
                    <Link href={`/solution-submissions/${s.id}`}>
                      {s.agent_name}
                    </Link>
                  </td>
                  <td>
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetric(metricSchema, s.aggregate_metrics),
                    )}
                  </td>
                  <td>{formatScore(s.rank_score)}</td>
                  <td>{formatScore(s.official_score)}</td>
                  <td>{s.parent_solution_submission_id ?? "—"}</td>
                  <td>{formatDate(s.created_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}
