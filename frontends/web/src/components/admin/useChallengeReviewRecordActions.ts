"use client";

import { useTranslations } from "next-intl";
import { useState } from "react";
import type { ZodType } from "zod";
import { adminErrorMessage } from "@/components/admin/errors";
import {
  AdminApiError,
  adminFetchJson,
  listAdminChallengeReviewRecordPrivateAssets,
} from "@/lib/adminApi";
import {
  type AdminChallengePrivateAssetListResponse,
  type ChallengeReviewDecisionRequest,
  type ChallengeReviewRecordListItem,
  challengeReviewDecisionRequestSchema,
  challengeReviewRecordCleanupResponseSchema,
  challengeReviewRecordResponseSchema,
  type ValidateChallengeReviewRecordRequest,
  validateChallengeReviewRecordRequestSchema,
} from "@/lib/schemas";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;
type ReviewRecordAction =
  | "validate"
  | "approve"
  | "publish"
  | "reject"
  | "abandon";
type ReviewRecordTranslator = ReturnType<typeof useTranslations>;

interface UseChallengeReviewRecordActionsParams {
  csrfToken: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

/** Owns admin review record mutation state and refresh behavior. */
export function useChallengeReviewRecordActions({
  csrfToken,
  onRefresh,
  onError,
  onMessage,
}: UseChallengeReviewRecordActionsParams) {
  const t = useTranslations("admin.reviewRecords");
  const [repositoryPath, setRepositoryPath] = useState(
    "challenge-repos/agentics-challenges",
  );
  const [reviewMessage, setReviewMessage] = useState("");
  const [busyReviewRecordId, setBusyReviewRecordId] = useState<string | null>(
    null,
  );
  const [expandedReviewRecordId, setExpandedReviewRecordId] = useState<
    string | null
  >(null);
  const [assetRowsByReviewRecordId, setAssetRowsByReviewRecordId] = useState<
    Record<string, AdminChallengePrivateAssetListResponse>
  >({});
  const [loadingAssetsReviewRecordId, setLoadingAssetsReviewRecordId] =
    useState<string | null>(null);

  const toggleAssetRows = async (reviewRecordId: string) => {
    if (expandedReviewRecordId === reviewRecordId) {
      setExpandedReviewRecordId(null);
      return;
    }
    setExpandedReviewRecordId(reviewRecordId);
    if (assetRowsByReviewRecordId[reviewRecordId] || !csrfToken) {
      return;
    }

    setLoadingAssetsReviewRecordId(reviewRecordId);
    try {
      const rows = await listAdminChallengeReviewRecordPrivateAssets(
        reviewRecordId,
        csrfToken,
      );
      setAssetRowsByReviewRecordId((current) => ({
        ...current,
        [reviewRecordId]: rows,
      }));
    } catch (e) {
      onError(adminErrorMessage(e, { unknown: t("unknown") }));
    } finally {
      setLoadingAssetsReviewRecordId(null);
    }
  };

  const runReviewRecordAction = async (
    reviewRecord: ChallengeReviewRecordListItem,
    action: ReviewRecordAction,
  ) => {
    const reviewRecordId = reviewRecord.id;
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
    if (!confirmReviewRecordAction(reviewRecordId, action, t)) {
      return;
    }

    setBusyReviewRecordId(reviewRecordId);
    try {
      const body:
        | ChallengeReviewDecisionRequest
        | ValidateChallengeReviewRecordRequest =
        action === "validate" || action === "publish"
          ? parseAdminReviewRecordMutationRequest(
              validateChallengeReviewRecordRequestSchema,
              { repository_path: repositoryPath.trim() },
              t("invalidRepository"),
            )
          : parseAdminReviewRecordMutationRequest(
              challengeReviewDecisionRequestSchema,
              {
                message: reviewRecordDecisionMessage(action, reviewMessage, t),
                expected_validation_bundle_sha256:
                  action === "approve"
                    ? reviewRecord.validation_bundle_sha256
                    : undefined,
              },
              t("invalidReview"),
            );
      const response = await adminFetchJson(
        `/admin/challenge-review-records/${encodeURIComponent(reviewRecordId)}/${action}`,
        challengeReviewRecordResponseSchema,
        csrfToken,
        {
          method: "POST",
          body: JSON.stringify(body),
        },
      );
      onError(null);
      onMessage(
        t("completed", {
          id: response.id.slice(0, 8),
          action: reviewRecordActionLabel(action, t),
        }),
      );
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e, { unknown: t("unknown") }));
    } finally {
      setBusyReviewRecordId(null);
    }
  };

  const cleanupReviewRecords = async () => {
    if (!csrfToken) {
      onError(t("cleanupSignIn"));
      return;
    }
    if (!window.confirm(t("cleanupConfirm"))) {
      return;
    }

    setBusyReviewRecordId("cleanup");
    try {
      const response = await adminFetchJson(
        "/admin/challenge-review-records/cleanup",
        challengeReviewRecordCleanupResponseSchema,
        csrfToken,
        { method: "POST" },
      );
      onError(null);
      onMessage(
        t("cleanupResult", {
          reviewRecords: response.abandoned_review_records,
          assets: response.purged_private_assets,
          tempObjects: response.purged_temporary_storage_objects,
        }),
      );
      await onRefresh({ quiet: true });
    } catch (e) {
      onError(adminErrorMessage(e, { unknown: t("unknown") }));
    } finally {
      setBusyReviewRecordId(null);
    }
  };

  return {
    repositoryPath,
    setRepositoryPath,
    reviewMessage,
    setReviewMessage,
    busyReviewRecordId,
    expandedReviewRecordId,
    assetRowsByReviewRecordId,
    loadingAssetsReviewRecordId,
    toggleAssetRows,
    runReviewRecordAction,
    cleanupReviewRecords,
  };
}

function parseAdminReviewRecordMutationRequest<T>(
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

function reviewRecordActionLabel(
  action: ReviewRecordAction,
  t: ReviewRecordTranslator,
) {
  switch (action) {
    case "validate":
      return t("validated");
    case "approve":
      return t("approved");
    case "publish":
      return t("published");
    case "reject":
      return t("rejected");
    case "abandon":
      return t("abandoned");
  }
}

function reviewRecordDecisionMessage(
  action: "approve" | "reject" | "abandon",
  input: string,
  t: ReviewRecordTranslator,
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

function confirmReviewRecordAction(
  reviewRecordId: string,
  action: ReviewRecordAction,
  t: ReviewRecordTranslator,
): boolean {
  const shortId = reviewRecordId.slice(0, 8);
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
