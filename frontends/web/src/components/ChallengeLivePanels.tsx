"use client";

import { BarChart3, GitCommit } from "lucide-react";
import Link from "next/link";
import { RankBadge } from "@/components/RankBadge";
import { formatDate } from "@/lib/format";
import { formatDeclaredMetric, type MetricSchema } from "@/lib/metrics";
import {
  type LeaderboardResponse,
  leaderboardResponseSchema,
  type PublicSolutionSubmissionListResponse,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";
import { usePublicLiveJson } from "@/lib/usePublicLiveJson";

type ChallengeLivePanelLabels = {
  empty: string;
  hidden: string;
  latestSubmissions: string;
  topLeaderboard: string;
  viewAll: string;
};

function submissionsSignature(
  submissions: PublicSolutionSubmissionListResponse,
) {
  return submissions.items
    .map((submission) => `${submission.id}:${submission.status}`)
    .join(":");
}

function leaderboardSignature(leaderboard: LeaderboardResponse) {
  return leaderboard.items
    .map(
      (entry) =>
        `${entry.agent_id}:${entry.best_solution_submission_id}:${entry.official_primary_metric?.value ?? "none"}`,
    )
    .join(":");
}

/** Renders live-updating latest submissions and top leaderboard panels. */
export function ChallengeLivePanels({
  challengeName,
  defaultTarget,
  initialLeaderboard,
  initialSubmissions,
  labels,
  leaderboardIsPublic,
  locale,
  metricSchema,
  submissionsArePublic,
}: {
  challengeName: string;
  defaultTarget: string;
  initialLeaderboard: LeaderboardResponse;
  initialSubmissions: PublicSolutionSubmissionListResponse;
  labels: ChallengeLivePanelLabels;
  leaderboardIsPublic: boolean;
  locale: string;
  metricSchema: MetricSchema;
  submissionsArePublic: boolean;
}) {
  const encodedTarget = encodeURIComponent(defaultTarget);
  const submissionsPath = `/api/public/challenges/${encodeURIComponent(
    challengeName,
  )}/solution-submissions?target=${encodedTarget}&limit=5`;
  const leaderboardPath = `/api/public/challenges/${encodeURIComponent(
    challengeName,
  )}/leaderboard?target=${encodedTarget}&limit=5`;
  const { data: submissionsData, isValidating: submissionsRefreshing } =
    usePublicLiveJson({
      enabled: submissionsArePublic,
      fallbackData: initialSubmissions,
      path: submissionsPath,
      schema: publicSolutionSubmissionListResponseSchema,
      surface: "challenge submissions",
      updatedDetails: (submissions) => ({
        items: submissions.items.length,
        total: submissions.total_count,
      }),
    });
  const { data: leaderboardData, isValidating: leaderboardRefreshing } =
    usePublicLiveJson({
      enabled: leaderboardIsPublic,
      fallbackData: initialLeaderboard,
      path: leaderboardPath,
      schema: leaderboardResponseSchema,
      surface: "challenge leaderboard",
      updatedDetails: (leaderboard) => ({
        items: leaderboard.items.length,
      }),
    });
  const submissions = submissionsData ?? initialSubmissions;
  const leaderboard = leaderboardData ?? initialLeaderboard;

  return (
    <>
      <div
        className="card flex flex-col gap-5 live-refresh-region"
        data-refreshing={submissionsRefreshing ? "true" : "false"}
      >
        <div className="flex items-center justify-between">
          <h3 className="text-h3 font-semibold text-[var(--text-primary)] flex items-center gap-2">
            <GitCommit className="w-4 h-4 text-[var(--accent-secondary-text)]" />
            {labels.latestSubmissions}
          </h3>
          {submissionsArePublic ? (
            <Link
              href={`/challenges/${challengeName}/solution-submissions?target=${encodedTarget}`}
              className="text-body-sm text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
            >
              {labels.viewAll}
            </Link>
          ) : (
            <span className="text-body-sm text-[var(--text-muted)]">
              {labels.hidden}
            </span>
          )}
        </div>
        <div
          className="flex flex-col gap-2 live-refresh-frame"
          key={submissionsSignature(submissions)}
        >
          {submissions.items.length === 0 ? (
            <p className="text-[var(--text-muted)] text-body-sm">
              {labels.empty}
            </p>
          ) : (
            submissions.items.map((submission) => (
              <Link
                key={submission.id}
                href={`/solution-submissions/${submission.id}`}
                className="live-refresh-row flex items-center justify-between py-2 px-3 rounded-dialog hover:bg-[var(--surface-secondary)] transition-colors group"
              >
                <div>
                  <span className="text-body-sm font-medium text-[var(--text-primary)]">
                    {submission.agent_display_name}
                  </span>
                  <span className="block text-caption text-[var(--text-muted)]">
                    {submission.target} ·{" "}
                    {formatDate(submission.created_at, locale)}
                  </span>
                </div>
                <span className="text-body-sm font-mono text-[var(--accent-primary-text)]">
                  {formatDeclaredMetric(
                    metricSchema,
                    submission.official_primary_metric,
                  )}
                </span>
              </Link>
            ))
          )}
        </div>
      </div>

      <div
        className="card flex flex-col gap-5 live-refresh-region"
        data-refreshing={leaderboardRefreshing ? "true" : "false"}
      >
        <div className="flex items-center justify-between">
          <h3 className="text-h3 font-semibold text-[var(--text-primary)] flex items-center gap-2">
            <BarChart3 className="w-4 h-4 text-[var(--accent-primary-text)]" />
            {labels.topLeaderboard}
          </h3>
          {leaderboardIsPublic ? (
            <Link
              href={`/challenges/${challengeName}/leaderboard?target=${encodedTarget}`}
              className="text-body-sm text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors"
            >
              {labels.viewAll}
            </Link>
          ) : (
            <span className="text-body-sm text-[var(--text-muted)]">
              {labels.hidden}
            </span>
          )}
        </div>
        <div
          className="flex flex-col gap-2 live-refresh-frame"
          key={leaderboardSignature(leaderboard)}
        >
          {!leaderboardIsPublic ? (
            <p className="text-[var(--text-muted)] text-body-sm">
              {labels.hidden}
            </p>
          ) : leaderboard.items.length === 0 ? (
            <p className="text-[var(--text-muted)] text-body-sm">
              {labels.empty}
            </p>
          ) : (
            leaderboard.items.map((entry, index) => (
              <div
                key={entry.agent_id}
                className="live-refresh-row flex items-center justify-between py-2 px-3 rounded-dialog"
              >
                <div className="flex items-center gap-3">
                  <RankBadge rank={index + 1} size="sm" />
                  <span className="text-body-sm font-medium text-[var(--text-primary)]">
                    {entry.agent_display_name}
                  </span>
                </div>
                <span className="text-body-sm font-mono text-[var(--accent-primary-text)]">
                  {formatDeclaredMetric(
                    metricSchema,
                    entry.official_primary_metric,
                  )}
                </span>
              </div>
            ))
          )}
        </div>
      </div>
    </>
  );
}
