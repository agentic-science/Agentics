"use client";

import {
  BarChart3,
  FileArchive,
  GitPullRequest,
  KeyRound,
  ListPlus,
  RefreshCw,
  Users,
} from "lucide-react";
import { useTranslations } from "next-intl";
import { ConsoleSectionTitle as SectionTitle } from "@/components/ConsolePrimitives";
import { StatusBadge } from "@/components/StatusBadge";
import { selectLocalizedText } from "@/lib/localizedText";
import type {
  ChallengeShortlistResponse,
  ChallengeShortlistRevisionResponse,
  CreatorChallengeDraftResponse,
  CreatorChallengeParticipantsResponse,
  CreatorChallengeStatsResponse,
  CreatorMeResponse,
} from "@/lib/schemas";

/** Renders the creator identity panel component. */
export function CreatorIdentityPanel({
  creator,
  loading,
  pioneerCode,
  onPioneerCodeChange,
  onSignIn,
  onRefresh,
}: {
  creator: CreatorMeResponse | null;
  loading: boolean;
  pioneerCode: string;
  onPioneerCodeChange: (value: string) => void;
  onSignIn: () => Promise<void>;
  onRefresh: () => Promise<void>;
}) {
  const t = useTranslations("creator.identity");

  return (
    <div className="card min-w-full lg:min-w-[360px] lg:max-w-[420px]">
      <div className="flex items-center gap-2 mb-4">
        <KeyRound className="w-4 h-4 text-[var(--accent-primary-text)]" />
        <h2 className="text-[var(--text-h3)] font-semibold">{t("title")}</h2>
      </div>
      {creator ? (
        <div className="space-y-3">
          <div>
            <div className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
              {t("githubAccount")}
            </div>
            <div className="font-mono text-[var(--text-body-sm)]">
              {creator.github_login} · {creator.github_user_id}
            </div>
          </div>
          <div>
            <div className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
              {t("agentId")}
            </div>
            <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)] break-all">
              {creator.agent_id}
            </div>
          </div>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => void onRefresh()}
            disabled={loading}
          >
            <RefreshCw className="w-4 h-4" />
            {t("refresh")}
          </button>
        </div>
      ) : (
        <div className="space-y-4">
          <p className="text-[var(--text-body-sm)] text-[var(--text-secondary)]">
            {t("oauthRequired")}
          </p>
          <label className="flex flex-col gap-1">
            <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
              {t("pioneerCode")}
            </span>
            <input
              className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
              value={pioneerCode}
              onChange={(event) => onPioneerCodeChange(event.target.value)}
              autoComplete="off"
            />
          </label>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void onSignIn()}
            disabled={loading}
          >
            <GitPullRequest className="w-4 h-4" />
            {t("signIn")}
          </button>
        </div>
      )}
    </div>
  );
}

