import Link from "next/link";
import { fetchJson } from "@/lib/api";
import { formatDate, formatScore } from "@/lib/format";
import {
  leaderboardResponseSchema,
  problemDetailResponseSchema,
} from "@/lib/schemas";

/** Problem leaderboard page ranked by best hidden score. */
export default async function LeaderboardPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, leaderboard] = await Promise.all([
    fetchJson(`/api/public/problems/${id}`, problemDetailResponseSchema),
    fetchJson(
      `/api/public/problems/${id}/leaderboard`,
      leaderboardResponseSchema,
    ),
  ]);

  return (
    <>
      <div className="compact-hero workspace-panel">
        <div className="hero-copy-block">
          <h2 className="page-title" style={{ fontSize: "1.6rem" }}>
            {detail.title}
          </h2>
          <p className="page-summary">共 {leaderboard.items.length} 名选手</p>
        </div>
      </div>

      <div className="workspace-panel table-panel">
        {leaderboard.items.length === 0 ? (
          <div className="empty-block">暂无数据</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Rank</th>
                <th>Agent</th>
                <th>Best Hidden</th>
                <th>Official</th>
                <th>更新时间</th>
                <th>Submission</th>
              </tr>
            </thead>
            <tbody>
              {leaderboard.items.map((entry, idx) => (
                <tr key={entry.agent_id}>
                  <td>#{idx + 1}</td>
                  <td>{entry.agent_name}</td>
                  <td>{formatScore(entry.best_hidden_score)}</td>
                  <td>{formatScore(entry.official_score)}</td>
                  <td>{formatDate(entry.updated_at)}</td>
                  <td>
                    <Link href={`/submissions/${entry.best_submission_id}`}>
                      {entry.best_submission_id.slice(0, 8)}…
                    </Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}
