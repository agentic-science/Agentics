"use client";

import {
  FileArchive,
  GitPullRequest,
  KeyRound,
  RefreshCw,
  UploadCloud,
} from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useState } from "react";
import {
  type ChallengeCreationManifest,
  type ChallengePrivateAssetKind,
  type CreateChallengeDraftRequest,
  CreatorApiError,
  createChallengeDraft,
  getChallengeDraft,
  getCreatorMe,
  readCreatorCsrfToken,
  startGithubLogin,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import type { ChallengeDraftResponse, CreatorMeResponse } from "@/lib/schemas";

const LAST_DRAFT_STORAGE_KEY = "agentics.creator.last_draft_id";

const defaultManifest = JSON.stringify(
  {
    schema_version: 1,
    request: "new_challenge",
    challenge_id: "matrix-multiplication",
    title: "Matrix Multiplication",
    summary: "Benchmark matrix multiplication solutions.",
    readme_path: "README.md",
    bundle_path: "v1",
    private_assets: [
      {
        asset_id: "official-seed-config",
        kind: "private_seeds",
        required: true,
      },
    ],
  },
  null,
  2,
);

const assetKinds: ChallengePrivateAssetKind[] = [
  "private_benchmark_data",
  "private_scorer_package",
  "private_seeds",
  "private_reference_outputs",
];

export function CreatorConsole() {
  const [creator, setCreator] = useState<CreatorMeResponse | null>(null);
  const [csrfToken, setCsrfToken] = useState("");
  const [draft, setDraft] = useState<ChallengeDraftResponse | null>(null);
  const [draftLookupId, setDraftLookupId] = useState("");
  const [draftForm, setDraftForm] = useState({
    repoUrl: "https://github.com/agentics-reifying/agentics-challenges",
    prNumber: "",
    prUrl: "",
    commitSha: "",
    challengePath: "challenges/matrix-multiplication",
    manifestText: defaultManifest,
  });
  const [assetForm, setAssetForm] = useState<{
    draftId: string;
    assetId: string;
    kind: ChallengePrivateAssetKind;
    required: boolean;
    file: File | null;
  }>({
    draftId: "",
    assetId: "official-seed-config",
    kind: "private_seeds",
    required: true,
    file: null,
  });
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setCsrfToken(readCreatorCsrfToken());
    const lastDraftId = window.localStorage.getItem(LAST_DRAFT_STORAGE_KEY);
    if (lastDraftId) {
      setDraftLookupId(lastDraftId);
      setAssetForm((current) => ({ ...current, draftId: lastDraftId }));
    }

    void getCreatorMe()
      .then(setCreator)
      .catch(() => {
        setCreator(null);
      });
  }, []);

  const signIn = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await startGithubLogin();
      window.location.href = response.authorization_url;
    } catch (e) {
      setError(creatorErrorMessage(e));
      setLoading(false);
    }
  };

  const refreshIdentity = async () => {
    setLoading(true);
    setError(null);
    try {
      setCreator(await getCreatorMe());
      setCsrfToken(readCreatorCsrfToken());
      setMessage("Creator identity refreshed.");
    } catch (e) {
      setCreator(null);
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const submitDraft = async (event: FormEvent) => {
    event.preventDefault();
    if (!creator) {
      setError("Sign in with GitHub before creating a challenge draft.");
      return;
    }
    if (!csrfToken) {
      setError("Refresh the creator session before creating a draft.");
      return;
    }

    const prNumber = Number.parseInt(draftForm.prNumber, 10);
    if (!Number.isInteger(prNumber) || prNumber <= 0) {
      setError("PR number must be a positive integer.");
      return;
    }

    let manifest: ChallengeCreationManifest;
    try {
      manifest = JSON.parse(
        draftForm.manifestText,
      ) as ChallengeCreationManifest;
    } catch (e) {
      setError(creatorErrorMessage(e));
      return;
    }

    const request: CreateChallengeDraftRequest = {
      repo_url: draftForm.repoUrl.trim(),
      pr_number: prNumber,
      pr_url: draftForm.prUrl.trim(),
      commit_sha: draftForm.commitSha.trim(),
      challenge_path: draftForm.challengePath.trim(),
      pr_author_github_user_id: creator.github_user_id,
      manifest,
    };

    setLoading(true);
    setError(null);
    try {
      const response = await createChallengeDraft(request, csrfToken);
      rememberDraft(response.id);
      setDraft(response);
      setMessage(`Challenge draft created: ${response.id}`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const inspectDraft = async (event: FormEvent) => {
    event.preventDefault();
    if (!draftLookupId.trim()) {
      setError("Enter a draft id to inspect.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const response = await getChallengeDraft(draftLookupId.trim());
      rememberDraft(response.id);
      setDraft(response);
      setMessage(`Loaded draft ${response.id}.`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const uploadAsset = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      setError("Refresh the creator session before uploading private assets.");
      return;
    }
    if (!assetForm.file) {
      setError("Choose a ZIP asset before uploading.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await uploadPrivateAsset(
        assetForm.draftId.trim(),
        {
          asset_id: assetForm.assetId.trim(),
          kind: assetForm.kind,
          required: assetForm.required,
          asset_base64: await fileToBase64(assetForm.file),
        },
        csrfToken,
      );
      const refreshed = await getChallengeDraft(assetForm.draftId.trim());
      rememberDraft(refreshed.id);
      setDraft(refreshed);
      setMessage(`Uploaded private asset ${assetForm.assetId}.`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const rememberDraft = (id: string) => {
    window.localStorage.setItem(LAST_DRAFT_STORAGE_KEY, id);
    setDraftLookupId(id);
    setAssetForm((current) => ({ ...current, draftId: id }));
  };

  return (
    <div className="flex flex-col gap-6">
      <section className="card-elevated">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-6">
          <div>
            <span className="badge badge-validation mb-4">
              <GitPullRequest className="w-3 h-3" />
              Creator Observatory
            </span>
            <h1
              className="text-[var(--text-h1)] font-bold leading-[var(--leading-h1)]"
              style={{ fontFamily: "var(--font-serif)" }}
            >
              Challenge draft console
            </h1>
            <p className="mt-3 max-w-2xl text-[var(--text-body)] leading-[var(--leading-body)] text-[var(--text-secondary)]">
              Create GitHub-backed challenge drafts and upload private benchmark
              assets without using the admin identity model.
            </p>
          </div>
          <CreatorIdentityPanel
            creator={creator}
            loading={loading}
            onSignIn={signIn}
            onRefresh={refreshIdentity}
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

      <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
        <div className="flex flex-col gap-5">
          <form className="card flex flex-col gap-4" onSubmit={submitDraft}>
            <SectionTitle
              icon={<GitPullRequest className="w-4 h-4" />}
              title="Create draft"
            />
            <TextInput
              label="Repository URL"
              value={draftForm.repoUrl}
              onChange={(repoUrl) => setDraftForm({ ...draftForm, repoUrl })}
              required
            />
            <TextInput
              label="PR number"
              value={draftForm.prNumber}
              onChange={(prNumber) => setDraftForm({ ...draftForm, prNumber })}
              required
            />
            <TextInput
              label="PR URL"
              value={draftForm.prUrl}
              onChange={(prUrl) => setDraftForm({ ...draftForm, prUrl })}
              required
            />
            <TextInput
              label="Commit SHA"
              value={draftForm.commitSha}
              onChange={(commitSha) =>
                setDraftForm({ ...draftForm, commitSha })
              }
              required
            />
            <TextInput
              label="Challenge path"
              value={draftForm.challengePath}
              onChange={(challengePath) =>
                setDraftForm({ ...draftForm, challengePath })
              }
              required
            />
            <label className="flex flex-col gap-1">
              <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
                Manifest JSON
              </span>
              <textarea
                className="min-h-80 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 font-mono text-[var(--text-caption)] leading-relaxed outline-none focus:border-[var(--accent-primary-500)]"
                value={draftForm.manifestText}
                onChange={(event) =>
                  setDraftForm({
                    ...draftForm,
                    manifestText: event.target.value,
                  })
                }
                required
              />
            </label>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={loading}
            >
              <GitPullRequest className="w-4 h-4" />
              Create draft
            </button>
          </form>

          <form className="card flex flex-col gap-4" onSubmit={inspectDraft}>
            <SectionTitle
              icon={<RefreshCw className="w-4 h-4" />}
              title="Inspect draft"
            />
            <TextInput
              label="Draft ID"
              value={draftLookupId}
              onChange={setDraftLookupId}
              required
            />
            <button
              type="submit"
              className="btn btn-secondary"
              disabled={loading}
            >
              Load draft
            </button>
          </form>

          <form className="card flex flex-col gap-4" onSubmit={uploadAsset}>
            <SectionTitle
              icon={<UploadCloud className="w-4 h-4" />}
              title="Upload private asset"
            />
            <TextInput
              label="Draft ID"
              value={assetForm.draftId}
              onChange={(draftId) => setAssetForm({ ...assetForm, draftId })}
              required
            />
            <TextInput
              label="Asset ID"
              value={assetForm.assetId}
              onChange={(assetId) => setAssetForm({ ...assetForm, assetId })}
              required
            />
            <label className="flex flex-col gap-1">
              <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
                Asset kind
              </span>
              <select
                className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)] outline-none focus:border-[var(--accent-primary-500)]"
                value={assetForm.kind}
                onChange={(event) =>
                  setAssetForm({
                    ...assetForm,
                    kind: event.target.value as ChallengePrivateAssetKind,
                  })
                }
              >
                {assetKinds.map((kind) => (
                  <option key={kind} value={kind}>
                    {kind}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-2 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
              <input
                type="checkbox"
                checked={assetForm.required}
                onChange={(event) =>
                  setAssetForm({
                    ...assetForm,
                    required: event.target.checked,
                  })
                }
              />
              Required for publish
            </label>
            <input
              type="file"
              className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 text-[var(--text-body-sm)]"
              onChange={(event) =>
                setAssetForm({
                  ...assetForm,
                  file: event.target.files?.[0] ?? null,
                })
              }
              required
            />
            <button
              type="submit"
              className="btn btn-primary"
              disabled={loading}
            >
              <FileArchive className="w-4 h-4" />
              Upload asset
            </button>
          </form>
        </div>

        <DraftDetail draft={draft} />
      </section>
    </div>
  );
}

function CreatorIdentityPanel({
  creator,
  loading,
  onSignIn,
  onRefresh,
}: {
  creator: CreatorMeResponse | null;
  loading: boolean;
  onSignIn: () => Promise<void>;
  onRefresh: () => Promise<void>;
}) {
  return (
    <div className="card min-w-full lg:min-w-[360px] lg:max-w-[420px]">
      <div className="flex items-center gap-2 mb-4">
        <KeyRound className="w-4 h-4 text-[var(--accent-primary-text)]" />
        <h2 className="text-[var(--text-h3)] font-semibold">
          Creator identity
        </h2>
      </div>
      {creator ? (
        <div className="space-y-3">
          <div>
            <div className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
              GitHub account
            </div>
            <div className="font-mono text-[var(--text-body-sm)]">
              {creator.github_login} · {creator.github_user_id}
            </div>
          </div>
          <div>
            <div className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
              Agent ID
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
            Refresh
          </button>
        </div>
      ) : (
        <div className="space-y-4">
          <p className="text-[var(--text-body-sm)] text-[var(--text-secondary)]">
            GitHub OAuth is required before creating drafts or uploading private
            assets.
          </p>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void onSignIn()}
            disabled={loading}
          >
            <GitPullRequest className="w-4 h-4" />
            Sign in with GitHub
          </button>
        </div>
      )}
    </div>
  );
}

function DraftDetail({ draft }: { draft: ChallengeDraftResponse | null }) {
  if (!draft) {
    return (
      <div className="card">
        <div className="empty-state">
          Create or load a challenge draft to inspect its assets, validation
          records, and publication state.
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="card">
        <div className="flex flex-col md:flex-row md:items-start justify-between gap-4">
          <div>
            <div className="flex flex-wrap items-center gap-2 mb-3">
              <StatusBadge status={draft.status} />
              <span className="badge badge-default">{draft.request}</span>
            </div>
            <h2 className="text-[var(--text-h2)] font-semibold">
              {draft.manifest.title}
            </h2>
            <p className="mt-2 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
              {draft.manifest.summary}
            </p>
          </div>
          <a
            href={draft.pr_url}
            target="_blank"
            rel="noreferrer"
            className="btn btn-secondary"
          >
            <GitPullRequest className="w-4 h-4" />
            Open PR
          </a>
        </div>

        <dl className="mt-6 grid grid-cols-1 md:grid-cols-2 gap-4 text-[var(--text-body-sm)]">
          <Metadata label="Draft ID" value={draft.id} />
          <Metadata label="Challenge ID" value={draft.challenge_id} />
          <Metadata label="Creator" value={draft.creator_github_login} />
          <Metadata label="Commit" value={shortHash(draft.commit_sha)} />
          <Metadata
            label="Manifest hash"
            value={shortHash(draft.manifest_sha256)}
          />
          <Metadata
            label="Validation bundle"
            value={shortHash(draft.validation_bundle_sha256)}
          />
          <Metadata
            label="Approved bundle"
            value={shortHash(draft.approved_bundle_sha256)}
          />
          <Metadata
            label="Published challenge"
            value={draft.published_challenge_id ?? "—"}
          />
        </dl>
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<FileArchive className="w-4 h-4" />}
            title="Private assets"
          />
          <span className="badge badge-default">
            {draft.private_assets.length} rows
          </span>
        </div>
        {draft.private_assets.length === 0 ? (
          <div className="empty-state">No private assets uploaded.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Asset</th>
                <th>Kind</th>
                <th>Size</th>
                <th>SHA-256</th>
              </tr>
            </thead>
            <tbody>
              {draft.private_assets.map((asset) => (
                <tr key={asset.id}>
                  <td>
                    <div className="font-mono">{asset.asset_id}</div>
                    <div className="text-[var(--text-caption)] text-[var(--text-muted)]">
                      {asset.required ? "required" : "optional"}
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
            title="Validation records"
          />
          <span className="badge badge-default">
            {draft.validation_records.length} rows
          </span>
        </div>
        {draft.validation_records.length === 0 ? (
          <div className="empty-state">No validation records yet.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Status</th>
                <th>Message</th>
                <th>Bundle</th>
              </tr>
            </thead>
            <tbody>
              {draft.validation_records.map((record) => (
                <tr key={record.id}>
                  <td>
                    <StatusBadge status={record.status} />
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
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  required?: boolean;
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
      />
    </label>
  );
}

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

function StatusBadge({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const className =
    normalized === "published" ||
    normalized === "approved" ||
    normalized === "validated" ||
    normalized === "passed"
      ? "badge-success"
      : normalized === "rejected" || normalized === "failed"
        ? "badge-error"
        : normalized === "draft"
          ? "badge-warning"
          : "badge-default";

  return <span className={`badge ${className}`}>{status}</span>;
}

function shortHash(value: string | undefined): string {
  if (!value) {
    return "—";
  }
  return value.length > 16 ? value.slice(0, 16) : value;
}

async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const chunks: string[] = [];
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    chunks.push(
      String.fromCharCode(...bytes.subarray(index, index + chunkSize)),
    );
  }
  return btoa(chunks.join(""));
}

function creatorErrorMessage(error: unknown): string {
  if (error instanceof CreatorApiError) {
    if (error.status === 401) {
      return "Sign in with GitHub before continuing.";
    }
    return error.message;
  }
  if (error instanceof SyntaxError) {
    return "Manifest JSON is not valid JSON.";
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unknown creator console error.";
}
