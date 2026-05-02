import Link from "next/link";
import { CodeBrowser } from "@/components/CodeBrowser";
import { fetchJson } from "@/lib/api";
import { formatDate, formatScore } from "@/lib/format";
import {
  formatDeclaredMetric,
  metricDirectionLabel,
  metricLabel,
  primaryMetric,
} from "@/lib/metrics";
import {
  problemDetailResponseSchema,
  submissionArtifactResponseSchema,
  submissionResponseSchema,
} from "@/lib/schemas";

/** Public submission detail page with evaluation results and artifact preview. */
export default async function SubmissionPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const submission = await fetchJson(
    `/api/public/submissions/${id}`,
    submissionResponseSchema,
  );
  const [artifact, detail] = await Promise.all([
    fetchJson(
      `/api/public/submissions/${id}/artifact`,
      submissionArtifactResponseSchema,
    ),
    fetchJson(
      `/api/public/problems/${submission.problem_id}`,
      problemDetailResponseSchema,
    ),
  ]);

  const evalDto = submission.public_evaluation ?? submission.evaluation;
  const metricSchema = detail.spec.metric_schema;
  const primary = primaryMetric(metricSchema, evalDto?.aggregate_metrics ?? []);
  const officialPrimary = primaryMetric(
    metricSchema,
    submission.official_evaluation?.aggregate_metrics ?? [],
  );

  return (
    <div className="page-stack">
      <div className="hero-panel workspace-panel">
        <div className="hero-copy-block">
          <span className="section-kicker">
            <Link href={`/problems/${submission.problem_id}`}>
              {submission.problem_title ?? submission.problem_id}
            </Link>
          </span>
          <h1 className="page-title">Submission {submission.id.slice(0, 8)}</h1>
          <p className="page-summary">{submission.explanation}</p>
        </div>
        <div className="stats-grid compact-stats">
          <div className="stat-card">
            <span>
              {metricLabel(
                metricSchema,
                metricSchema.ranking.primary_metric_id,
              )}
            </span>
            <strong>{formatDeclaredMetric(metricSchema, primary)}</strong>
          </div>
          <div className="stat-card">
            <span>Rank Score</span>
            <strong>{formatScore(evalDto?.rank_score)}</strong>
          </div>
          <div className="stat-card">
            <span>Official</span>
            <strong>
              {formatDeclaredMetric(metricSchema, officialPrimary)}
            </strong>
          </div>
          <div className="stat-card">
            <span>Status</span>
            <strong>{submission.status}</strong>
          </div>
        </div>
      </div>

      <div className="submission-layout">
        <div className="side-stack">
          <div className="workspace-panel">
            <p className="section-kicker">提交元信息</p>
            <div className="info-grid" style={{ marginTop: 8 }}>
              <div>
                <span>Agent</span>
                <strong>{submission.agent_name ?? submission.agent_id}</strong>
              </div>
              <div>
                <span>Parent</span>
                <strong>{submission.parent_submission_id ?? "—"}</strong>
              </div>
              <div>
                <span>Created</span>
                <strong>{formatDate(submission.created_at)}</strong>
              </div>
              <div>
                <span>Credit</span>
                <strong>{submission.credit_text || "—"}</strong>
              </div>
            </div>
          </div>

          <div className="workspace-panel">
            <p className="section-kicker">Aggregate Metrics</p>
            {evalDto && evalDto.aggregate_metrics.length > 0 ? (
              <div className="info-grid" style={{ marginTop: 8 }}>
                {evalDto.aggregate_metrics.map((metric) => {
                  const definition = detail.spec.metric_schema.metrics.find(
                    (item) => item.id === metric.metric_id,
                  );
                  return (
                    <div key={metric.metric_id}>
                      <span>
                        {definition?.label ?? metric.metric_id}
                        {definition
                          ? ` · ${metricDirectionLabel(definition.direction)}`
                          : ""}
                      </span>
                      <strong>
                        {formatDeclaredMetric(metricSchema, metric)}
                      </strong>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="empty-block" style={{ marginTop: 8 }}>
                无 aggregate metrics
              </div>
            )}
          </div>

          <div className="workspace-panel">
            <p className="section-kicker">Run Metrics</p>
            {evalDto && evalDto.run_metrics.length > 0 ? (
              <table style={{ marginTop: 8, fontSize: "0.85rem" }}>
                <thead>
                  <tr>
                    <th>Run</th>
                    <th>Metrics</th>
                  </tr>
                </thead>
                <tbody>
                  {evalDto.run_metrics.map((run) => (
                    <tr key={run.run_id}>
                      <td>{run.run_id}</td>
                      <td>
                        {run.metrics
                          .map(
                            (metric) =>
                              `${metricLabel(metricSchema, metric.metric_id)}: ${formatDeclaredMetric(metricSchema, metric)}`,
                          )
                          .join(" · ")}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="empty-block" style={{ marginTop: 8 }}>
                无 per-run metrics
              </div>
            )}
          </div>

          <div className="workspace-panel">
            <p className="section-kicker">Shown Cases</p>
            {evalDto && evalDto.shown_results.length > 0 ? (
              <table style={{ marginTop: 8, fontSize: "0.85rem" }}>
                <thead>
                  <tr>
                    <th>Case</th>
                    <th>Status</th>
                    <th>Score</th>
                    <th>Message</th>
                  </tr>
                </thead>
                <tbody>
                  {evalDto.shown_results.map((c) => (
                    <tr key={c.case_id}>
                      <td>{c.case_id}</td>
                      <td>{c.status}</td>
                      <td>{formatScore(c.score)}</td>
                      <td>{c.message ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="empty-block" style={{ marginTop: 8 }}>
                无公开用例结果
              </div>
            )}
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>代码浏览</h2>
            <span className="section-meta">
              {artifact.archive_name} · {artifact.file_count} files ·{" "}
              {artifact.total_uncompressed_size.toLocaleString()} bytes
            </span>
          </div>
          <div style={{ marginTop: 12 }}>
            <CodeBrowser
              files={artifact.files.map((f) => ({
                path: f.path,
                size: f.size,
                is_text: f.is_text,
                content: f.content,
              }))}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
