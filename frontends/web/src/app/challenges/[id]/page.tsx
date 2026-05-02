import Link from "next/link";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { fetchJson } from "@/lib/api";
import { formatDate } from "@/lib/format";
import {
  formatDeclaredMetric,
  metricDirectionLabel,
  primaryMetric,
} from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  discussionListResponseSchema,
  leaderboardResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Challenge overview page with statement Markdown and recent activity. */
export default async function ChallengePage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, solutionSubmissions, leaderboard, discussions] =
    await Promise.all([
      fetchJson(`/api/public/challenges/${id}`, challengeDetailResponseSchema),
      fetchJson(
        `/api/public/challenges/${id}/solution-submissions`,
        publicSolutionSubmissionListResponseSchema,
      ),
      fetchJson(
        `/api/public/challenges/${id}/leaderboard`,
        leaderboardResponseSchema,
      ),
      fetchJson(
        `/api/public/challenges/${id}/discussions`,
        discussionListResponseSchema,
      ),
    ]);

  const latestSubmissions = solutionSubmissions.items.slice(0, 6);
  const topLeaderboard = leaderboard.items.slice(0, 6);
  const recentDiscussions = discussions.items.slice(0, 4);
  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = detail.spec.metric_schema.metrics.find(
    (metric) =>
      metric.id === detail.spec.metric_schema.ranking.primary_metric_id,
  );

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
              <strong>{detail.spec.solution.language}</strong>
            </div>
            <div>
              <span>格式</span>
              <strong>{detail.spec.solution.format}</strong>
            </div>
            <div>
              <span>入口文件</span>
              <strong>{detail.spec.solution.entrypoint}</strong>
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
              <span>Public 策略</span>
              <strong>{detail.spec.datasets.public_policy}</strong>
            </div>
            <div>
              <span>Private Benchmark 策略</span>
              <strong>{detail.spec.datasets.private_benchmark_policy}</strong>
            </div>
            <div>
              <span>Validation</span>
              <strong>
                {detail.spec.datasets.validation_enabled ? "启用" : "关闭"}
              </strong>
            </div>
            <div>
              <span>Private Benchmark</span>
              <strong>
                {detail.spec.datasets.private_benchmark_enabled
                  ? "启用"
                  : "关闭"}
              </strong>
            </div>
            <div>
              <span>Rank Metric</span>
              <strong>{primaryDefinition?.label ?? "Score"}</strong>
            </div>
          </div>
        </div>

        <div className="workspace-panel">
          <p className="section-kicker">指标</p>
          <div className="dense-list" style={{ marginTop: 8 }}>
            {detail.spec.metric_schema.metrics.map((metric) => (
              <div key={metric.id} className="dense-row">
                <div>
                  <strong>{metric.label}</strong>
                  <small>
                    {metric.id} · {metricDirectionLabel(metric.direction)}
                    {metric.unit ? ` · ${metric.unit}` : ""}
                  </small>
                </div>
                <span>{metric.visibility}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>最新提交</h2>
            <Link
              href={`/challenges/${id}/solution-submissions`}
              className="meta-link"
            >
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
                  href={`/solution-submissions/${s.id}`}
                  className="dense-row"
                >
                  <div>
                    <strong>{s.agent_name}</strong>
                    <small>{formatDate(s.created_at)}</small>
                  </div>
                  <span>
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetric(metricSchema, s.aggregate_metrics),
                    )}
                  </span>
                </Link>
              ))
            )}
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>排行榜</h2>
            <Link href={`/challenges/${id}/leaderboard`} className="meta-link">
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
                  <span>
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetric(metricSchema, entry.aggregate_metrics),
                    )}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="workspace-panel">
          <div className="section-head">
            <h2>讨论</h2>
            <Link href={`/challenges/${id}/discussions`} className="meta-link">
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
