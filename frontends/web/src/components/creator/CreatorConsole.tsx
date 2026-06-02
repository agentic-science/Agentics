"use client";

import { GitPullRequest } from "lucide-react";
import { useTranslations } from "next-intl";
import { type FormEvent, useEffect, useState } from "react";
import {
  type CreatorOwnerFormState,
  type CreatorPrivateAssetFormState,
  type CreatorReviewRecordFormState,
  defaultCreatorManifest,
  OwnerStatsForm,
  PrivateAssetUploadForm,
  ReviewRecordCreateForm,
  ReviewRecordInspectForm,
  ShortlistUploadForm,
} from "@/components/creator/CreatorForms";
import {
  CreatorIdentityPanel,
  OwnerSurfaces,
  ReviewRecordDetail,
} from "@/components/creator/CreatorPanels";
import {
  type ChallengeCreationManifest,
  type CreateChallengeReviewRecordRequest,
  CreatorApiError,
  createChallengeReviewRecord,
  createChallengeReviewRecordRequestSchema,
  createChallengeShortlistRevision,
  createChallengeShortlistRevisionRequestSchema,
  getChallengeReviewRecord,
  startGithubLogin,
  uploadChallengePrivateAssetRequestSchema,
  uploadPrivateAsset,
} from "@/lib/creatorApi";
import {
  type CreatorOwnerScope,
  fetchCreatorOwnerBundle,
  mutateCreatorOwnerBundle,
  mutateCreatorReviewRecord,
  useCreatorOwnerBundle,
  useCreatorReviewRecord,
  useCreatorSession,
} from "@/lib/creatorData";
import type {
  ChallengeShortlistResponse,
  ChallengeShortlistRevisionResponse,
  CreatorChallengeParticipantsResponse,
  CreatorChallengeReviewRecordResponse,
  CreatorChallengeStatsResponse,
  CreatorMeResponse,
} from "@/lib/schemas";

const LAST_REVIEW_RECORD_STORAGE_KEY = "agentics.creator.last_review_record_id";
type CreatorPendingAction =
  | "createReviewRecord"
  | "inspectReviewRecord"
  | "loadOwner"
  | "refreshIdentity"
  | "signIn"
  | "uploadAsset"
  | "uploadShortlist";

