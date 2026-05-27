"use client";

import { GitPullRequest, Trash2 } from "lucide-react";
import { useTranslations } from "next-intl";
import {
  ConsoleSectionTitle as SectionTitle,
  ConsoleTextInput as TextInput,
} from "@/components/ConsolePrimitives";
import type { ChallengeDraftListItem } from "@/lib/schemas";
import { ChallengeDraftReviewTable } from "./ChallengeDraftReviewTable";
import { useChallengeDraftReviewActions } from "./useChallengeDraftReviewActions";

type RefreshOptions = { quiet?: boolean };
type AdminRefresh = (options?: RefreshOptions) => Promise<void>;

interface ChallengeDraftReviewPanelProps {
  csrfToken: string;
  drafts: ChallengeDraftListItem[];
  locale: string;
  onRefresh: AdminRefresh;
  onError: (message: string | null) => void;
  onMessage: (message: string | null) => void;
}

/** Renders the admin challenge-draft review panel shell. */
export function ChallengeDraftReviewPanel({
  csrfToken,
  drafts,
  locale,
  onRefresh,
  onError,
  onMessage,
}: ChallengeDraftReviewPanelProps) {
  const t = useTranslations("admin.draftReview");
  const common = useTranslations("common");
  const reviewActions = useChallengeDraftReviewActions({
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
              onClick={() => void reviewActions.cleanupDrafts()}
              disabled={!csrfToken || reviewActions.busyDraftId === "cleanup"}
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
            {common("rows", { count: drafts.length })}
          </span>
        </div>
        <ChallengeDraftReviewTable
          drafts={drafts}
          locale={locale}
          csrfToken={csrfToken}
          busyDraftId={reviewActions.busyDraftId}
          expandedDraftId={reviewActions.expandedDraftId}
          assetRowsByDraftId={reviewActions.assetRowsByDraftId}
          loadingAssetsDraftId={reviewActions.loadingAssetsDraftId}
          onToggleAssetRows={reviewActions.toggleAssetRows}
          onRunDraftAction={reviewActions.runDraftAction}
        />
      </div>
    </section>
  );
}
