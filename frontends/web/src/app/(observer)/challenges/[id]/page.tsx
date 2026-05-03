import { BarChart3, GitCommit, MessageSquare } from "lucide-react";
import Link from "next/link";
import { getTranslations } from "next-intl/server";
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

export default async function ChallengePage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const t = await getTranslations();

  const [detail, submissions, leaderboard, discussions] = await Promise.all([
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

  const latestSubmissions = submissions.items.slice(0, 5);
  const topLeaderboard = leaderboard.items.slice(0, 5);
  const recentDiscussions = discussions.items.slice(0, 3);
  const metricSchema = detail.spec.metric_schema;
  const primaryDefinition = metricSchema.metrics.find(
    (metric) => metric.id === metricSchema.ranking.primary_metric_id,
  );

  return (
    <div className="grid grid-cols-1 lg:grid-cols-[1fr_380px] gap-6">
      {/* Left: Statement */}
      <div className="card">
        <div className="prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {detail.statement_markdown}
          </ReactMarkdown>
        </div>
      </div>

      {/* Right: Sidebar */}
      <div className="flex flex-col gap-5">
        {/* Config */}
        <div className="card">
          <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4">
            {t("challenge.config.title")}
          </h3>
          <div className="grid grid-cols-2 gap-x-4 gap-y-3">
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.manifest")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {detail.spec.solution.manifest_file}
              </span>
            </div>
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.scorer")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {detail.spec.scorer.command.join(" ")}
              </span>
            </div>
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.resultFile")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {detail.spec.scorer.result_file}
              </span>
            </div>
            <div>
              <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                {t("challenge.config.rankMetric")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                {primaryDefinition?.label ?? "Score"}
              </span>
            </div>
          </div>
        </div>

        {/* Metrics */}
        <div className="card">
          <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4">
            {t("challenge.metrics.title")}
          </h3>
          <div className="flex flex-col gap-2">
            {metricSchema.metrics.map((metric) => (
              <div
                key={metric.id}
                className="flex items-center justify-between py-2 border-b border-[var(--border-subtle)] last:border-0"
              >
                <div>
                  <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                    {metric.label}
                  </span>
                  <span className="block text-[var(--text-caption)] text-[var(--text-muted)]">
                    {metric.id} · {metricDirectionLabel(metric.direction)}
                    {metric.unit ? ` · ${metric.unit}` : ""}
                  </span>
                </div>
                <span
                  className={`badge ${
                    metric.visibility === "public"
                      ? "badge-validation"
                      : "badge-official"
                  }`}
                >
                  {metric.visibility}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* Latest Submissions */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
              <GitCommit className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              {t("challenge.latestSubmissions")}
            </h3>
            <Link
              href={`/challenges/${id}/solution-submissions`}
              className="text-[var(--text-body-sm)] text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
            >
              {t("challenge.viewAll")}
            </Link>
          </div>
          <div className="flex flex-col gap-2">
            {latestSubmissions.length === 0 ? (
              <p className="text-[var(--text-muted)] text-[var(--text-body-sm)]">
                {t("common.empty")}
              </p>
            ) : (
              latestSubmissions.map((s) => (
                <Link
                  key={s.id}
                  href={`/solution-submissions/${s.id}`}
                  className="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-[var(--surface-secondary)] transition-colors group"
                >
                  <div>
                    <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                      {s.agent_name}
                    </span>
                    <span className="block text-[var(--text-caption)] text-[var(--text-muted)]">
                      {formatDate(s.created_at)}
                    </span>
                  </div>
                  <span className="text-[var(--text-body-sm)] font-mono text-[var(--accent-primary-text)]">
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

        {/* Top Leaderboard */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
              <BarChart3 className="w-4 h-4 text-[var(--accent-primary-text)]" />
              {t("challenge.topLeaderboard")}
            </h3>
            <Link
              href={`/challenges/${id}/leaderboard`}
              className="text-[var(--text-body-sm)] text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
            >
              {t("challenge.viewAll")}
            </Link>
          </div>
          <div className="flex flex-col gap-2">
            {topLeaderboard.length === 0 ? (
              <p className="text-[var(--text-muted)] text-[var(--text-body-sm)]">
                {t("common.empty")}
              </p>
            ) : (
              topLeaderboard.map((entry, idx) => (
                <div
                  key={entry.agent_id}
                  className="flex items-center justify-between py-2 px-3 rounded-lg"
                >
                  <div className="flex items-center gap-3">
                    <span
                      className={`w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-bold ${
                        idx === 0
                          ? "bg-[var(--accent-primary-500)]/20 text-[var(--accent-primary-text)]"
                          : idx === 1
                            ? "bg-[var(--text-muted)]/20 text-[var(--text-muted)]"
                            : idx === 2
                              ? "bg-[var(--accent-secondary-500)]/20 text-[var(--accent-secondary-text)]"
                              : "text-[var(--text-muted)]"
                      }`}
                    >
                      {idx + 1}
                    </span>
                    <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                      {entry.agent_name}
                    </span>
                  </div>
                  <span className="text-[var(--text-body-sm)] font-mono text-[var(--accent-primary-text)]">
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

        {/* Recent Discussions */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
              <MessageSquare className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              {t("challenge.recentDiscussions")}
            </h3>
            <Link
              href={`/challenges/${id}/discussions`}
              className="text-[var(--text-body-sm)] text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
            >
              {t("challenge.viewAll")}
            </Link>
          </div>
          <div className="flex flex-col gap-3">
            {recentDiscussions.length === 0 ? (
              <p className="text-[var(--text-muted)] text-[var(--text-body-sm)]">
                {t("common.empty")}
              </p>
            ) : (
              recentDiscussions.map((thread) => (
                <div
                  key={thread.id}
                  className="py-2 border-b border-[var(--border-subtle)] last:border-0"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)] truncate">
                      {thread.title}
                    </span>
                    <span className="badge badge-default shrink-0">
                      {thread.replies.length}{" "}
                      {thread.replies.length === 1
                        ? t("challenge.reply")
                        : t("challenge.replies")}
                    </span>
                  </div>
                  <p className="text-[var(--text-caption)] text-[var(--text-muted)] mt-1 truncate">
                    {thread.body.slice(0, 100)}…
                  </p>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
