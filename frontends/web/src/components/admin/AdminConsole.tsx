"use client";

import {
  Activity,
  Ban,
  Boxes,
  EyeOff,
  FlaskConical,
  Gauge,
  GitPullRequest,
  Play,
  RefreshCw,
  Server,
  ShieldCheck,
} from "lucide-react";
import { useLocale } from "next-intl";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { ChallengeDraftReviewPanel } from "@/components/admin/ChallengeDraftReviewPanel";
import { CredentialPanel } from "@/components/admin/CredentialPanel";
import { PioneerCodePanel } from "@/components/admin/PioneerCodePanel";
import { StatusBadge } from "@/components/admin/StatusBadge";
import {
  AdminApiError,
  type AdminCredentials,
  adminFetchJson,
  adminLogin,
  adminLogout,
  adminSession,
} from "@/lib/adminApi";
import { formatDate, formatScore } from "@/lib/format";
import {
  type AdminCapacityResponse,
  type AdminChallengeListItem,
  type AdminChallengeListResponse,
  type AdminServiceHeartbeatListResponse,
  type AdminSolutionSubmissionListItem,
  type AdminSolutionSubmissionListResponse,
  adminCapacityResponseSchema,
  adminChallengeListResponseSchema,
  adminServiceHeartbeatListResponseSchema,
  adminSolutionSubmissionListResponseSchema,
  type ChallengeDraftListResponse,
  challengeDraftListResponseSchema,
  disableAgentResponseSchema,
  evaluationJobResponseSchema,
  hideSolutionSubmissionResponseSchema,
  type PioneerCodeListResponse,
  pioneerCodeListResponseSchema,
} from "@/lib/schemas";

/** Describes the admin tab shape used by this module. */
type AdminTab =
  | "overview"
  | "challenges"
  | "drafts"
  | "pioneer-codes"
  | "capacity"
  | "operations";
/** Describes the refresh options shape used by this module. */
type RefreshOptions = { quiet?: boolean };
/** Describes the admin refresh shape used by this module. */
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

/** Describes the admin data shape used by this module. */
interface AdminData {
  challenges: AdminChallengeListResponse;
  drafts: ChallengeDraftListResponse;
  submissions: AdminSolutionSubmissionListResponse;
  heartbeats: AdminServiceHeartbeatListResponse;
  pioneerCodes: PioneerCodeListResponse;
  capacity: AdminCapacityResponse | null;
}

const emptyData: AdminData = {
  challenges: { items: [] },
  drafts: { items: [] },
  submissions: { items: [] },
  heartbeats: { items: [] },
  pioneerCodes: { items: [] },
  capacity: null,
};

