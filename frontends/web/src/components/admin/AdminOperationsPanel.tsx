"use client";

import { Ban, Boxes, Play, RefreshCw, Server } from "lucide-react";
import { useTranslations } from "next-intl";
import { useState } from "react";
import { adminErrorMessage } from "@/components/admin/errors";
import { LocalizedStatusBadge } from "@/components/admin/LocalizedStatusBadge";
import { ConsoleSectionTitle as SectionTitle } from "@/components/ConsolePrimitives";
import { adminFetchJson } from "@/lib/adminApi";
import { formatDate, formatScore } from "@/lib/format";
import {
  type AdminServiceHeartbeatListResponse,
  type AdminSolutionSubmissionListItem,
  disableAgentResponseSchema,
  evaluationJobResponseSchema,
} from "@/lib/schemas";
import type { AdminRefresh } from "./AdminPanels";

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
                    <div className="font-mono text-caption text-fg-muted">
                      {submission.id.slice(0, 8)} ·{" "}
                      {submission.agent_display_name}
                    </div>
                    <div className="font-mono text-caption text-fg-muted">
                      {submission.target}
                    </div>
                    <div className="text-caption text-fg-muted max-w-[20rem] truncate">
                      {submission.note || t("noNote")}
                    </div>
                  </td>
                  <td>
                    <LocalizedStatusBadge status={submission.status} />
                  </td>
                  <td>
                    <div className="font-mono text-caption">
                      {submission.latest_job_id?.slice(0, 8) ?? "—"}
                    </div>
                    <div className="text-caption text-fg-muted">
                      {submission.latest_job_eval_type ?? t("noJob")} ·{" "}
                      {submission.latest_job_status ?? "—"}
                    </div>
                  </td>
                  <td className="font-mono">
                    {formatScore(submission.rank_score)}
                  </td>
                  <td className="text-fg-muted">
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
                  <td className="text-fg-muted">
                    {formatDate(heartbeat.last_seen_at, locale)}
                  </td>
                  <td className="font-mono text-caption text-fg-muted">
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
        className="btn btn-ghost btn-sm text-danger"
        onClick={() => runAction("disable-agent")}
        disabled={!csrfToken || pendingAction !== null}
      >
        <Ban className="w-3 h-3" />
        {t("disableAgent")}
      </button>
    </div>
  );
}
