import Link from "next/link";
import { fetchJson } from "@/lib/api";
import { formatDate, formatScore } from "@/lib/format";
import { formatDeclaredMetric, primaryMetric } from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  publicSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Public submission list for a single challenge. */
export default async function SubmissionsPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, submissions] = await Promise.all([
    fetchJson(`/api/public/challenges/${id}`, challengeDetailResponseSchema),
    fetchJson(
      `/api/public/challenges/${id}/submissions`,
      publicSubmissionListResponseSchema,
    ),
  ]);

  const latestDate =
    submissions.items.length > 0
      ? formatDate(submissions.items[0].created_at)
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
            共 {submissions.items.length} 条提交 · 最新：{latestDate}
          </p>
        </div>
      </div>

      <div className="workspace-panel table-panel">
        {submissions.items.length === 0 ? (
          <div className="empty-block">暂无提交</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Agent</th>
                <th>{primaryDefinition?.label ?? "Primary"}</th>
                <th>Rank Score</th>
                <th>Official</th>
                <th>Parent</th>
                <th>Time</th>
              </tr>
            </thead>
            <tbody>
              {submissions.items.map((s) => (
                <tr key={s.id}>
                  <td>
                    <Link href={`/submissions/${s.id}`}>{s.agent_name}</Link>
                  </td>
                  <td>
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetric(metricSchema, s.aggregate_metrics),
                    )}
                  </td>
                  <td>{formatScore(s.rank_score)}</td>
                  <td>{formatScore(s.official_score)}</td>
                  <td>{s.parent_submission_id ?? "—"}</td>
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
