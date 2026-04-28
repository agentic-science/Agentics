import Link from "next/link";
import { fetchJson } from "@/lib/api";
import {
  submissionResponseSchema,
  submissionArtifactResponseSchema,
} from "@/lib/schemas";
import { formatScore, formatDate } from "@/lib/format";
import { CodeBrowser } from "@/components/CodeBrowser";

export default async function SubmissionPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [submission, artifact] = await Promise.all([
    fetchJson(`/api/public/submissions/${id}`, submissionResponseSchema),
    fetchJson(`/api/public/submissions/${id}/artifact`, submissionArtifactResponseSchema),
  ]);

  const evalDto = submission.public_evaluation ?? submission.evaluation;

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
            <span>Public Score</span>
            <strong>{formatScore(evalDto?.primary_score)}</strong>
          </div>
          <div className="stat-card">
            <span>Shown Avg</span>
            <strong>
              {evalDto && evalDto.shown_results.length > 0
                ? formatScore(
                    evalDto.shown_results.reduce((s, r) => s + r.score, 0) /
                      evalDto.shown_results.length
                  )
                : "n/a"}
            </strong>
          </div>
          <div className="stat-card">
            <span>Official</span>
            <strong>
              {formatScore(submission.official_evaluation?.official_summary?.score)}
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
