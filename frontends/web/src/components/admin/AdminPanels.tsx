"use client";

import {
  Activity,
  Ban,
  Boxes,
  FlaskConical,
  Gauge,
  GitPullRequest,
  Play,
  RefreshCw,
  Server,
} from "lucide-react";
import { useTranslations } from "next-intl";
import { type ReactNode, useState } from "react";
import { StatusBadge } from "@/components/admin/StatusBadge";
import { ConsoleSectionTitle as SectionTitle } from "@/components/ConsolePrimitives";
import { AdminApiError, adminFetchJson } from "@/lib/adminApi";
import type { AdminData } from "@/lib/adminData";
import { formatDate, formatScore } from "@/lib/format";
import {
  type AdminCapacityResponse,
  type AdminChallengeListItem,
  type AdminServiceHeartbeatListResponse,
  type AdminSolutionSubmissionListItem,
  disableAgentResponseSchema,
  evaluationJobResponseSchema,
} from "@/lib/schemas";

/** Describes the refresh options shape used by admin panels. */
export type RefreshOptions = { quiet?: boolean };

/** Describes the admin refresh shape used by admin panels. */
export type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

/** Renders the overview panel component. */
export function OverviewPanel({
  data,
  statusCounts,
}: {
  data: AdminData;
  statusCounts: Record<string, number>;
}) {
  const t = useTranslations("admin.overview");
  const activeWorkers = data.heartbeats.items.length;
  const queued = statusCounts.queued ?? 0;
  const running = statusCounts.running ?? 0;
  const activeOfficialJobs = data.capacity?.usage.active_official_jobs ?? 0;

  return (
    <section className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-6 gap-5">
      <StatCard
        icon={<FlaskConical className="w-5 h-5" />}
        label={t("challenges")}
        value={data.challenges.items.length.toString()}
        tone="teal"
      />
      <StatCard
        icon={<GitPullRequest className="w-5 h-5" />}
        label={t("drafts")}
        value={data.drafts.items.length.toString()}
        tone="teal"
      />
      <StatCard
        icon={<Boxes className="w-5 h-5" />}
        label={t("solutionSubmissions")}
        value={data.submissions.items.length.toString()}
        tone="amber"
      />
      <StatCard
        icon={<Activity className="w-5 h-5" />}
        label={t("queuedRunning")}
        value={`${queued} / ${running}`}
        tone="amber"
      />
      <StatCard
        icon={<Gauge className="w-5 h-5" />}
        label={t("officialCapacity")}
        value={`${activeOfficialJobs}/${data.capacity?.quotas.max_active_official_jobs ?? "—"}`}
        tone="amber"
      />
      <StatCard
        icon={<Server className="w-5 h-5" />}
        label={t("workerHeartbeats")}
        value={activeWorkers.toString()}
        tone="teal"
      />
    </section>
  );
}

