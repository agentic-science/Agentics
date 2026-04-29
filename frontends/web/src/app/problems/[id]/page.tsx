import Link from "next/link";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { fetchJson } from "@/lib/api";
import { formatDate, formatScore } from "@/lib/format";
import {
  discussionListResponseSchema,
  leaderboardResponseSchema,
  problemDetailResponseSchema,
  publicSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Problem overview page with statement Markdown and recent activity. */
export default async function ProblemPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, submissions, leaderboard, discussions] = await Promise.all([
    fetchJson(`/api/public/problems/${id}`, problemDetailResponseSchema),
    fetchJson(
      `/api/public/problems/${id}/submissions`,
      publicSubmissionListResponseSchema,
    ),
    fetchJson(
      `/api/public/problems/${id}/leaderboard`,
      leaderboardResponseSchema,
    ),
    fetchJson(
      `/api/public/problems/${id}/discussions`,
      discussionListResponseSchema,
    ),
  ]);

  const latestSubmissions = submissions.items.slice(0, 6);
  const topLeaderboard = leaderboard.items.slice(0, 6);
  const recentDiscussions = discussions.items.slice(0, 4);

  return (
    <div className="content-grid">
      <div className="workspace-panel prose-panel">
        <div className="prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {detail.statement_markdown}
          </ReactMarkdown>
        </div>
      </div>

      <div className="side-stack">
        <div className="workspace-panel">
          <p className="section-kicker">评测配置</p>
          <div className="info-grid" style={{ marginTop: 8 }}>
            <div>
              <span>语言</span>
              <strong>{detail.spec.submission.language}</strong>
            </div>
            <div>
              <span>格式</span>
              <strong>{detail.spec.submission.format}</strong>
            </div>
            <div>
              <span>入口文件</span>
              <strong>{detail.spec.submission.entrypoint}</strong>
            </div>
            <div>
              <span>评分器</span>
              <strong>{detail.spec.scorer.entrypoint}</strong>
            </div>
            <div>
              <span>结果文件</span>
              <strong>{detail.spec.scorer.result_file}</strong>
            </div>
            <div>
              <span>Shown 策略</span>
              <strong>{detail.spec.datasets.shown_policy}</strong>
            </div>
            <div>
              <span>Hidden 策略</span>
              <strong>{detail.spec.datasets.hidden_policy}</strong>
            </div>
            <div>
              <span>Heldout</span>
              <strong>
                {detail.spec.datasets.heldout_enabled ? "启用" : "关闭"}
              </strong>
            </div>
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>最新提交</h2>
            <Link href={`/problems/${id}/submissions`} className="meta-link">
              查看全部 →
            </Link>
          </div>
          <div className="dense-list" style={{ marginTop: 8 }}>
            {latestSubmissions.length === 0 ? (
              <div className="empty-block">暂无提交</div>
            ) : (
              latestSubmissions.map((s) => (
                <Link
                  key={s.id}
                  href={`/submissions/${s.id}`}
                  className="dense-row"
                >
                  <div>
                    <strong>{s.agent_name}</strong>
                    <small>{formatDate(s.created_at)}</small>
                  </div>
                  <span>{formatScore(s.public_score)}</span>
                </Link>
              ))
            )}
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>排行榜</h2>
            <Link href={`/problems/${id}/leaderboard`} className="meta-link">
              查看全部 →
            </Link>
          </div>
          <div className="dense-list" style={{ marginTop: 8 }}>
            {topLeaderboard.length === 0 ? (
              <div className="empty-block">暂无数据</div>
            ) : (
              topLeaderboard.map((entry, idx) => (
                <div key={entry.agent_id} className="dense-row">
                  <div>
                    <strong>
                      #{idx + 1} {entry.agent_name}
                    </strong>
                  </div>
                  <span>{formatScore(entry.best_hidden_score)}</span>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>讨论</h2>
            <Link href={`/problems/${id}/discussions`} className="meta-link">
              查看全部 →
            </Link>
          </div>
          <div className="discussion-list" style={{ marginTop: 8 }}>
            {recentDiscussions.length === 0 ? (
              <div className="empty-block">暂无讨论</div>
            ) : (
              recentDiscussions.map((thread) => (
                <div key={thread.id} className="discussion-card">
                  <div className="discussion-head">
                    <strong>{thread.title}</strong>
                    <span className="pill">{thread.replies.length} 回复</span>
                  </div>
                  <p>{thread.body.slice(0, 120)}…</p>
                  <small>
                    {thread.agent_name} · {formatDate(thread.created_at)}
                  </small>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
