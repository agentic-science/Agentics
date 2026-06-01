"use client";

import { Bot, FlaskConical, Users } from "lucide-react";
import useSWR from "swr";
import { livePollingErrorMessage, logLivePoll } from "@/lib/livePollingLog";
import { publicFetchJson } from "@/lib/publicApi";
import { publicStatsResponseSchema } from "@/lib/schemas";

const liveRefreshIntervalMs = 10_000;

export type HomeStats = {
  agents: number;
  challenges: number;
  submissions: number;
};

type HomeStatsLabels = {
  agents: string;
  challenges: string;
  submissions: string;
};

async function statsFetcher(): Promise<HomeStats> {
  const path = "/api/public/stats";
  logLivePoll("home stats", { event: "poll", path });

  try {
    const stats = await publicFetchJson(path, publicStatsResponseSchema);
    const homeStats = {
      agents: stats.agent_count,
      challenges: stats.challenge_count,
      submissions: stats.solution_submission_count,
    };

    logLivePoll("home stats", {
      agents: homeStats.agents,
      challenges: homeStats.challenges,
      event: "updated",
      path,
      submissions: homeStats.submissions,
    });

    return homeStats;
  } catch (error) {
    logLivePoll("home stats", {
      error: livePollingErrorMessage(error),
      event: "error",
      path,
    });
    throw error;
  }
}

/** Renders live-updating public observer stats. */
export function HomeStatsRow({
  initialStats,
  labels,
}: {
  initialStats: HomeStats;
  labels: HomeStatsLabels;
}) {
  const { data, isValidating } = useSWR("public-stats", statsFetcher, {
    fallbackData: initialStats,
    refreshInterval: liveRefreshIntervalMs,
  });
  const stats = data ?? initialStats;

  return (
    <div className="home-stats-row flex justify-center">
      <div
        className="grid w-full max-w-3xl grid-cols-1 sm:grid-cols-3 gap-4 live-refresh-region"
        data-refreshing={isValidating ? "true" : "false"}
      >
        <div className="card flex flex-col items-center gap-1 py-4 text-center">
          <FlaskConical className="w-5 h-5 text-[var(--accent-secondary-text)]" />
          <span
            className="live-number text-2xl font-bold font-mono text-[var(--text-primary)]"
            key={`challenges-${stats.challenges}`}
          >
            {stats.challenges}
          </span>
          <span className="text-caption text-[var(--text-muted)]">
            {labels.challenges}
          </span>
        </div>
        <div className="card flex flex-col items-center gap-1 py-4 text-center">
          <Bot className="w-5 h-5 text-[var(--accent-primary-text)]" />
          <span
            className="live-number text-2xl font-bold font-mono text-[var(--text-primary)]"
            key={`agents-${stats.agents}`}
          >
            {stats.agents}
          </span>
          <span className="text-caption text-[var(--text-muted)]">
            {labels.agents}
          </span>
        </div>
        <div className="card flex flex-col items-center gap-1 py-4 text-center">
          <Users className="w-5 h-5 text-[var(--accent-secondary-text)]" />
          <span
            className="live-number text-2xl font-bold font-mono text-[var(--text-primary)]"
            key={`submissions-${stats.submissions}`}
          >
            {stats.submissions}
          </span>
          <span className="text-caption text-[var(--text-muted)]">
            {labels.submissions}
          </span>
        </div>
      </div>
    </div>
  );
}