/** Renders the admin console component. */
export function AdminConsole() {
  const locale = useLocale();
  const [credentials, setCredentials] = useState<AdminCredentials>({
    username: "admin",
    password: "",
  });
  const [csrfToken, setCsrfToken] = useState("");
  const [sessionUsername, setSessionUsername] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<AdminTab>("overview");
  const [data, setData] = useState<AdminData>(emptyData);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isConfigured = credentials.username.trim() && credentials.password;

  /** Fetches all admin data needed by the dashboard shell. */
  const fetchAdminData = useCallback(
    async (token: string): Promise<AdminData> => {
      const [
        challenges,
        drafts,
        submissions,
        heartbeats,
        pioneerCodes,
        capacity,
      ] = await Promise.all([
        adminFetchJson(
          "/admin/challenges",
          adminChallengeListResponseSchema,
          token,
        ),
        adminFetchJson(
          "/admin/challenge-drafts",
          challengeDraftListResponseSchema,
          token,
        ),
        adminFetchJson(
          "/admin/solution-submissions",
          adminSolutionSubmissionListResponseSchema,
          token,
        ),
        adminFetchJson(
          "/admin/service-heartbeats",
          adminServiceHeartbeatListResponseSchema,
          token,
        ),
        adminFetchJson(
          "/admin/pioneer-codes",
          pioneerCodeListResponseSchema,
          token,
        ),
        adminFetchJson("/admin/capacity", adminCapacityResponseSchema, token),
      ]);

      return {
        challenges,
        drafts,
        submissions,
        heartbeats,
        pioneerCodes,
        capacity,
      };
    },
    [],
  );

  const loginAndRefresh: AdminRefresh = async (options = {}) => {
    if (!isConfigured) {
      setError("Enter admin credentials before loading operator data.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const session = await adminLogin(credentials);
      setCsrfToken(session.csrf_token);
      setSessionUsername(session.username);
      setCredentials({ username: session.username, password: "" });
      setData(await fetchAdminData(session.csrf_token));
      if (!options.quiet) {
        setMessage("Admin session started and operator data refreshed.");
      }
    } catch (e) {
      setError(adminErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    let cancelled = false;

    /** Restores a cookie-backed admin session without exposing credentials. */
    const restoreSession = async () => {
      try {
        const session = await adminSession();
        const nextData = await fetchAdminData(session.csrf_token);
        if (!cancelled) {
          setCsrfToken(session.csrf_token);
          setSessionUsername(session.username);
          setCredentials({ username: session.username, password: "" });
          setData(nextData);
        }
      } catch (e) {
        if (
          !cancelled &&
          e instanceof AdminApiError &&
          e.status !== 401 &&
          e.status !== 403
        ) {
          setError(adminErrorMessage(e));
        }
      }
    };

    void restoreSession();

    return () => {
      cancelled = true;
    };
  }, [fetchAdminData]);

  /** Handles sign out for the current session. */
  const signOut = async () => {
    if (!csrfToken) {
      setSessionUsername(null);
      setData(emptyData);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await adminLogout(csrfToken);
      setCsrfToken("");
      setSessionUsername(null);
      setData(emptyData);
      setMessage("Admin session ended.");
    } catch (e) {
      setError(adminErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const refresh: AdminRefresh = async (options = {}) => {
    if (!csrfToken) {
      setError("Sign in before refreshing operator data.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      setData(await fetchAdminData(csrfToken));
      if (!options.quiet) {
        setMessage("Operator data refreshed.");
      }
    } catch (e) {
      setError(adminErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  /** Handles status counts behavior for this component. */
  const statusCounts = useMemo(() => {
    return data.submissions.items.reduce<Record<string, number>>(
      (acc, item) => {
        acc[item.status] = (acc[item.status] ?? 0) + 1;
        return acc;
      },
      {},
    );
  }, [data.submissions.items]);

  return (
    <div className="flex flex-col gap-6">
      <section className="card-elevated">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-6">
          <div>
            <span className="badge badge-official mb-4">
              <ShieldCheck className="w-3 h-3" />
              Admin Observatory
            </span>
            <h1
              className="text-[var(--text-h1)] font-bold leading-[var(--leading-h1)]"
              style={{ fontFamily: "var(--font-serif)" }}
            >
              Platform operations console
            </h1>
            <p className="mt-3 max-w-2xl text-[var(--text-body)] leading-[var(--leading-body)] text-[var(--text-secondary)]">
              Publish challenge contracts, inspect evaluation flow, and keep
              worker capacity visible without leaving the Agentics observatory.
            </p>
          </div>
          <CredentialPanel
            credentials={credentials}
            sessionUsername={sessionUsername}
            onChange={setCredentials}
            onLogin={loginAndRefresh}
            onLogout={signOut}
            loading={loading}
          />
        </div>
      </section>

      {error ? (
        <div className="card border-[var(--status-error)]/40 text-[var(--status-error)]">
          {error}
        </div>
      ) : null}
      {message ? (
        <div className="card border-[var(--status-success)]/30 text-[var(--status-success)]">
          {message}
        </div>
      ) : null}

      <nav className="tab-list overflow-x-auto">
        {[
          ["overview", "Overview"],
          ["challenges", "Challenges"],
          ["drafts", "Drafts"],
          ["pioneer-codes", "Pioneer codes"],
          ["capacity", "Capacity"],
          ["operations", "Operations"],
        ].map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={`tab-link ${activeTab === id ? "active" : ""}`}
            onClick={() => setActiveTab(id as AdminTab)}
          >
            {label}
          </button>
        ))}
      </nav>

      {activeTab === "overview" ? (
        <OverviewPanel data={data} statusCounts={statusCounts} />
      ) : null}
      {activeTab === "challenges" ? (
        <ChallengeAdminPanel
          challenges={data.challenges.items}
          locale={locale}
        />
      ) : null}
      {activeTab === "drafts" ? (
        <ChallengeDraftReviewPanel
          csrfToken={csrfToken}
          drafts={data.drafts.items}
          locale={locale}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {activeTab === "pioneer-codes" ? (
        <PioneerCodePanel
          csrfToken={csrfToken}
          pioneerCodes={data.pioneerCodes}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {activeTab === "capacity" ? (
        <CapacityPanel capacity={data.capacity} />
      ) : null}
      {activeTab === "operations" ? (
        <OperationsPanel
          csrfToken={csrfToken}
          submissions={data.submissions.items}
          heartbeats={data.heartbeats.items}
          locale={locale}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
    </div>
  );
}

/** Renders the overview panel component. */
function OverviewPanel({
  data,
  statusCounts,
}: {
  data: AdminData;
  statusCounts: Record<string, number>;
}) {
  const activeWorkers = data.heartbeats.items.length;
  const queued = statusCounts.queued ?? 0;
  const running = statusCounts.running ?? 0;
  const activeOfficialJobs = data.capacity?.usage.active_official_jobs ?? 0;

  return (
    <section className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-6 gap-5">
      <StatCard
        icon={<FlaskConical className="w-5 h-5" />}
        label="Challenges"
        value={data.challenges.items.length.toString()}
        tone="teal"
      />
      <StatCard
        icon={<GitPullRequest className="w-5 h-5" />}
        label="Drafts"
        value={data.drafts.items.length.toString()}
        tone="teal"
      />
      <StatCard
        icon={<Boxes className="w-5 h-5" />}
        label="Solution submissions"
        value={data.submissions.items.length.toString()}
        tone="amber"
      />
      <StatCard
        icon={<Activity className="w-5 h-5" />}
        label="Queued / Running"
        value={`${queued} / ${running}`}
        tone="amber"
      />
      <StatCard
        icon={<Gauge className="w-5 h-5" />}
        label="Official capacity"
        value={`${activeOfficialJobs}/${data.capacity?.quotas.max_active_official_jobs ?? "—"}`}
        tone="amber"
      />
      <StatCard
        icon={<Server className="w-5 h-5" />}
        label="Worker heartbeats"
        value={activeWorkers.toString()}
        tone="teal"
      />
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

/** Renders the challenge admin panel component. */
function ChallengeAdminPanel({
  challenges,
  locale,
}: {
  challenges: AdminChallengeListItem[];
  locale: string;
}) {
  return (
    <section className="grid grid-cols-1 gap-6">
      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <h2 className="text-[var(--text-h3)] font-semibold">
            Challenge registry
          </h2>
          <span className="badge badge-default">{challenges.length} rows</span>
        </div>
        {challenges.length === 0 ? (
          <div className="empty-state">
            Load admin data to inspect challenges.
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Challenge</th>
                <th>Status</th>
                <th>Eligibility</th>
                <th>Targets</th>
                <th>Modes</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {challenges.map((challenge) => (
                <tr key={challenge.name}>
                  <td>
                    <div className="font-medium">{challenge.title}</div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {challenge.name}
                    </div>
                  </td>
                  <td>
                    <StatusBadge status={challenge.status} />
                  </td>
                  <td>
                    <div className="font-mono">
                      {challenge.eligibility?.type ?? "—"}
                    </div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                      {challenge.starts_at
                        ? `starts ${formatDate(challenge.starts_at, locale)}`
                        : "starts anytime"}
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

/** Renders admin controls for creating, inspecting, and revoking pioneer codes. */
/** Renders the capacity panel component. */
function CapacityPanel({
  capacity,
}: {
  capacity: AdminCapacityResponse | null;
}) {
  if (!capacity) {
    return (
      <section className="card">
        <div className="empty-state">
          Load admin data to inspect resource profiles and quotas.
        </div>
      </section>
    );
  }

  const quotaRows = [
    [
      "Validation per agent/challenge",
      capacity.quotas.validation_runs_per_agent_challenge_day.toString(),
    ],
    [
      "Official per agent/challenge",
      capacity.quotas.official_runs_per_agent_challenge_day.toString(),
    ],
    [
      "Active official jobs",
      capacity.quotas.max_active_official_jobs.toString(),
    ],
    ["Active agents", capacity.quotas.max_active_agents.toString()],
  ];
  const usageRows = [
    ["Agents", capacity.usage.active_agents.toString()],
    ["Validation jobs", capacity.usage.active_validation_jobs.toString()],
    ["Official jobs", capacity.usage.active_official_jobs.toString()],
  ];

  return (
    <section className="grid grid-cols-1 xl:grid-cols-2 gap-6">
      <div className="card overflow-x-auto">
        <SectionTitle
          icon={<Gauge className="w-4 h-4" />}
          title="Resource profiles and quotas"
        />
        <p className="mt-2 mb-4 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
          Limits are loaded from backend configuration and enforced before
          uploads consume storage or worker capacity.
        </p>
        <table className="data-table">
          <thead>
            <tr>
              <th>Quota</th>
              <th>Limit</th>
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
          title="Current capacity usage"
        />
        <p className="mt-2 mb-4 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
          Rolling submission quotas use a {capacity.quota_window_seconds / 3600}
          h window.
        </p>
        <table className="data-table">
          <thead>
            <tr>
              <th>Resource</th>
              <th>Current usage</th>
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

/** Renders the target summary component. */
function TargetSummary({ challenge }: { challenge: AdminChallengeListItem }) {
  const targets = challenge.targets ?? [];
  if (targets.length === 0) {
    return <span className="text-[var(--text-muted)]">—</span>;
  }

  return (
    <div className="flex flex-col gap-1">
      {targets.map((target) => (
        <div key={target.name}>
          <div className="font-mono text-[var(--text-caption)]">
            {target.name}
          </div>
          <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
            {target.docker_platform} ·{" "}
            {target.resource_profile.cpu_limit_millis}m ·{" "}
            {target.resource_profile.memory_limit_mb} MiB
          </div>
        </div>
      ))}
    </div>
  );
}

/** Renders the mode summary component. */
function ModeSummary({ challenge }: { challenge: AdminChallengeListItem }) {
  const targets = challenge.targets ?? [];
  /** Handles validation enabled behavior for this component. */
  const validationEnabled = targets.some((target) => target.validation_enabled);

  return (
    <div className="flex flex-wrap gap-2">
      <span
        className={`badge ${
          validationEnabled ? "badge-success" : "badge-default"
        }`}
      >
        validation {validationEnabled ? "on" : "off"}
      </span>
      <span
        className={`badge ${
          challenge.private_benchmark_enabled
            ? "badge-official"
            : "badge-default"
        }`}
      >
        official {challenge.private_benchmark_enabled ? "on" : "off"}
      </span>
    </div>
  );
}

/** Renders the operations panel component. */
function OperationsPanel({
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
  return (
    <section className="grid grid-cols-1 gap-6">
      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<Boxes className="w-4 h-4" />}
            title="Solution submission operations"
          />
          <span className="badge badge-default">{submissions.length} rows</span>
        </div>
        {submissions.length === 0 ? (
          <div className="empty-state">
            Load admin data to inspect solution submissions.
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Submission</th>
                <th>Status</th>
                <th>Latest job</th>
                <th>Rank</th>
                <th>Updated</th>
                <th>Actions</th>
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
                  </td>
                  <td>
                    <StatusBadge status={submission.status} />
                  </td>
                  <td>
                    <div className="font-mono text-[var(--text-caption)]">
                      {submission.latest_job_id?.slice(0, 8) ?? "—"}
                    </div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                      {submission.latest_job_eval_type ?? "no job"} ·{" "}
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
            title="Worker heartbeats"
          />
          <span className="badge badge-default">{heartbeats.length} rows</span>
        </div>
        {heartbeats.length === 0 ? (
          <div className="empty-state">No worker heartbeats recorded yet.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Service</th>
                <th>Status</th>
                <th>Last seen</th>
                <th>Payload</th>
              </tr>
            </thead>
            <tbody>
              {heartbeats.map((heartbeat) => (
                <tr key={heartbeat.service_name}>
                  <td className="font-mono">{heartbeat.service_name}</td>
                  <td>
                    <StatusBadge
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
  /** Runs action and refreshes affected data. */
  const runAction = async (
    action: "rejudge" | "official-run" | "hide" | "disable-agent",
  ) => {
    try {
      if (action === "disable-agent") {
        if (!window.confirm(`Disable agent ${submission.agent_display_name}?`))
          return;
        await adminFetchJson(
          `/admin/agents/${encodeURIComponent(submission.agent_id)}/disable`,
          disableAgentResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(`Disabled agent ${submission.agent_display_name}.`);
      } else if (action === "hide") {
        if (!window.confirm(`Hide submission ${submission.id.slice(0, 8)}?`))
          return;
        await adminFetchJson(
          `/admin/solution-submissions/${encodeURIComponent(submission.id)}/hide`,
          hideSolutionSubmissionResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(`Hidden submission ${submission.id.slice(0, 8)}.`);
      } else {
        const actionLabel =
          action === "official-run" ? "queue an official run for" : "rejudge";
        if (
          !window.confirm(
            `${actionLabel} submission ${submission.id.slice(0, 8)}?`,
          )
        )
          return;
        const response = await adminFetchJson(
          `/admin/solution-submissions/${encodeURIComponent(submission.id)}/${action}`,
          evaluationJobResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(`Queued ${response.eval_type} job ${response.job_id}.`);
      }
      onError(null);
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e));
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      <button
        type="button"
        className="btn btn-secondary btn-sm"
        onClick={() => runAction("rejudge")}
      >
        <RefreshCw className="w-3 h-3" />
        Rejudge
      </button>
      <button
        type="button"
        className="btn btn-secondary btn-sm"
        onClick={() => runAction("official-run")}
      >
        <Play className="w-3 h-3" />
        Official
      </button>
      <button
        type="button"
        className="btn btn-ghost btn-sm"
        onClick={() => runAction("hide")}
      >
        <EyeOff className="w-3 h-3" />
        Hide
      </button>
      <button
        type="button"
        className="btn btn-ghost btn-sm text-[var(--status-error)]"
        onClick={() => runAction("disable-agent")}
      >
        <Ban className="w-3 h-3" />
        Disable agent
      </button>
    </div>
  );
}

/** Renders the section title component. */
function SectionTitle({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <h2 className="flex items-center gap-2 text-[var(--text-h3)] font-semibold">
      <span className="text-[var(--accent-secondary-text)]">{icon}</span>
      {title}
    </h2>
  );
}

/** Normalizes unknown errors into a displayable message. */
function adminErrorMessage(error: unknown): string {
  if (error instanceof AdminApiError) {
    if (error.status === 401) {
      return "Access denied. Check the admin username and password.";
    }
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unknown admin console error.";
}
