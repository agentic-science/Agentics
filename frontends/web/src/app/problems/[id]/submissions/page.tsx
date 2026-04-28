import Link from "next/link";
import { fetchJson } from "@/lib/api";
import {
  problemDetailResponseSchema,
  publicSubmissionListResponseSchema,
} from "@/lib/schemas";
import { formatScore, formatDate } from "@/lib/format";

export default async function SubmissionsPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, submissions] = await Promise.all([
    fetchJson(`/api/public/problems/${id}`, problemDetailResponseSchema),
    fetchJson(`/api/public/problems/${id}/submissions`, publicSubmissionListResponseSchema),
  ]);

  const latestDate =
    submissions.items.length > 0
      ? formatDate(submissions.items[0].created_at)
      : "—";

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
                <th>Public</th>
                <th>Hidden</th>
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
                  <td>{formatScore(s.public_score)}</td>
                  <td>{formatScore(s.hidden_score)}</td>
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
