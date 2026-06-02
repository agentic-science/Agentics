"use client";

import { GitPullRequest, Trash2 } from "lucide-react";
import { useTranslations } from "next-intl";
import {
  ConsoleSectionTitle as SectionTitle,
  ConsoleTextInput as TextInput,
} from "@/components/ConsolePrimitives";
import type { ChallengeReviewRecordListItem } from "@/lib/schemas";
import { ChallengeReviewRecordTable } from "./ChallengeReviewRecordTable";
import { useChallengeReviewRecordActions } from "./useChallengeReviewRecordActions";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

interface ChallengeReviewRecordPanelProps {
  csrfToken: string;
  reviewRecords: ChallengeReviewRecordListItem[];
  locale: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

/** Renders the admin challenge-review-record review panel shell. */
export function ChallengeReviewRecordPanel({
  csrfToken,
  reviewRecords,
  locale,
  onRefresh,
  onError,
  onMessage,
}: ChallengeReviewRecordPanelProps) {
  const t = useTranslations("admin.reviewRecords");
  const common = useTranslations("common");
  const reviewActions = useChallengeReviewRecordActions({
    csrfToken,
    onRefresh,
    onError,
    onMessage,
  });

  return (
    <section className="grid grid-cols-1 gap-6">
      <div className="card">
        <div className="flex flex-col lg:flex-row lg:items-end justify-between gap-5">
          <div>
            <SectionTitle
              icon={<GitPullRequest className="w-4 h-4" />}
              title={t("title")}
            />
            <p className="mt-2 text-body-sm text-fg-secondary">
              {t("description")}
            </p>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-[minmax(260px,1fr)_minmax(200px,280px)_auto] gap-3 w-full lg:w-auto">
            <TextInput
              label={t("repositoryPath")}
              value={reviewActions.repositoryPath}
              onChange={reviewActions.setRepositoryPath}
            />
            <TextInput
              label={t("reviewMessage")}
              value={reviewActions.reviewMessage}
              onChange={reviewActions.setReviewMessage}
            />
            <button
              type="button"
              className="btn btn-secondary self-end"
              onClick={() => void reviewActions.cleanupReviewRecords()}
              disabled={
                !csrfToken || reviewActions.busyReviewRecordId === "cleanup"
              }
            >
              <Trash2 className="w-4 h-4" />
              {t("cleanupStale")}
            </button>
          </div>
        </div>
      </div>

      <div className="card overflow-x-auto">
        <div className="flex items-center justify-between gap-4 mb-4">
          <span className="badge badge-default">
            {common("rows", { count: reviewRecords.length })}
          </span>
        </div>
        <ChallengeReviewRecordTable
          reviewRecords={reviewRecords}
          locale={locale}
          csrfToken={csrfToken}
          busyReviewRecordId={reviewActions.busyReviewRecordId}
          expandedReviewRecordId={reviewActions.expandedReviewRecordId}
          assetRowsByReviewRecordId={reviewActions.assetRowsByReviewRecordId}
          loadingAssetsReviewRecordId={
            reviewActions.loadingAssetsReviewRecordId
          }
          onToggleAssetRows={reviewActions.toggleAssetRows}
          onRunReviewRecordAction={reviewActions.runReviewRecordAction}
        />
      </div>
    </section>
  );
}
