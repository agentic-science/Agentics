"use client";

import {
  BarChart3,
  FileArchive,
  GitPullRequest,
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
import {
  CreatorIdentityPanel,
  DraftDetail,
  OwnerSurfaces,
} from "@/components/creator/CreatorPanels";
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
  startGithubLogin,
  uploadChallengePrivateAssetRequestSchema,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import {
  type CreatorOwnerScope,
  fetchCreatorOwnerBundle,
  mutateCreatorDraft,
  mutateCreatorOwnerBundle,
  useCreatorDraft,
  useCreatorOwnerBundle,
  useCreatorSession,
} from "@/lib/creatorData";
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
    challengeName: "",
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
  const [ownerScope, setOwnerScope] = useState<CreatorOwnerScope | null>(null);
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
  const creatorSession = useCreatorSession();
  const draftResource = useCreatorDraft(draftLookupId);
  const ownerBundle = useCreatorOwnerBundle(ownerScope);

  useEffect(() => {
    const lastDraftId = window.localStorage.getItem(LAST_DRAFT_STORAGE_KEY);
    if (lastDraftId) {
      setDraftLookupId(lastDraftId);
      setAssetForm((current) => ({ ...current, draftId: lastDraftId }));
    }
  }, []);

  useEffect(() => {
    if (creatorSession.session) {
      setCreator(creatorSession.session);
      setCsrfToken(creatorSession.session.csrf_token);
    } else if (creatorSession.error) {
      setCreator(null);
      setCsrfToken("");
    }
  }, [creatorSession.error, creatorSession.session]);

  useEffect(() => {
    if (draftResource.draft) {
      setDraft(draftResource.draft);
    }
  }, [draftResource.draft]);

  useEffect(() => {
    if (ownerBundle.bundle) {
      setStats(ownerBundle.bundle.stats);
      setParticipants(ownerBundle.bundle.participants);
      setShortlist(ownerBundle.bundle.shortlist);
    }
  }, [ownerBundle.bundle]);

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
      const session = await creatorSession.mutate();
      if (!session) {
        throw new Error(t("messages.signInBeforeContinue"));
      }
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
      await mutateCreatorDraft(refreshed.id);
      setMessage(t("messages.assetUploaded", { name: assetForm.assetName }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setLoading(false);
    }
  };

  /** Loads owner surfaces for the selected challenge. */
  const loadOwnerSurfaces = async () => {
    if (!ownerForm.challengeName.trim()) {
      setError(t("messages.enterChallenge"));
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const challengeName = ownerForm.challengeName.trim();
      const target = ownerForm.target.trim() || undefined;
      const scope = { challengeName, target };
      setOwnerScope(scope);
      const bundle = await fetchCreatorOwnerBundle(scope);
      setStats(bundle.stats);
      setParticipants(bundle.participants);
      setShortlist(bundle.shortlist);
      setMessage(t("messages.ownerLoaded", { id: challengeName }));
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
    if (!ownerForm.challengeName.trim()) {
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
      const challengeName = ownerForm.challengeName.trim();
      const response = await createChallengeShortlistRevision(
        challengeName,
        parsedPayload.data,
        csrfToken,
      );
      setShortlistRevision(response);
      const scope = {
        challengeName,
        target: ownerForm.target.trim() || undefined,
      };
      setOwnerScope(scope);
      await mutateCreatorOwnerBundle(scope);
      const bundle = await fetchCreatorOwnerBundle(scope);
      setShortlist(bundle.shortlist);
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
              label={t("owner.publishedChallengeName")}
              value={ownerForm.challengeName}
              onChange={(challengeName) =>
                setOwnerForm({ ...ownerForm, challengeName })
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