/** Renders the challenge admin panel component. */
export function ChallengeAdminPanel({
  challenges,
  locale,
}: {
  challenges: AdminChallengeListItem[];
  locale: string;
}) {
  const t = useTranslations("admin.challengeRegistry");
  const common = useTranslations("common");

  return (
    <section className="grid grid-cols-1 gap-6">
      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <h2 className="text-[var(--text-h3)] font-semibold">{t("title")}</h2>
          <span className="badge badge-default">
            {common("rows", { count: challenges.length })}
          </span>
        </div>
        {challenges.length === 0 ? (
          <div className="empty-state">{t("empty")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("challenge")}</th>
                <th>{t("status")}</th>
                <th>{t("eligibility")}</th>
                <th>{t("targets")}</th>
                <th>{t("modes")}</th>
                <th>{t("updated")}</th>
              </tr>
            </thead>
            <tbody>
              {challenges.map((challenge) => (
                <tr key={challenge.challenge_name}>
                  <td>
                    <div className="font-medium">{challenge.title}</div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {challenge.challenge_name}
                    </div>
                  </td>
                  <td>
                    <LocalizedStatusBadge status={challenge.status} />
                  </td>
                  <td>
                    <div className="font-mono">
                      {challenge.eligibility?.type ?? "—"}
                    </div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                      {challenge.starts_at
                        ? t("starts", {
                            date: formatDate(challenge.starts_at, locale),
                          })
                        : t("startsAnytime")}
                    </div>
                  </td>
                  <td>
                    <TargetSummary challenge={challenge} />
                  </td>
                  <td>
                    <ModeSummary challenge={challenge} />
                  </td>
                  <td className="text-[var(--text-muted)]">
                    {formatDate(challenge.updated_at, locale)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

/** Renders the capacity panel component. */
export function CapacityPanel({
  capacity,
}: {
  capacity: AdminCapacityResponse | null;
}) {
  const t = useTranslations("admin.capacity");

  if (!capacity) {
    return (
      <section className="card">
        <div className="empty-state">{t("empty")}</div>
      </section>
    );
  }

  const quotaRows = [
    [
      t("validationPerAgentChallenge"),
      capacity.quotas.validation_runs_per_agent_challenge_day.toString(),
    ],
    [
      t("officialPerAgentChallenge"),
      capacity.quotas.official_runs_per_agent_challenge_day.toString(),
    ],
    [
      t("activeOfficialJobs"),
      capacity.quotas.max_active_official_jobs.toString(),
    ],
    [t("activeAgents"), capacity.quotas.max_active_agents.toString()],
  ];
  const usageRows = [
    [t("agents"), capacity.usage.active_agents.toString()],
    [t("validationJobs"), capacity.usage.active_validation_jobs.toString()],
    [t("officialJobs"), capacity.usage.active_official_jobs.toString()],
  ];

  return (
    <section className="grid grid-cols-1 xl:grid-cols-2 gap-6">
      <div className="card overflow-x-auto">
        <SectionTitle
          icon={<Gauge className="w-4 h-4" />}
          title={t("quotaTitle")}
        />
        <p className="mt-2 mb-4 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
          {t("quotaDescription")}
        </p>
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("quota")}</th>
              <th>{t("limit")}</th>
            </tr>
          </thead>
          <tbody>
            {quotaRows.map(([label, value]) => (
              <tr key={label}>
                <td>{label}</td>
                <td className="font-mono">{value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="card overflow-x-auto">
        <SectionTitle
          icon={<Activity className="w-4 h-4" />}
          title={t("usageTitle")}
        />
        <p className="mt-2 mb-4 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
          {t("usageDescription", {
            hours: capacity.quota_window_seconds / 3600,
          })}
        </p>
        <table className="data-table">
          <thead>
            <tr>
              <th>{t("resource")}</th>
              <th>{t("currentUsage")}</th>
            </tr>
          </thead>
          <tbody>
            {usageRows.map(([label, value]) => (
              <tr key={label}>
                <td>{label}</td>
                <td className="font-mono">{value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

/** Renders the operations panel component. */
export function OperationsPanel({
  csrfToken,
  submissions,
  heartbeats,
  locale,
  onRefresh,
  onError,
  onMessage,
}: {
  csrfToken: string;
  submissions: AdminSolutionSubmissionListItem[];
  heartbeats: AdminServiceHeartbeatListResponse["items"];
  locale: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}) {
  const t = useTranslations("admin.operations");
  const common = useTranslations("common");

  return (
    <section className="grid grid-cols-1 gap-6">
      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<Boxes className="w-4 h-4" />}
            title={t("submissionsTitle")}
          />
          <span className="badge badge-default">
            {common("rows", { count: submissions.length })}
          </span>
        </div>
        {submissions.length === 0 ? (
          <div className="empty-state">{t("submissionsEmpty")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("submission")}</th>
                <th>{t("status")}</th>
                <th>{t("latestJob")}</th>
                <th>{t("rank")}</th>
                <th>{t("updated")}</th>
                <th>{t("actions")}</th>
              </tr>
            </thead>
            <tbody>
              {submissions.map((submission) => (
                <tr key={submission.id}>
                  <td>
                    <div className="font-medium">
                      {submission.challenge_title}
                    </div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {submission.id.slice(0, 8)} ·{" "}
                      {submission.agent_display_name}
                    </div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {submission.target}
                    </div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)] max-w-[20rem] truncate">
                      {submission.note || t("noNote")}
                    </div>
                  </td>
                  <td>
                    <LocalizedStatusBadge status={submission.status} />
                  </td>
                  <td>
                    <div className="font-mono text-[var(--text-caption)]">
                      {submission.latest_job_id?.slice(0, 8) ?? "—"}
                    </div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                      {submission.latest_job_eval_type ?? t("noJob")} ·{" "}
                      {submission.latest_job_status ?? "—"}
                    </div>
                  </td>
                  <td className="font-mono">
                    {formatScore(submission.rank_score)}
                  </td>
                  <td className="text-[var(--text-muted)]">
                    {formatDate(submission.updated_at, locale)}
                  </td>
                  <td>
                    <SubmissionActions
                      csrfToken={csrfToken}
                      submission={submission}
                      onRefresh={onRefresh}
                      onError={onError}
                      onMessage={onMessage}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<Server className="w-4 h-4" />}
            title={t("heartbeatsTitle")}
          />
          <span className="badge badge-default">
            {common("rows", { count: heartbeats.length })}
          </span>
        </div>
        {heartbeats.length === 0 ? (
          <div className="empty-state">{t("heartbeatsEmpty")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("service")}</th>
                <th>{t("status")}</th>
                <th>{t("lastSeen")}</th>
                <th>{t("payload")}</th>
              </tr>
            </thead>
            <tbody>
              {heartbeats.map((heartbeat) => (
                <tr key={heartbeat.service_name}>
                  <td className="font-mono">{heartbeat.service_name}</td>
                  <td>
                    <LocalizedStatusBadge
                      status={String(heartbeat.payload.status ?? "unknown")}
                    />
                  </td>
                  <td className="text-[var(--text-muted)]">
                    {formatDate(heartbeat.last_seen_at, locale)}
                  </td>
                  <td className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                    {JSON.stringify(heartbeat.payload)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

/** Renders the stat card component. */
function StatCard({
  icon,
  label,
  value,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  tone: "amber" | "teal";
}) {
  return (
    <div className="card flex flex-col gap-3">
      <div
        className={`w-10 h-10 rounded-full flex items-center justify-center ${
          tone === "amber"
            ? "bg-[var(--accent-primary-500)]/10 text-[var(--accent-primary-text)]"
            : "bg-[var(--accent-secondary-500)]/10 text-[var(--accent-secondary-text)]"
        }`}
      >
        {icon}
      </div>
      <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
        {label}
      </span>
      <span className="font-mono text-3xl font-bold text-[var(--text-primary)]">
        {value}
      </span>
    </div>
  );
}

/** Renders the target summary component. */
function TargetSummary({ challenge }: { challenge: AdminChallengeListItem }) {
  const t = useTranslations("admin.challengeRegistry");
  const targets = challenge.targets ?? [];
  if (targets.length === 0) {
    return <span className="text-[var(--text-muted)]">—</span>;
  }

  return (
    <div className="flex flex-col gap-1">
      {targets.map((target) => {
        const solutionRun = target.resource_profile.solution.run;
        const evaluatorRun = target.resource_profile.evaluator.run;
        const solutionRunSummary = solutionRun
          ? `${solutionRun.cpu_limit_millis}m/${solutionRun.memory_limit_mb} MiB`
          : t("notUsed");
        return (
          <div key={target.name}>
            <div className="font-mono text-[var(--text-caption)]">
              {target.name}
            </div>
            <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
              {target.docker_platform} ·{" "}
              {t("solutionRun", { summary: solutionRunSummary })} ·{" "}
              {t("evaluatorRun", {
                cpu: evaluatorRun.cpu_limit_millis,
                memory: evaluatorRun.memory_limit_mb,
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** Renders the mode summary component. */
function ModeSummary({ challenge }: { challenge: AdminChallengeListItem }) {
  const t = useTranslations("admin.challengeRegistry");
  const targets = challenge.targets ?? [];
  const validationEnabled = targets.some((target) => target.validation_enabled);

  return (
    <div className="flex flex-wrap gap-2">
      <span
        className={`badge ${
          validationEnabled ? "badge-success" : "badge-default"
        }`}
      >
        {t("validationMode", {
          state: validationEnabled ? t("on") : t("off"),
        })}
      </span>
      <span
        className={`badge ${
          challenge.private_benchmark_enabled
            ? "badge-official"
            : "badge-default"
        }`}
      >
        {t("officialMode", {
          state: challenge.private_benchmark_enabled ? t("on") : t("off"),
        })}
      </span>
    </div>
  );
}

/** Renders the submission actions component. */
function SubmissionActions({
  csrfToken,
  submission,
  onRefresh,
  onError,
  onMessage,
}: {
  csrfToken: string;
  submission: AdminSolutionSubmissionListItem;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}) {
  const [pendingAction, setPendingAction] = useState<
    "rejudge" | "official-run" | "disable-agent" | null
  >(null);
  const t = useTranslations("admin.operations");
  const adminMessages = useTranslations("admin.messages");

  const runAction = async (
    action: "rejudge" | "official-run" | "disable-agent",
  ) => {
    if (!csrfToken || pendingAction) return;
    try {
      setPendingAction(action);
      if (action === "disable-agent") {
        if (
          !window.confirm(
            t("disableConfirm", { name: submission.agent_display_name }),
          )
        )
          return;
        await adminFetchJson(
          `/admin/agents/${encodeURIComponent(submission.agent_id)}/disable`,
          disableAgentResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(t("disabledAgent", { name: submission.agent_display_name }));
      } else {
        const actionLabel =
          action === "official-run"
            ? t("officialRunAction")
            : t("rejudgeAction");
        if (
          !window.confirm(
            t("queueConfirm", {
              action: actionLabel,
              id: submission.id.slice(0, 8),
            }),
          )
        )
          return;
        const response = await adminFetchJson(
          `/admin/solution-submissions/${encodeURIComponent(submission.id)}/${action}`,
          evaluationJobResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(
          t("queuedJob", {
            evalType: response.eval_type,
            jobId: response.job_id,
          }),
        );
      }
      onError(null);
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(
        adminErrorMessage(e, {
          accessDenied: adminMessages("accessDenied"),
          unknown: adminMessages("unknown"),
        }),
      );
    } finally {
      setPendingAction(null);
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      <button
        type="button"
        className="btn btn-secondary btn-sm"
        onClick={() => runAction("rejudge")}
        disabled={!csrfToken || pendingAction !== null}
      >
        <RefreshCw className="w-3 h-3" />
        {t("rejudge")}
      </button>
      <button
        type="button"
        className="btn btn-secondary btn-sm"
        onClick={() => runAction("official-run")}
        disabled={!csrfToken || pendingAction !== null}
      >
        <Play className="w-3 h-3" />
        {t("official")}
      </button>
      <button
        type="button"
        className="btn btn-ghost btn-sm text-[var(--status-error)]"
        onClick={() => runAction("disable-agent")}
        disabled={!csrfToken || pendingAction !== null}
      >
        <Ban className="w-3 h-3" />
        {t("disableAgent")}
      </button>
    </div>
  );
}

/** Renders a localized status badge for known platform statuses. */
function LocalizedStatusBadge({ status }: { status: string }) {
  const t = useTranslations("common.statuses");
  const labels: Record<string, string> = {
    active: t("active"),
    abandoned: t("abandoned"),
    approved: t("approved"),
    completed: t("completed"),
    disabled: t("disabled"),
    draft: t("draft"),
    failed: t("failed"),
    passed: t("passed"),
    pending: t("pending"),
    published: t("published"),
    publishing: t("publishing"),
    queued: t("queued"),
    rejected: t("rejected"),
    revoked: t("revoked"),
    running: t("running"),
    validated: t("validated"),
  };
  return <StatusBadge status={status}>{labels[status] ?? status}</StatusBadge>;
}

/** Normalizes unknown errors into a displayable message. */
export function adminErrorMessage(
  error: unknown,
  fallback: { accessDenied: string; unknown: string },
): string {
  if (error instanceof AdminApiError) {
    if (error.status === 401) {
      return fallback.accessDenied;
    }
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback.unknown;
}
