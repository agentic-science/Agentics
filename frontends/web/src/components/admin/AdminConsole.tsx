"use client";

import {
  Activity,
  Ban,
  Boxes,
  EyeOff,
  FlaskConical,
  Gauge,
  GitPullRequest,
  KeyRound,
  Play,
  RefreshCw,
  Rocket,
  Server,
  ShieldCheck,
  UploadCloud,
} from "lucide-react";
import { useLocale } from "next-intl";
import { type FormEvent, type ReactNode, useMemo, useState } from "react";
import { ChallengeDraftReviewPanel } from "@/components/admin/ChallengeDraftReviewPanel";
import {
  AdminApiError,
  type AdminCredentials,
  adminFetchJson,
  adminLogin,
  adminLogout,
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
  challengeAdminResponseSchema,
  challengeDraftListResponseSchema,
  disableAgentResponseSchema,
  evaluationJobResponseSchema,
  hideSolutionSubmissionResponseSchema,
  publishChallengeResponseSchema,
} from "@/lib/schemas";

type AdminTab =
  | "overview"
  | "challenges"
  | "drafts"
  | "capacity"
  | "operations";
type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

interface AdminData {
  challenges: AdminChallengeListResponse;
  drafts: ChallengeDraftListResponse;
  submissions: AdminSolutionSubmissionListResponse;
  heartbeats: AdminServiceHeartbeatListResponse;
  capacity: AdminCapacityResponse | null;
}

const emptyData: AdminData = {
  challenges: { items: [] },
  drafts: { items: [] },
  submissions: { items: [] },
  heartbeats: { items: [] },
  capacity: null,
};

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

  const fetchAdminData = async (token: string): Promise<AdminData> => {
    const [challenges, drafts, submissions, heartbeats, capacity] =
      await Promise.all([
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
        adminFetchJson("/admin/capacity", adminCapacityResponseSchema, token),
      ]);

    return { challenges, drafts, submissions, heartbeats, capacity };
  };

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
          csrfToken={csrfToken}
          challenges={data.challenges.items}
          locale={locale}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
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

function CredentialPanel({
  credentials,
  sessionUsername,
  onChange,
  onLogin,
  onLogout,
  loading,
}: {
  credentials: AdminCredentials;
  sessionUsername: string | null;
  onChange: (credentials: AdminCredentials) => void;
  onLogin: AdminRefresh;
  onLogout: () => Promise<void>;
  loading: boolean;
}) {
  return (
    <div className="card min-w-full lg:min-w-[360px] lg:max-w-[420px]">
      <div className="flex items-center gap-2 mb-4">
        <KeyRound className="w-4 h-4 text-[var(--accent-primary-text)]" />
        <h2 className="text-[var(--text-h3)] font-semibold">Admin sign-in</h2>
      </div>
      <div className="grid grid-cols-1 gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Username
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            value={credentials.username}
            onChange={(event) =>
              onChange({ ...credentials, username: event.target.value })
            }
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
            Password
          </span>
          <input
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
            type="password"
            value={credentials.password}
            onChange={(event) =>
              onChange({ ...credentials, password: event.target.value })
            }
          />
        </label>
      </div>
      <div className="mt-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
          {sessionUsername
            ? `Signed in as ${sessionUsername}`
            : "Password is exchanged for an HttpOnly session cookie."}
        </span>
        <div className="flex gap-2">
          {sessionUsername ? (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void onLogout()}
              disabled={loading}
            >
              Sign out
            </button>
          ) : null}
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void onLogin()}
            disabled={loading}
          >
            <RefreshCw className="w-4 h-4" />
            {loading ? "Loading" : "Sign in"}
          </button>
        </div>
      </div>
    </div>
  );
}

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