/** Renders the draft detail component. */
export function DraftDetail({
  draft,
}: {
  draft: CreatorChallengeDraftResponse | null;
}) {
  const t = useTranslations("creator.draft");
  const common = useTranslations("common");

  if (!draft) {
    return (
      <div className="card">
        <div className="empty-state">{t("empty")}</div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="card">
        <div className="flex flex-col md:flex-row md:items-start justify-between gap-4">
          <div>
            <div className="flex flex-wrap items-center gap-2 mb-3">
              <LocalizedStatusBadge status={draft.status} />
              <span className="badge badge-default">{draft.request}</span>
            </div>
            <h2 className="text-[var(--text-h2)] font-semibold">
              {draft.manifest.title}
            </h2>
            <p className="mt-2 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
              {selectLocalizedText(
                draft.manifest.summary,
                currentDocumentLocale(),
              )}
            </p>
          </div>
          <a
            href={draft.pr_url}
            target="_blank"
            rel="noreferrer"
            className="btn btn-secondary"
          >
            <GitPullRequest className="w-4 h-4" />
            {t("openPr")}
          </a>
        </div>

        <dl className="mt-6 grid grid-cols-1 md:grid-cols-2 gap-4 text-[var(--text-body-sm)]">
          <Metadata label={t("draftId")} value={draft.id} />
          <Metadata label={t("challengeName")} value={draft.challenge_name} />
          <Metadata label={t("creator")} value={draft.creator_github_login} />
          <Metadata label={t("commit")} value={shortHash(draft.commit_sha)} />
          <Metadata
            label={t("manifestHash")}
            value={shortHash(draft.manifest_sha256)}
          />
          <Metadata
            label={t("validationBundle")}
            value={shortHash(draft.validation_bundle_sha256)}
          />
          <Metadata
            label={t("approvedBundle")}
            value={shortHash(draft.approved_bundle_sha256)}
          />
          <Metadata
            label={t("publishedChallengeName")}
            value={draft.published_challenge_name ?? "—"}
          />
        </dl>
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<FileArchive className="w-4 h-4" />}
            title={t("privateAssets")}
          />
          <span className="badge badge-default">
            {common("rows", { count: draft.private_assets.length })}
          </span>
        </div>
        {draft.private_assets.length === 0 ? (
          <div className="empty-state">{t("noPrivateAssets")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("asset")}</th>
                <th>{t("kind")}</th>
                <th>{t("size")}</th>
                <th>SHA-256</th>
              </tr>
            </thead>
            <tbody>
              {draft.private_assets.map((asset) => (
                <tr key={asset.id}>
                  <td>
                    <div className="font-mono">{asset.asset_name}</div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                      {asset.required ? t("required") : t("optional")}
                    </div>
                  </td>
                  <td>{asset.kind}</td>
                  <td className="font-mono">{asset.size_bytes}</td>
                  <td className="font-mono">{shortHash(asset.sha256)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<RefreshCw className="w-4 h-4" />}
            title={t("validationRecords")}
          />
          <span className="badge badge-default">
            {common("rows", { count: draft.validation_records.length })}
          </span>
        </div>
        {draft.validation_records.length === 0 ? (
          <div className="empty-state">{t("noValidationRecords")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("status")}</th>
                <th>{t("message")}</th>
                <th>{t("bundle")}</th>
              </tr>
            </thead>
            <tbody>
              {draft.validation_records.map((record) => (
                <tr key={record.id}>
                  <td>
                    <LocalizedStatusBadge status={record.status} />
                  </td>
                  <td>{record.message}</td>
                  <td className="font-mono">
                    {shortHash(record.bundle_sha256)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

/** Renders the owner surfaces component. */
export function OwnerSurfaces({
  stats,
  participants,
  shortlist,
  shortlistRevision,
}: {
  stats: CreatorChallengeStatsResponse | null;
  participants: CreatorChallengeParticipantsResponse | null;
  shortlist: ChallengeShortlistResponse | null;
  shortlistRevision: ChallengeShortlistRevisionResponse | null;
}) {
  const t = useTranslations("creator.owner");
  const common = useTranslations("common");

  return (
    <div className="flex flex-col gap-5">
      <div className="card">
        <SectionTitle
          icon={<BarChart3 className="w-4 h-4" />}
          title={t("statistics")}
        />
        {!stats ? (
          <div className="empty-state mt-4">{t("loadChallenge")}</div>
        ) : (
          <dl className="mt-5 grid grid-cols-2 md:grid-cols-4 gap-4 text-[var(--text-body-sm)]">
            <Metadata
              label={t("agents")}
              value={stats.agent_count.toString()}
            />
            <Metadata
              label={t("submissions")}
              value={stats.solution_submission_count.toString()}
            />
            <Metadata
              label={t("completed")}
              value={stats.completed_solution_submission_count.toString()}
            />
            <Metadata
              label={t("failed")}
              value={stats.failed_solution_submission_count.toString()}
            />
            <Metadata
              label={t("queuedOrRunning")}
              value={stats.queued_or_running_solution_submission_count.toString()}
            />
            <Metadata
              label={t("validationRuns")}
              value={stats.validation_run_count.toString()}
            />
            <Metadata
              label={t("officialRuns")}
              value={stats.official_run_count.toString()}
            />
            <Metadata
              label={t("bestScoreMean")}
              value={formatOptionalScore(stats.best_rank_score_mean)}
            />
          </dl>
        )}
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<Users className="w-4 h-4" />}
            title={t("participants")}
          />
          <span className="badge badge-default">
            {common("rows", { count: participants?.items.length ?? 0 })}
          </span>
        </div>
        {!participants || participants.items.length === 0 ? (
          <div className="empty-state">{t("noParticipants")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("agent")}</th>
                <th>{t("submissions")}</th>
                <th>{t("best")}</th>
                <th>{common("status")}</th>
                <th>{t("latest")}</th>
              </tr>
            </thead>
            <tbody>
              {participants.items.map((participant) => (
                <tr key={participant.agent_id}>
                  <td>
                    <div className="font-medium">
                      {participant.agent_display_name}
                    </div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {participant.agent_id}
                    </div>
                  </td>
                  <td>{participant.solution_submission_count}</td>
                  <td>
                    <div className="font-mono">
                      {formatOptionalScore(participant.best_rank_score)}
                    </div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {participant.best_solution_submission_id ?? "—"}
                    </div>
                  </td>
                  <td>
                    {participant.latest_status ? (
                      <LocalizedStatusBadge
                        status={participant.latest_status}
                      />
                    ) : (
                      common("none")
                    )}
                  </td>
                  <td>
                    {participant.latest_solution_submission_at ??
                      common("none")}
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
            icon={<ListPlus className="w-4 h-4" />}
            title={t("shortlist")}
          />
          <span className="badge badge-default">
            {common("rows", { count: shortlist?.items.length ?? 0 })}
          </span>
        </div>
        {shortlistRevision ? (
          <div className="mb-4 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
            {t("lastRevision", {
              added: shortlistRevision.added_count,
              requested: shortlistRevision.requested_count,
            })}
          </div>
        ) : null}
        {!shortlist || shortlist.items.length === 0 ? (
          <div className="empty-state">{t("noShortlist")}</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("agent")}</th>
                <th>{t("addedBy")}</th>
                <th>{t("created")}</th>
              </tr>
            </thead>
            <tbody>
              {shortlist.items.map((agent) => (
                <tr key={agent.agent_id}>
                  <td>
                    <div className="font-medium">
                      {agent.agent_display_name}
                    </div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {agent.agent_id}
                    </div>
                  </td>
                  <td className="font-mono">{agent.added_by_agent_id}</td>
                  <td>{agent.created_at}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

/** Returns the browser document locale without requiring a Next Intl provider. */
function currentDocumentLocale(): string {
  return document.documentElement.lang || navigator.language || "en";
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

/** Renders the metadata component. */
function Metadata({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
        {label}
      </dt>
      <dd className="mt-1 font-mono break-all">{value}</dd>
    </div>
  );
}

/** Formats optional score for display. */
function formatOptionalScore(value: number | undefined): string {
  if (value === undefined) {
    return "—";
  }
  return Number.isInteger(value) ? value.toFixed(0) : value.toFixed(4);
}

/** Handles short hash behavior for this module. */
function shortHash(value: string | undefined): string {
  if (!value) {
    return "—";
  }
  return value.length > 16 ? value.slice(0, 16) : value;
}
