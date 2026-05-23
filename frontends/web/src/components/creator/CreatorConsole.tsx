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
import { useTranslations } from "next-intl";
import { type FormEvent, useEffect, useState } from "react";
import {
  ConsoleSectionTitle as SectionTitle,
  ConsoleTextInput as TextInput,
} from "@/components/ConsolePrimitives";
import { StatusBadge } from "@/components/StatusBadge";
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
  getCreatorSession,
  startGithubLogin,
  uploadChallengePrivateAssetRequestSchema,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import { selectLocalizedText } from "@/lib/localizedText";
import type {
  ChallengeShortlistResponse,
  ChallengeShortlistRevisionResponse,
  CreatorChallengeDraftResponse,
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
    summary: {
      en: "Benchmark matrix multiplication solutions.",
      zh: "评测矩阵乘法解决方案。",
    },
    keywords: ["linear algebra", "performance", "matrix"],
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
  "private_evaluator_package",
  "private_seeds",
  "private_reference_outputs",
];

/** Renders the creator console component. */
export function CreatorConsole() {
  const t = useTranslations("creator");
  const [creator, setCreator] = useState<CreatorMeResponse | null>(null);
  const [csrfToken, setCsrfToken] = useState("");
  const [draft, setDraft] = useState<CreatorChallengeDraftResponse | null>(
    null,
  );
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
    challengeId: "",
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
  const displayCreatorError = (error: unknown) =>
    creatorErrorMessage(error, {
      signIn: t("messages.signInBeforeContinue"),
      invalidJson: t("messages.invalidManifestJson"),
      unknown: t("messages.unknown"),
    });

  useEffect(() => {
    const lastDraftId = window.localStorage.getItem(LAST_DRAFT_STORAGE_KEY);
    if (lastDraftId) {
      setDraftLookupId(lastDraftId);
      setAssetForm((current) => ({ ...current, draftId: lastDraftId }));
    }

    void getCreatorSession()
      .then((session) => {
        setCreator(session);
        setCsrfToken(session.csrf_token);
      })
      .catch(() => {
        setCreator(null);
        setCsrfToken("");
      });
  }, []);

  /** Handles sign in for the current session. */
  const signIn = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await startGithubLogin(pioneerCode.trim());
      window.location.href = response.authorization_url;
    } catch (e) {
      setError(displayCreatorError(e));
      setLoading(false);
    }
  };

  /** Refreshes identity from the backend. */
  const refreshIdentity = async () => {
    setLoading(true);
    setError(null);
    try {
      const session = await getCreatorSession();
      setCreator(session);
      setCsrfToken(session.csrf_token);
      setMessage(t("messages.identityRefreshed"));
    } catch (e) {
      setCreator(null);
      setError(displayCreatorError(e));
    } finally {
      setLoading(false);
    }
  };

  /** Handles submit draft behavior for this component. */
  const submitDraft = async (event: FormEvent) => {
    event.preventDefault();
    if (!creator) {
      setError(t("messages.signInBeforeDraft"));
      return;
    }
    if (!csrfToken) {
      setError(t("messages.refreshBeforeDraft"));
      return;
    }

    const prNumberText = draftForm.prNumber.trim();
    if (!/^[1-9]\d*$/.test(prNumberText)) {
      setError(t("messages.prNumberInvalid"));
      return;
    }
    const prNumber = Number(prNumberText);
    if (!Number.isSafeInteger(prNumber) || prNumber > 2147483647) {
      setError(t("messages.prNumberTooLarge"));
      return;
    }

    let manifest: ChallengeCreationManifest;
    try {
      manifest = JSON.parse(
        draftForm.manifestText,
      ) as ChallengeCreationManifest;
    } catch (e) {
      setError(displayCreatorError(e));
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
        parsedRequest.error.issues[0]?.message ?? t("messages.invalidDraft"),
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
      setMessage(t("messages.draftCreated", { id: response.id }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setLoading(false);
    }
  };

  /** Handles inspect draft behavior for this component. */
  const inspectDraft = async (event: FormEvent) => {
    event.preventDefault();
    if (!draftLookupId.trim()) {
      setError(t("messages.enterDraft"));
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const response = await getChallengeDraft(draftLookupId.trim());
      rememberDraft(response.id);
      setDraft(response);
      setMessage(t("messages.draftLoaded", { id: response.id }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setLoading(false);
    }
  };

  /** Uploads asset selected by the user. */
  const uploadAsset = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      setError(t("messages.refreshBeforeAsset"));
      return;
    }
    if (!assetForm.file) {
      setError(t("messages.chooseZip"));
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
          parsedRequest.error.issues[0]?.message ?? t("messages.invalidAsset"),
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
      setMessage(t("messages.assetUploaded", { name: assetForm.assetName }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setLoading(false);
    }
  };

  /** Loads owner surfaces for the selected challenge. */
  const loadOwnerSurfaces = async () => {
    if (!ownerForm.challengeId.trim()) {
      setError(t("messages.enterChallenge"));
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const challengeId = ownerForm.challengeId.trim();
      const target = ownerForm.target.trim() || undefined;
      const [statsResponse, participantsResponse, shortlistResponse] =
        await Promise.all([
          getCreatorChallengeStats(challengeId, target),
          getCreatorChallengeParticipants(challengeId, target),
          getChallengeShortlist(challengeId),
        ]);
      setStats(statsResponse);
      setParticipants(participantsResponse);
      setShortlist(shortlistResponse);
      setMessage(t("messages.ownerLoaded", { id: challengeId }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setLoading(false);
    }
  };

  /** Uploads shortlist selected by the user. */
  const uploadShortlist = async (event: FormEvent) => {
    event.preventDefault();
    if (!csrfToken) {
      setError(t("messages.refreshBeforeShortlist"));
      return;
    }
    if (!ownerForm.challengeId.trim()) {
      setError(t("messages.enterChallenge"));
      return;
    }

    let payload: unknown;
    try {
      payload = JSON.parse(ownerForm.shortlistText);
    } catch (e) {
      setError(displayCreatorError(e));
      return;
    }
    const parsedPayload =
      createChallengeShortlistRevisionRequestSchema.safeParse(payload);
    if (!parsedPayload.success) {
      setError(
        parsedPayload.error.issues[0]?.message ??
          t("messages.invalidShortlist"),
      );
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const challengeId = ownerForm.challengeId.trim();
      const response = await createChallengeShortlistRevision(
        challengeId,
        parsedPayload.data,
        csrfToken,
      );
      setShortlistRevision(response);
      setShortlist(await getChallengeShortlist(challengeId));
      setMessage(t("messages.shortlistUploaded", { id: response.id }));
    } catch (e) {
      setError(displayCreatorError(e));
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
              {t("hero.badge")}
            </span>
            <h1
              className="text-[var(--text-h1)] font-bold leading-[var(--leading-h1)]"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {t("hero.title")}
            </h1>
            <p className="mt-3 max-w-2xl text-[var(--text-body)] leading-[var(--leading-body)] text-[var(--text-secondary)]">
              {t("hero.description")}
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
              title={t("draft.create")}
            />
            <TextInput
              label={t("draft.repositoryUrl")}
              value={draftForm.repoUrl}
              onChange={(repoUrl) => setDraftForm({ ...draftForm, repoUrl })}
              required
            />
            <TextInput
              label={t("draft.prNumber")}
              value={draftForm.prNumber}
              onChange={(prNumber) => setDraftForm({ ...draftForm, prNumber })}
              required
            />
            <TextInput
              label={t("draft.prUrl")}
              value={draftForm.prUrl}
              onChange={(prUrl) => setDraftForm({ ...draftForm, prUrl })}
              required
            />
            <TextInput
              label={t("draft.commitSha")}
              value={draftForm.commitSha}
              onChange={(commitSha) =>
                setDraftForm({ ...draftForm, commitSha })
              }
              required
            />
            <TextInput
              label={t("draft.challengePath")}
              value={draftForm.challengePath}
              onChange={(challengePath) =>
                setDraftForm({ ...draftForm, challengePath })
              }
              required
            />
            <label className="flex flex-col gap-1">
              <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
                {t("draft.manifestJson")}
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
              {t("draft.create")}
            </button>
          </form>

          <form className="card flex flex-col gap-4" onSubmit={inspectDraft}>
            <SectionTitle
              icon={<RefreshCw className="w-4 h-4" />}
              title={t("draft.inspect")}
            />
            <TextInput
              label={t("draft.draftId")}
              value={draftLookupId}
              onChange={setDraftLookupId}
              required
            />
            <button
              type="submit"
              className="btn btn-secondary"
              disabled={loading}
            >
              {t("draft.load")}
            </button>
          </form>

          <form className="card flex flex-col gap-4" onSubmit={uploadAsset}>
            <SectionTitle
              icon={<UploadCloud className="w-4 h-4" />}
              title={t("draft.uploadPrivateAsset")}
            />
            <TextInput
              label={t("draft.draftId")}
              value={assetForm.draftId}
              onChange={(draftId) => setAssetForm({ ...assetForm, draftId })}
              required
            />
            <TextInput
              label={t("draft.assetName")}
              value={assetForm.assetName}
              onChange={(assetName) =>
                setAssetForm({ ...assetForm, assetName })
              }
              required
            />
            <label className="flex flex-col gap-1">
              <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
                {t("draft.assetKind")}
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
              {t("draft.requiredForPublish")}
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
              {t("draft.uploadAsset")}
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
              title={t("owner.statisticsForm")}
            />
            <TextInput
              label={t("owner.publishedChallengeId")}
              value={ownerForm.challengeId}
              onChange={(challengeId) =>
                setOwnerForm({ ...ownerForm, challengeId })
              }
              required
            />
            <TextInput
              label={t("owner.target")}
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
              {t("owner.load")}
            </button>
          </div>

          <form className="card flex flex-col gap-4" onSubmit={uploadShortlist}>
            <SectionTitle
              icon={<ListPlus className="w-4 h-4" />}
              title={t("owner.uploadShortlist")}
            />
            <label className="flex flex-col gap-1">
              <span className="text-[var(--text-caption)] uppercase tracking-wide text-[var(--text-muted)]">
                {t("owner.deltaJson")}
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
              {t("owner.uploadDelta")}
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
function DraftDetail({
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
            label={t("publishedChallenge")}
            value={draft.published_challenge_name ?? "—"}
          />
          <Metadata
            label={t("publishedChallengeId")}
            value={draft.published_challenge_id ?? "—"}
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

/** Returns the browser document locale without requiring a Next Intl provider. */
function currentDocumentLocale(): string {
  return document.documentElement.lang || navigator.language || "en";
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
function creatorErrorMessage(
  error: unknown,
  fallback: { signIn: string; invalidJson: string; unknown: string },
): string {
  if (error instanceof CreatorApiError) {
    if (error.status === 401) {
      return fallback.signIn;
    }
    return error.message;
  }
  if (error instanceof SyntaxError) {
    return fallback.invalidJson;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback.unknown;
}
