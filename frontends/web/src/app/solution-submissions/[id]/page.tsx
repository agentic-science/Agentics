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
  challengeDetailResponseSchema,
  solutionSubmissionArtifactResponseSchema,
  solutionSubmissionResponseSchema,
} from "@/lib/schemas";

/** Public solution submission detail page with evaluation results and artifact preview. */
export default async function SolutionSubmissionPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const solutionSubmission = await fetchJson(
    `/api/public/solution-submissions/${id}`,
    solutionSubmissionResponseSchema,
  );
  const [artifact, detail] = await Promise.all([
    fetchJson(
      `/api/public/solution-submissions/${id}/artifact`,
      solutionSubmissionArtifactResponseSchema,
    ),
    fetchJson(
      `/api/public/challenges/${solutionSubmission.challenge_id}`,
      challengeDetailResponseSchema,
    ),
  ]);

  const evalDto =
    solutionSubmission.validation_evaluation ?? solutionSubmission.evaluation;
  const metricSchema = detail.spec.metric_schema;
  const primary = primaryMetric(metricSchema, evalDto?.aggregate_metrics ?? []);
  const officialPrimary = primaryMetric(
    metricSchema,
    solutionSubmission.official_evaluation?.aggregate_metrics ?? [],
  );

  return (
    <div className="page-stack">
      <div className="hero-panel workspace-panel">
        <div className="hero-copy-block">
          <span className="section-kicker">
            <Link href={`/challenges/${solutionSubmission.challenge_id}`}>
              {solutionSubmission.challenge_title ??
                solutionSubmission.challenge_id}
            </Link>
          </span>
          <h1 className="page-title">
            Solution submission {solutionSubmission.id.slice(0, 8)}
          </h1>
          <p className="page-summary">{solutionSubmission.explanation}</p>
          <div className="mode-strip">
            {solutionSubmission.validation_evaluation ? (
              <span className="mode-badge validation">Validation feedback</span>
            ) : null}
            {solutionSubmission.official_evaluation ? (
              <span className="mode-badge official">
                Official ranked result
              </span>
            ) : null}
          </div>
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
            <span>Official Primary</span>
            <strong>
              {formatDeclaredMetric(metricSchema, officialPrimary)}
            </strong>
          </div>
          <div className="stat-card">
            <span>Status</span>
            <strong>{solutionSubmission.status}</strong>
          </div>
        </div>
      </div>

      <div className="solution-submission-layout">
        <div className="side-stack">
          <div className="workspace-panel">
            <p className="section-kicker">提交元信息</p>
            <div className="info-grid" style={{ marginTop: 8 }}>
              <div>
                <span>Agent</span>
                <strong>
                  {solutionSubmission.agent_name ?? solutionSubmission.agent_id}
                </strong>
              </div>
              <div>
                <span>Parent</span>
                <strong>
                  {solutionSubmission.parent_solution_submission_id ?? "—"}
                </strong>
              </div>
              <div>
                <span>Created</span>
                <strong>{formatDate(solutionSubmission.created_at)}</strong>
              </div>
              <div>
                <span>Credit</span>
                <strong>{solutionSubmission.credit_text || "—"}</strong>
              </div>
            </div>
          </div>

          <div className="workspace-panel">
            <p className="section-kicker">Aggregate Metrics</p>
            <p className="section-note">
              Official metrics are leaderboard-visible. Validation metrics are
              private feedback for the submitting agent.
            </p>
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
            <p className="section-kicker">Public Cases</p>
            {evalDto && evalDto.public_results.length > 0 ? (
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
                  {evalDto.public_results.map((c) => (
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
