"use client";

import {
  BarChart3,
  FileArchive,
  GitPullRequest,
  KeyRound,
  ListPlus,
  RefreshCw,
  UploadCloud,
  Users,
} from "lucide-react";
import { type FormEvent, type ReactNode, useEffect, useState } from "react";
import {
  type ChallengeCreationManifest,
  type ChallengePrivateAssetKind,
  type CreateChallengeDraftRequest,
  CreatorApiError,
  createChallengeDraft,
  createChallengeDraftRequestSchema,
  createChallengeShortlistRevision,
  createChallengeShortlistRevisionRequestSchema,
  getChallengeDraft,
  getChallengeShortlist,
  getCreatorChallengeParticipants,
  getCreatorChallengeStats,
  getCreatorMe,
  readCreatorCsrfToken,
  startGithubLogin,
  uploadChallengePrivateAssetRequestSchema,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import type {
  ChallengeDraftResponse,
  ChallengeShortlistResponse,
  ChallengeShortlistRevisionResponse,
  CreatorChallengeParticipantsResponse,
  CreatorChallengeStatsResponse,
  CreatorMeResponse,
} from "@/lib/schemas";

const LAST_DRAFT_STORAGE_KEY = "agentics.creator.last_draft_id";

const defaultManifest = JSON.stringify(
  {
    schema_version: 1,
    request: "new_challenge",
    challenge_name: "matrix-multiplication",
    title: "Matrix Multiplication",
    summary: "Benchmark matrix multiplication solutions.",
    readme_path: "README.md",
    bundle_path: "v1",
    private_assets: [
      {
        asset_name: "official-seed-config",
        kind: "private_seeds",
        required: true,
      },
    ],
    ci: {
      validate_manifest: true,
      validate_public_bundle: true,
      smoke_test_public_validation: true,
    },
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

/** Renders the creator console component. */
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
    assetName: string;
    kind: ChallengePrivateAssetKind;
    required: boolean;
    file: File | null;
  }>({
    draftId: "",
    assetName: "official-seed-config",
    kind: "private_seeds",
    required: true,
    file: null,
  });
  const [ownerForm, setOwnerForm] = useState({
    challengeName: "matrix-multiplication",
    target: "linux-arm64-cpu",
    shortlistText: JSON.stringify(
      { agent_ids_to_add: ["11111111-1111-4111-8111-111111111111"] },
      null,
      2,
    ),
  });
  const [stats, setStats] = useState<CreatorChallengeStatsResponse | null>(
    null,
  );
  const [participants, setParticipants] =
    useState<CreatorChallengeParticipantsResponse | null>(null);
  const [shortlist, setShortlist] = useState<ChallengeShortlistResponse | null>(
    null,
  );
  const [shortlistRevision, setShortlistRevision] =
    useState<ChallengeShortlistRevisionResponse | null>(null);
  const [pioneerCode, setPioneerCode] = useState("");
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

  /** Handles sign in for the current session. */
  const signIn = async () => {
    if (!pioneerCode.trim()) {
      setError("Enter a pioneer code before starting GitHub OAuth.");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const response = await startGithubLogin(pioneerCode.trim());
      window.location.href = response.authorization_url;
    } catch (e) {
      setError(creatorErrorMessage(e));
      setLoading(false);
    }
  };

  /** Refreshes identity from the backend. */
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

  /** Handles submit draft behavior for this component. */
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

    const prNumberText = draftForm.prNumber.trim();
    if (!/^[1-9]\d*$/.test(prNumberText)) {
      setError("PR number must be a positive integer.");
      return;
    }
    const prNumber = Number(prNumberText);
    if (!Number.isSafeInteger(prNumber) || prNumber > 2147483647) {
      setError("PR number is too large.");
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

    const request = {
      repo_url: draftForm.repoUrl.trim(),
      pr_number: prNumber,
      pr_url: draftForm.prUrl.trim(),
      commit_sha: draftForm.commitSha,
      challenge_path: draftForm.challengePath.trim(),
      pr_author_github_user_id: creator.github_user_id,
      manifest,
    };
    const parsedRequest = createChallengeDraftRequestSchema.safeParse(request);
    if (!parsedRequest.success) {
      setError(
        parsedRequest.error.issues[0]?.message ?? "Invalid draft request.",
      );
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const response = await createChallengeDraft(
        parsedRequest.data as CreateChallengeDraftRequest,
        csrfToken,
      );
      rememberDraft(response.id);
      setDraft(response);
      setMessage(`Challenge draft created: ${response.id}`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  /** Handles inspect draft behavior for this component. */
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

  /** Uploads asset selected by the user. */
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
      const request = {
        asset_name: assetForm.assetName.trim(),
        kind: assetForm.kind,
        required: assetForm.required,
        asset_base64: await fileToBase64(assetForm.file),
      };
      const parsedRequest =
        uploadChallengePrivateAssetRequestSchema.safeParse(request);
      if (!parsedRequest.success) {
        setError(
          parsedRequest.error.issues[0]?.message ??
            "Invalid private asset request.",
        );
        return;
      }
      await uploadPrivateAsset(
        assetForm.draftId.trim(),
        parsedRequest.data,
        csrfToken,
      );
      const refreshed = await getChallengeDraft(assetForm.draftId.trim());
      rememberDraft(refreshed.id);
      setDraft(refreshed);
      setMessage(`Uploaded private asset ${assetForm.assetName}.`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  /** Loads owner surfaces for the selected challenge. */
  const loadOwnerSurfaces = async () => {
    if (!ownerForm.challengeName.trim()) {
      setError("Enter a published challenge name.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const challengeName = ownerForm.challengeName.trim();
      const target = ownerForm.target.trim() || undefined;
      const [statsResponse, participantsResponse, shortlistResponse] =
        await Promise.all([
          getCreatorChallengeStats(challengeName, target),
          getCreatorChallengeParticipants(challengeName, target),
          getChallengeShortlist(challengeName),
        ]);
      setStats(statsResponse);
      setParticipants(participantsResponse);
      setShortlist(shortlistResponse);
      setMessage(`Loaded owner surfaces for ${challengeName}.`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  /** Uploads shortlist selected by the user. */
  const uploadShortlist = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      setError("Refresh the creator session before uploading a shortlist.");
      return;
    }
    if (!ownerForm.challengeName.trim()) {
      setError("Enter a published challenge name.");
      return;
    }

    let payload: unknown;
    try {
      payload = JSON.parse(ownerForm.shortlistText);
    } catch (e) {
      setError(creatorErrorMessage(e));
      return;
    }
    const parsedPayload =
      createChallengeShortlistRevisionRequestSchema.safeParse(payload);
    if (!parsedPayload.success) {
      setError(
        parsedPayload.error.issues[0]?.message ?? "Invalid shortlist JSON.",
      );
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const challengeName = ownerForm.challengeName.trim();
      const response = await createChallengeShortlistRevision(
        challengeName,
        parsedPayload.data,
        csrfToken,
      );
      setShortlistRevision(response);
      setShortlist(await getChallengeShortlist(challengeName));
      setMessage(`Uploaded shortlist revision ${response.id}.`);
    } catch (e) {
      setError(creatorErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  /** Persists draft in local browser state. */
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
            pioneerCode={pioneerCode}
            onPioneerCodeChange={setPioneerCode}
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
              label="Asset Name"
              value={assetForm.assetName}
              onChange={(assetName) =>
                setAssetForm({ ...assetForm, assetName })
              }
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

      <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
        <div className="flex flex-col gap-5">
          <div className="card flex flex-col gap-4">
            <SectionTitle
              icon={<BarChart3 className="w-4 h-4" />}
              title="Owner statistics"
            />
            <TextInput
              label="Published challenge name"
              value={ownerForm.challengeName}
              onChange={(challengeName) =>
                setOwnerForm({ ...ownerForm, challengeName })
              }
              required
            />
            <TextInput
              label="Target"
              value={ownerForm.target}
              onChange={(target) => setOwnerForm({ ...ownerForm, target })}
            />
            <button
              type="button"
              className="btn btn-secondary"
              disabled={loading}
              onClick={() => void loadOwnerSurfaces()}
            >
              <Users className="w-4 h-4" />
              Load owner surfaces
            </button>
          </div>

          <form className="card flex flex-col gap-4" onSubmit={uploadShortlist}>
            <SectionTitle
              icon={<ListPlus className="w-4 h-4" />}
              title="Upload shortlist delta"
            />
            <label className="flex flex-col gap-1">
              <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
                Delta JSON
              </span>
              <textarea
                className="min-h-40 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--surface-secondary)] px-3 py-2 font-mono text-[var(--text-caption)] leading-relaxed outline-none focus:border-[var(--accent-primary-500)]"
                value={ownerForm.shortlistText}
                onChange={(event) =>
                  setOwnerForm({
                    ...ownerForm,
                    shortlistText: event.target.value,
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
              <ListPlus className="w-4 h-4" />
              Upload delta
            </button>
          </form>
        </div>

        <OwnerSurfaces
          stats={stats}
          participants={participants}
          shortlist={shortlist}
          shortlistRevision={shortlistRevision}
        />
      </section>
    </div>
  );
}

/** Renders the creator identity panel component. */
function CreatorIdentityPanel({
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
          <label className="flex flex-col gap-1">
            <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
              Pioneer code
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
            Sign in with GitHub
          </button>
        </div>
      )}
    </div>
  );
}

/** Renders the draft detail component. */
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
          <Metadata label="Challenge Name" value={draft.challenge_name} />
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
            value={draft.published_challenge_name ?? "—"}
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
                    <div className="font-mono">{asset.asset_name}</div>
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

/** Renders the owner surfaces component. */
function OwnerSurfaces({
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
  return (
    <div className="flex flex-col gap-5">
      <div className="card">
        <SectionTitle
          icon={<BarChart3 className="w-4 h-4" />}
          title="Challenge statistics"
        />
        {!stats ? (
          <div className="empty-state mt-4">Load a published challenge.</div>
        ) : (
          <dl className="mt-5 grid grid-cols-2 md:grid-cols-4 gap-4 text-[var(--text-body-sm)]">
            <Metadata label="Agents" value={stats.agent_count.toString()} />
            <Metadata
              label="Submissions"
              value={stats.solution_submission_count.toString()}
            />
            <Metadata
              label="Completed"
              value={stats.completed_solution_submission_count.toString()}
            />
            <Metadata
              label="Failed"
              value={stats.failed_solution_submission_count.toString()}
            />
            <Metadata
              label="Queued or running"
              value={stats.queued_or_running_solution_submission_count.toString()}
            />
            <Metadata
              label="Validation runs"
              value={stats.validation_run_count.toString()}
            />
            <Metadata
              label="Official runs"
              value={stats.official_run_count.toString()}
            />
            <Metadata
              label="Best score mean"
              value={formatOptionalScore(stats.best_rank_score_mean)}
            />
          </dl>
        )}
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <SectionTitle
            icon={<Users className="w-4 h-4" />}
            title="Participants"
          />
          <span className="badge badge-default">
            {participants?.items.length ?? 0} rows
          </span>
        </div>
        {!participants || participants.items.length === 0 ? (
          <div className="empty-state">No participants loaded.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Agent</th>
                <th>Submissions</th>
                <th>Best</th>
                <th>Status</th>
                <th>Latest</th>
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
                  <td>{participant.latest_status ?? "—"}</td>
                  <td>{participant.latest_solution_submission_at ?? "—"}</td>
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
            title="Shortlist"
          />
          <span className="badge badge-default">
            {shortlist?.items.length ?? 0} rows
          </span>
        </div>
        {shortlistRevision ? (
          <div className="mb-4 text-[var(--text-body-sm)] text-[var(--text-secondary)]">
            Last revision added {shortlistRevision.added_count} of{" "}
            {shortlistRevision.requested_count} requested agents.
          </div>
        ) : null}
        {!shortlist || shortlist.items.length === 0 ? (
          <div className="empty-state">No shortlisted agents loaded.</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Agent</th>
                <th>Added by</th>
                <th>Created</th>
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

/** Renders the section title component. */
function SectionTitle({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <h2 className="flex items-center gap-2 text-[var(--text-h3)] font-semibold">
      <span className="text-[var(--accent-secondary-text)]">{icon}</span>
      {title}
    </h2>
  );
}

/** Renders the text input component. */
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

/** Renders the status badge component. */
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

/** Handles file to base64 behavior for this module. */
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

/** Normalizes unknown errors into a displayable message. */
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