/** Renders the creator console component. */
export function CreatorConsole() {
  const t = useTranslations("creator");
  const [creator, setCreator] = useState<CreatorMeResponse | null>(null);
  const [csrfToken, setCsrfToken] = useState("");
  const [reviewRecord, setReviewRecord] =
    useState<CreatorChallengeReviewRecordResponse | null>(null);
  const [reviewRecordLookupId, setReviewRecordLookupId] = useState("");
  const [reviewRecordForm, setReviewRecordForm] =
    useState<CreatorReviewRecordFormState>({
      repoUrl: "https://github.com/agentics-reifying/agentics-challenges",
      prNumber: "",
      prUrl: "",
      commitSha: "",
      challengePath: "challenges/frontier-cs-example-challenge",
      manifestText: defaultCreatorManifest,
    });
  const [assetForm, setAssetForm] = useState<CreatorPrivateAssetFormState>({
    reviewRecordId: "",
    assetName: "official-seed-config",
    kind: "private_seeds",
    required: true,
    file: null,
  });
  const [ownerForm, setOwnerForm] = useState<CreatorOwnerFormState>({
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
  const [pendingAction, setPendingAction] =
    useState<CreatorPendingAction | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const displayCreatorError = (error: unknown) =>
    creatorErrorMessage(error, {
      signIn: t("messages.signInBeforeContinue"),
      invalidJson: t("messages.invalidManifestJson"),
      unknown: t("messages.unknown"),
    });
  const creatorSession = useCreatorSession();
  const reviewRecordResource = useCreatorReviewRecord(reviewRecordLookupId);
  const ownerBundle = useCreatorOwnerBundle(ownerScope);

  useEffect(() => {
    const lastReviewRecordId = window.localStorage.getItem(
      LAST_REVIEW_RECORD_STORAGE_KEY,
    );
    if (lastReviewRecordId) {
      setReviewRecordLookupId(lastReviewRecordId);
      setAssetForm((current) => ({
        ...current,
        reviewRecordId: lastReviewRecordId,
      }));
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
    if (reviewRecordResource.reviewRecord) {
      setReviewRecord(reviewRecordResource.reviewRecord);
    }
  }, [reviewRecordResource.reviewRecord]);

  useEffect(() => {
    if (ownerBundle.bundle) {
      setStats(ownerBundle.bundle.stats);
      setParticipants(ownerBundle.bundle.participants);
      setShortlist(ownerBundle.bundle.shortlist);
    }
  }, [ownerBundle.bundle]);

  /** Handles sign in for the current session. */
  const signIn = async () => {
    setPendingAction("signIn");
    setError(null);
    try {
      const response = await startGithubLogin(pioneerCode.trim());
      window.location.href = response.authorization_url;
    } catch (e) {
      setError(displayCreatorError(e));
      setPendingAction(null);
    }
  };

  /** Refreshes identity from the backend. */
  const refreshIdentity = async () => {
    setPendingAction("refreshIdentity");
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
      setPendingAction(null);
    }
  };

  /** Handles submit review record behavior for this component. */
  const submitReviewRecord = async (event: FormEvent) => {
    event.preventDefault();
    if (!creator) {
      setError(t("messages.signInBeforeReviewRecord"));
      return;
    }
    if (!csrfToken) {
      setError(t("messages.refreshBeforeReviewRecord"));
      return;
    }

    const prNumberText = reviewRecordForm.prNumber.trim();
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
        reviewRecordForm.manifestText,
      ) as ChallengeCreationManifest;
    } catch (e) {
      setError(displayCreatorError(e));
      return;
    }

    const request = {
      repo_url: reviewRecordForm.repoUrl.trim(),
      pr_number: prNumber,
      pr_url: reviewRecordForm.prUrl.trim(),
      commit_sha: reviewRecordForm.commitSha,
      challenge_path: reviewRecordForm.challengePath.trim(),
      pr_author_github_user_id: creator.github_user_id,
      manifest,
    };
    const parsedRequest =
      createChallengeReviewRecordRequestSchema.safeParse(request);
    if (!parsedRequest.success) {
      setError(
        parsedRequest.error.issues[0]?.message ??
          t("messages.invalidReviewRecord"),
      );
      return;
    }

    setPendingAction("createReviewRecord");
    setError(null);
    try {
      const response = await createChallengeReviewRecord(
        parsedRequest.data as CreateChallengeReviewRecordRequest,
        csrfToken,
      );
      rememberReviewRecord(response.id);
      setReviewRecord(response);
      setMessage(t("messages.reviewRecordCreated", { id: response.id }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setPendingAction(null);
    }
  };

  /** Handles inspect review record behavior for this component. */
  const inspectReviewRecord = async (event: FormEvent) => {
    event.preventDefault();
    if (!reviewRecordLookupId.trim()) {
      setError(t("messages.enterReviewRecord"));
      return;
    }

    setPendingAction("inspectReviewRecord");
    setError(null);
    try {
      const response = await getChallengeReviewRecord(
        reviewRecordLookupId.trim(),
      );
      rememberReviewRecord(response.id);
      setReviewRecord(response);
      setMessage(t("messages.reviewRecordLoaded", { id: response.id }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setPendingAction(null);
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

    setPendingAction("uploadAsset");
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
        assetForm.reviewRecordId.trim(),
        parsedRequest.data,
        csrfToken,
      );
      const refreshed = await getChallengeReviewRecord(
        assetForm.reviewRecordId.trim(),
      );
      rememberReviewRecord(refreshed.id);
      setReviewRecord(refreshed);
      await mutateCreatorReviewRecord(refreshed.id);
      setMessage(t("messages.assetUploaded", { name: assetForm.assetName }));
    } catch (e) {
      setError(displayCreatorError(e));
    } finally {
      setPendingAction(null);
    }
  };

  /** Loads owner surfaces for the selected challenge. */
  const loadOwnerSurfaces = async () => {
    if (!ownerForm.challengeName.trim()) {
      setError(t("messages.enterChallenge"));
      return;
    }

    setPendingAction("loadOwner");
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
      setPendingAction(null);
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

    setPendingAction("uploadShortlist");
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
      setPendingAction(null);
    }
  };

  /** Persists review record in local browser state. */
  const rememberReviewRecord = (id: string) => {
    window.localStorage.setItem(LAST_REVIEW_RECORD_STORAGE_KEY, id);
    setReviewRecordLookupId(id);
    setAssetForm((current) => ({ ...current, reviewRecordId: id }));
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
              className="text-h1 font-bold leading-h1"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {t("hero.title")}
            </h1>
            <p className="mt-3 max-w-2xl text-body leading-body text-fg-secondary">
              {t("hero.description")}
            </p>
          </div>
          <CreatorIdentityPanel
            creator={creator}
            loading={
              pendingAction === "signIn" || pendingAction === "refreshIdentity"
            }
            pioneerCode={pioneerCode}
            onPioneerCodeChange={setPioneerCode}
            onSignIn={signIn}
            onRefresh={refreshIdentity}
          />
        </div>
      </section>

      {error ? (
        <div className="card border-danger/40 text-danger">{error}</div>
      ) : null}
      {message ? (
        <div className="card border-success/30 text-success">{message}</div>
      ) : null}

      <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
        <div className="flex flex-col gap-5">
          <ReviewRecordCreateForm
            reviewRecordForm={reviewRecordForm}
            setReviewRecordForm={setReviewRecordForm}
            loading={pendingAction === "createReviewRecord"}
            onSubmit={submitReviewRecord}
          />

          <ReviewRecordInspectForm
            reviewRecordLookupId={reviewRecordLookupId}
            setReviewRecordLookupId={setReviewRecordLookupId}
            loading={pendingAction === "inspectReviewRecord"}
            onSubmit={inspectReviewRecord}
          />

          <PrivateAssetUploadForm
            assetForm={assetForm}
            setAssetForm={setAssetForm}
            loading={pendingAction === "uploadAsset"}
            onSubmit={uploadAsset}
          />
        </div>

        <ReviewRecordDetail reviewRecord={reviewRecord} />
      </section>

      <section className="grid grid-cols-1 xl:grid-cols-[420px_1fr] gap-6">
        <div className="flex flex-col gap-5">
          <OwnerStatsForm
            ownerForm={ownerForm}
            setOwnerForm={setOwnerForm}
            loading={pendingAction === "loadOwner"}
            onLoad={loadOwnerSurfaces}
          />

          <ShortlistUploadForm
            ownerForm={ownerForm}
            setOwnerForm={setOwnerForm}
            loading={pendingAction === "uploadShortlist"}
            onSubmit={uploadShortlist}
          />
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