function ChallengeAdminPanel({
  csrfToken,
  challenges,
  locale,
  onRefresh,
  onError,
  onMessage,
}: {
  csrfToken: string;
  challenges: AdminChallengeListItem[];
  locale: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}) {
  return (
    <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
      <div className="flex flex-col gap-5">
        <ChallengeShellForm
          csrfToken={csrfToken}
          onRefresh={onRefresh}
          onError={onError}
          onMessage={onMessage}
        />
        <PublishVersionForm
          csrfToken={csrfToken}
          onRefresh={onRefresh}
          onError={onError}
          onMessage={onMessage}
        />
      </div>
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
                <tr key={challenge.id}>
                  <td>
                    <div className="font-medium">{challenge.title}</div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {challenge.id} · {challenge.slug}
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
                    <BenchmarkTargetSummary challenge={challenge} />
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

function BenchmarkTargetSummary({
  challenge,
}: {
  challenge: AdminChallengeListItem;
}) {
  const targets = challenge.benchmark_targets ?? [];
  if (targets.length === 0) {
    return <span className="text-[var(--text-muted)]">—</span>;
  }

  return (
    <div className="flex flex-col gap-1">
      {targets.map((target) => (
        <div key={target.id}>
          <div className="font-mono text-[var(--text-caption)]">
            {target.id}
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

function ModeSummary({ challenge }: { challenge: AdminChallengeListItem }) {
  const targets = challenge.benchmark_targets ?? [];
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

function ChallengeShellForm({
  csrfToken,
  onRefresh,
  onError,
  onMessage,
}: ActionProps) {
  const [form, setForm] = useState({
    id: "",
    slug: "",
    title: "",
    summary: "",
  });

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const body = {
        id: form.id.trim(),
        slug: form.slug.trim() || undefined,
        title: form.title.trim(),
        summary: form.summary.trim(),
      };
      const response = await adminFetchJson(
        "/admin/challenges",
        challengeAdminResponseSchema,
        csrfToken,
        {
          method: "POST",
          body: JSON.stringify(body),
        },
      );
      onError(null);
      onMessage(`Challenge shell saved: ${response.id}`);
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e));
    }
  };

  return (
    <form className="card flex flex-col gap-4" onSubmit={submit}>
      <SectionTitle
        icon={<Rocket className="w-4 h-4" />}
        title="Challenge shell"
      />
      <TextInput
        label="Challenge ID"
        value={form.id}
        onChange={(id) => setForm({ ...form, id })}
        required
      />
      <TextInput
        label="Slug"
        value={form.slug}
        onChange={(slug) => setForm({ ...form, slug })}
      />
      <TextInput
        label="Title"
        value={form.title}
        onChange={(title) => setForm({ ...form, title })}
        required
      />
      <label className="flex flex-col gap-1">
        <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
          Summary
        </span>
        <textarea
          className="min-h-24 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
          value={form.summary}
          onChange={(event) =>
            setForm({ ...form, summary: event.target.value })
          }
        />
      </label>
      <button type="submit" className="btn btn-primary">
        Save shell
      </button>
    </form>
  );
}

function PublishVersionForm({
  csrfToken,
  onRefresh,
  onError,
  onMessage,
}: ActionProps) {
  const [form, setForm] = useState({ challengeId: "", bundlePath: "" });

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const response = await adminFetchJson(
        `/admin/challenges/${encodeURIComponent(form.challengeId.trim())}/publish`,
        publishChallengeResponseSchema,
        csrfToken,
        {
          method: "POST",
          body: JSON.stringify({ bundle_path: form.bundlePath.trim() }),
        },
      );
      onError(null);
      onMessage(
        `Published ${response.challenge_id} from ${response.bundle_path}`,
      );
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e));
    }
  };

  return (
    <form className="card flex flex-col gap-4" onSubmit={submit}>
      <SectionTitle
        icon={<UploadCloud className="w-4 h-4" />}
        title="Publish bundle"
      />
      <TextInput
        label="Challenge ID"
        value={form.challengeId}
        onChange={(challengeId) => setForm({ ...form, challengeId })}
        required
      />
      <TextInput
        label="Bundle path"
        value={form.bundlePath}
        placeholder="sample-sum/v1"
        onChange={(bundlePath) => setForm({ ...form, bundlePath })}
        required
      />
      <button type="submit" className="btn btn-primary">
        Validate and publish
      </button>
    </form>
  );
}

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
                      {submission.id.slice(0, 8)} · {submission.agent_name}
                    </div>
                    <div className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                      {submission.benchmark_target_id}
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
  const runAction = async (
    action: "rejudge" | "official-run" | "hide" | "disable-agent",
  ) => {
    try {
      if (action === "disable-agent") {
        if (!window.confirm(`Disable agent ${submission.agent_name}?`)) return;
        await adminFetchJson(
          `/admin/agents/${encodeURIComponent(submission.agent_id)}/disable`,
          disableAgentResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(`Disabled agent ${submission.agent_name}.`);
      } else if (action === "hide") {
        await adminFetchJson(
          `/admin/solution-submissions/${encodeURIComponent(submission.id)}/hide`,
          hideSolutionSubmissionResponseSchema,
          csrfToken,
          { method: "POST" },
        );
        onMessage(`Hidden submission ${submission.id.slice(0, 8)}.`);
      } else {
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

interface ActionProps {
  csrfToken: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

function SectionTitle({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <h2 className="flex items-center gap-2 text-[var(--text-h3)] font-semibold">
      <span className="text-[var(--accent-secondary-text)]">{icon}</span>
      {title}
    </h2>
  );
}

function TextInput({
  label,
  value,
  onChange,
  required,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  required?: boolean;
  placeholder?: string;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
        {label}
      </span>
      <input
        className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        required={required}
        placeholder={placeholder}
      />
    </label>
  );
}

function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const className =
    normalized === "completed" ||
    normalized === "active" ||
    normalized === "idle"
      ? "badge-success"
      : normalized === "failed" ||
          normalized === "error" ||
          normalized === "disabled"
        ? "badge-error"
        : normalized === "running" ||
            normalized === "queued" ||
            normalized === "pending"
          ? "badge-warning"
          : "badge-default";

  return <span className={`badge ${className}`}>{status}</span>;
}

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
