"use client";

import { useTranslations } from "next-intl";
import { useState } from "react";
import type { ZodType } from "zod";
import { adminErrorMessage } from "@/components/admin/errors";
import {
  AdminApiError,
  adminFetchJson,
  listAdminChallengeDraftPrivateAssets,
} from "@/lib/adminApi";
import {
  type AdminChallengePrivateAssetListResponse,
  type ChallengeDraftListItem,
  challengeDraftCleanupResponseSchema,
  challengeDraftResponseSchema,
  type ReviewChallengeDraftRequest,
  reviewChallengeDraftRequestSchema,
  type ValidateChallengeDraftRequest,
  validateChallengeDraftRequestSchema,
} from "@/lib/schemas";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;
type DraftAction = "validate" | "approve" | "publish" | "reject" | "abandon";
type DraftReviewTranslator = ReturnType<typeof useTranslations>;

interface UseChallengeDraftReviewActionsParams {
  csrfToken: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

/** Owns admin draft-review mutation state and refresh behavior. */
export function useChallengeDraftReviewActions({
  csrfToken,
  onRefresh,
  onError,
  onMessage,
}: UseChallengeDraftReviewActionsParams) {
  const t = useTranslations("admin.draftReview");
  const [repositoryPath, setRepositoryPath] = useState(
    "challenge-repos/agentics-challenges",
  );
  const [reviewMessage, setReviewMessage] = useState("");
  const [busyDraftId, setBusyDraftId] = useState<string | null>(null);
  const [expandedDraftId, setExpandedDraftId] = useState<string | null>(null);
  const [assetRowsByDraftId, setAssetRowsByDraftId] = useState<
    Record<string, AdminChallengePrivateAssetListResponse>
  >({});
  const [loadingAssetsDraftId, setLoadingAssetsDraftId] = useState<
    string | null
  >(null);

  const toggleAssetRows = async (draftId: string) => {
    if (expandedDraftId === draftId) {
      setExpandedDraftId(null);
      return;
    }
    setExpandedDraftId(draftId);
    if (assetRowsByDraftId[draftId] || !csrfToken) {
      return;
    }

    setLoadingAssetsDraftId(draftId);
    try {
      const rows = await listAdminChallengeDraftPrivateAssets(
        draftId,
        csrfToken,
      );
      setAssetRowsByDraftId((current) => ({ ...current, [draftId]: rows }));
    } catch (e) {
      onError(adminErrorMessage(e, { unknown: t("unknown") }));
    } finally {
      setLoadingAssetsDraftId(null);
    }
  };

  const runDraftAction = async (
    draft: ChallengeDraftListItem,
    action: DraftAction,
  ) => {
    const draftId = draft.id;
    if (!csrfToken) {
      onError(t("signIn"));
      return;
    }
    if (
      (action === "validate" || action === "publish") &&
      !repositoryPath.trim()
    ) {
      onError(t("repositoryRequired"));
      return;
    }
    if (!confirmDraftAction(draftId, action, t)) {
      return;
    }

    setBusyDraftId(draftId);
    try {
      const body: ReviewChallengeDraftRequest | ValidateChallengeDraftRequest =
        action === "validate" || action === "publish"
          ? parseAdminDraftMutationRequest(
              validateChallengeDraftRequestSchema,
              { repository_path: repositoryPath.trim() },
              t("invalidRepository"),
            )
          : parseAdminDraftMutationRequest(
              reviewChallengeDraftRequestSchema,
              {
                message: draftReviewMessage(action, reviewMessage, t),
                expected_validation_bundle_sha256:
                  action === "approve"
                    ? draft.validation_bundle_sha256
                    : undefined,
              },
              t("invalidReview"),
            );
      const response = await adminFetchJson(
        `/admin/challenge-drafts/${encodeURIComponent(draftId)}/${action}`,
        challengeDraftResponseSchema,
        csrfToken,
        {
          method: "POST",
          body: JSON.stringify(body),
        },
      );
      onError(null);
      onMessage(
        t("completed", { id: response.id.slice(0, 8), action: action }),
      );
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e, { unknown: t("unknown") }));
    } finally {
      setBusyDraftId(null);
    }
  };

  const cleanupDrafts = async () => {
    if (!csrfToken) {
      onError(t("cleanupSignIn"));
      return;
    }
    if (!window.confirm(t("cleanupConfirm"))) {
      return;
    }

    setBusyDraftId("cleanup");
    try {
      const response = await adminFetchJson(
        "/admin/challenge-drafts/cleanup",
        challengeDraftCleanupResponseSchema,
        csrfToken,
        { method: "POST" },
      );
      onError(null);
      onMessage(
        t("cleanupResult", {
          drafts: response.abandoned_drafts,
          assets: response.purged_private_assets,
          tempObjects: response.purged_temporary_storage_objects,
        }),
      );
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e, { unknown: t("unknown") }));
    } finally {
      setBusyDraftId(null);
    }
  };

  return {
    repositoryPath,
    setRepositoryPath,
    reviewMessage,
    setReviewMessage,
    busyDraftId,
    expandedDraftId,
    assetRowsByDraftId,
    loadingAssetsDraftId,
    toggleAssetRows,
    runDraftAction,
    cleanupDrafts,
  };
}

function parseAdminDraftMutationRequest<T>(
  schema: ZodType<T>,
  value: unknown,
  fallbackMessage: string,
): T {
  const parsed = schema.safeParse(value);
  if (!parsed.success) {
    throw new AdminApiError(
      400,
      parsed.error.issues[0]?.message ?? fallbackMessage,
    );
  }
  return parsed.data;
}

function draftReviewMessage(
  action: "approve" | "reject" | "abandon",
  input: string,
  t: DraftReviewTranslator,
): string {
  const message = input.trim();
  if (message) {
    return message;
  }

  switch (action) {
    case "approve":
      return t("approved");
    case "reject":
      return t("rejected");
    case "abandon":
      return t("abandoned");
  }
}

function confirmDraftAction(
  draftId: string,
  action: DraftAction,
  t: DraftReviewTranslator,
): boolean {
  const shortId = draftId.slice(0, 8);
  switch (action) {
    case "validate":
      return true;
    case "approve":
      return window.confirm(t("approveConfirm", { id: shortId }));
    case "publish":
      return window.confirm(t("publishConfirm", { id: shortId }));
    case "reject":
      return window.confirm(t("rejectConfirm", { id: shortId }));
    case "abandon":
      return window.confirm(t("abandonConfirm", { id: shortId }));
  }
}
