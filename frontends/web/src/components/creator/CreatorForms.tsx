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
import type { Dispatch, FormEvent, SetStateAction } from "react";
import {
  ConsoleSectionTitle as SectionTitle,
  ConsoleTextInput as TextInput,
} from "@/components/ConsolePrimitives";
import type { ChallengePrivateAssetKind } from "@/lib/creatorApi";

export type CreatorReviewRecordFormState = {
  repoUrl: string;
  prNumber: string;
  prUrl: string;
  commitSha: string;
  challengePath: string;
  manifestText: string;
};

export type CreatorPrivateAssetFormState = {
  reviewRecordId: string;
  assetName: string;
  kind: ChallengePrivateAssetKind;
  required: boolean;
  file: File | null;
};

export type CreatorOwnerFormState = {
  challengeName: string;
  target: string;
  shortlistText: string;
};

export const defaultCreatorManifest = JSON.stringify(
  {
    schema_version: 1,
    request: "new_challenge",
    challenge_name: "frontier-cs-example-challenge",
    title: "Frontier-CS Example Challenge",
    summary: {
      en: "Benchmark a small Frontier-CS style task.",
      zh: "评测一个小型 Frontier-CS 风格任务。",
    },
    keywords: ["frontier-cs", "benchmark", "migration"],
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

type ReviewRecordCreateFormProps = {
  reviewRecordForm: CreatorReviewRecordFormState;
  setReviewRecordForm: Dispatch<SetStateAction<CreatorReviewRecordFormState>>;
  loading: boolean;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

export function ReviewRecordCreateForm({
  reviewRecordForm,
  setReviewRecordForm,
  loading,
  onSubmit,
}: ReviewRecordCreateFormProps) {
  const t = useTranslations("creator");
  return (
    <form className="card flex flex-col gap-4" onSubmit={onSubmit}>
      <SectionTitle
        icon={<GitPullRequest className="w-4 h-4" />}
        title={t("reviewRecord.create")}
      />
      <TextInput
        label={t("reviewRecord.repositoryUrl")}
        value={reviewRecordForm.repoUrl}
        onChange={(repoUrl) =>
          setReviewRecordForm({ ...reviewRecordForm, repoUrl })
        }
        required
      />
      <TextInput
        label={t("reviewRecord.prNumber")}
        value={reviewRecordForm.prNumber}
        onChange={(prNumber) =>
          setReviewRecordForm({ ...reviewRecordForm, prNumber })
        }
        required
      />
      <TextInput
        label={t("reviewRecord.prUrl")}
        value={reviewRecordForm.prUrl}
        onChange={(prUrl) =>
          setReviewRecordForm({ ...reviewRecordForm, prUrl })
        }
        required
      />
      <TextInput
        label={t("reviewRecord.commitSha")}
        value={reviewRecordForm.commitSha}
        onChange={(commitSha) =>
          setReviewRecordForm({ ...reviewRecordForm, commitSha })
        }
        required
      />
      <TextInput
        label={t("reviewRecord.challengePath")}
        value={reviewRecordForm.challengePath}
        onChange={(challengePath) =>
          setReviewRecordForm({ ...reviewRecordForm, challengePath })
        }
        required
      />
      <label className="flex flex-col gap-1">
        <span className="text-caption uppercase tracking-wide text-fg-muted">
          {t("reviewRecord.manifestJson")}
        </span>
        <textarea
          className="min-h-80 rounded-control border border-line bg-surface-2 px-3 py-2 font-mono text-caption leading-relaxed outline-none focus:border-action"
          value={reviewRecordForm.manifestText}
          onChange={(event) =>
            setReviewRecordForm({
              ...reviewRecordForm,
              manifestText: event.target.value,
            })
          }
          required
        />
      </label>
      <button type="submit" className="btn btn-primary" disabled={loading}>
        <GitPullRequest className="w-4 h-4" />
        {t("reviewRecord.create")}
      </button>
    </form>
  );
}

type ReviewRecordInspectFormProps = {
  reviewRecordLookupId: string;
  setReviewRecordLookupId: (reviewRecordId: string) => void;
  loading: boolean;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

export function ReviewRecordInspectForm({
  reviewRecordLookupId,
  setReviewRecordLookupId,
  loading,
  onSubmit,
}: ReviewRecordInspectFormProps) {
  const t = useTranslations("creator");
  return (
    <form className="card flex flex-col gap-4" onSubmit={onSubmit}>
      <SectionTitle
        icon={<RefreshCw className="w-4 h-4" />}
        title={t("reviewRecord.inspect")}
      />
      <TextInput
        label={t("reviewRecord.reviewRecordId")}
        value={reviewRecordLookupId}
        onChange={setReviewRecordLookupId}
        required
      />
      <button type="submit" className="btn btn-secondary" disabled={loading}>
        {t("reviewRecord.load")}
      </button>
    </form>
  );
}

type PrivateAssetUploadFormProps = {
  assetForm: CreatorPrivateAssetFormState;
  setAssetForm: Dispatch<SetStateAction<CreatorPrivateAssetFormState>>;
  loading: boolean;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

export function PrivateAssetUploadForm({
  assetForm,
  setAssetForm,
  loading,
  onSubmit,
}: PrivateAssetUploadFormProps) {
  const t = useTranslations("creator");
  return (
    <form className="card flex flex-col gap-4" onSubmit={onSubmit}>
      <SectionTitle
        icon={<UploadCloud className="w-4 h-4" />}
        title={t("reviewRecord.uploadPrivateAsset")}
      />
      <TextInput
        label={t("reviewRecord.reviewRecordId")}
        value={assetForm.reviewRecordId}
        onChange={(reviewRecordId) =>
          setAssetForm({ ...assetForm, reviewRecordId })
        }
        required
      />
      <TextInput
        label={t("reviewRecord.assetName")}
        value={assetForm.assetName}
        onChange={(assetName) => setAssetForm({ ...assetForm, assetName })}
        required
      />
      <label className="flex flex-col gap-1">
        <span className="text-caption uppercase tracking-wide text-fg-muted">
          {t("reviewRecord.assetKind")}
        </span>
        <select
          className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm outline-none focus:border-action"
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
      <label className="flex items-center gap-2 text-body-sm text-fg-secondary">
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
        {t("reviewRecord.requiredForPublish")}
      </label>
      <input
        type="file"
        className="rounded-control border border-line bg-surface-2 px-3 py-2 text-body-sm"
        onChange={(event) =>
          setAssetForm({
            ...assetForm,
            file: event.target.files?.[0] ?? null,
          })
        }
        required
      />
      <button type="submit" className="btn btn-primary" disabled={loading}>
        <FileArchive className="w-4 h-4" />
        {t("reviewRecord.uploadAsset")}
      </button>
    </form>
  );
}

type OwnerStatsFormProps = {
  ownerForm: CreatorOwnerFormState;
  setOwnerForm: Dispatch<SetStateAction<CreatorOwnerFormState>>;
  loading: boolean;
  onLoad: () => void;
};

export function OwnerStatsForm({
  ownerForm,
  setOwnerForm,
  loading,
  onLoad,
}: OwnerStatsFormProps) {
  const t = useTranslations("creator");
  return (
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
        onClick={() => void onLoad()}
      >
        <Users className="w-4 h-4" />
        {t("owner.load")}
      </button>
    </div>
  );
}

type ShortlistUploadFormProps = {
  ownerForm: CreatorOwnerFormState;
  setOwnerForm: Dispatch<SetStateAction<CreatorOwnerFormState>>;
  loading: boolean;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

export function ShortlistUploadForm({
  ownerForm,
  setOwnerForm,
  loading,
  onSubmit,
}: ShortlistUploadFormProps) {
  const t = useTranslations("creator");
  return (
    <form className="card flex flex-col gap-4" onSubmit={onSubmit}>
      <SectionTitle
        icon={<ListPlus className="w-4 h-4" />}
        title={t("owner.uploadShortlist")}
      />
      <label className="flex flex-col gap-1">
        <span className="text-caption uppercase tracking-wide text-fg-muted">
          {t("owner.deltaJson")}
        </span>
        <textarea
          className="min-h-40 rounded-control border border-line bg-surface-2 px-3 py-2 font-mono text-caption leading-relaxed outline-none focus:border-action"
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
      <button type="submit" className="btn btn-primary" disabled={loading}>
        <ListPlus className="w-4 h-4" />
        {t("owner.uploadDelta")}
      </button>
    </form>
  );
}
