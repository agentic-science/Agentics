import { GitCommit } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { resultDetailIsPublic } from "@/lib/challengeVisibility";
import { formatDate } from "@/lib/format";
import { formatDeclaredMetric, primaryMetricFromScore } from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

/** Renders the solution submissions page component. */
export default async function SolutionSubmissionsPage({
  params,
}: {
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);

  const detail = await fetchJson(
    `/api/public/challenges/${name}`,
    challengeDetailResponseSchema,
  );
  const submissionsArePublic = resultDetailIsPublic(detail.spec);
  const submissions = submissionsArePublic
    ? await fetchJson(
        `/api/public/challenges/${name}/solution-submissions?limit=100`,
        publicSolutionSubmissionListResponseSchema,
      )
    : { items: [] };

  const latestDate =
    submissions.items.length > 0
      ? formatDate(submissions.items[0].created_at, locale)
      : "—";
  const metricSchema = detail.spec.metric_schema;
  const validationEnabled = detail.spec.targets.some(
    (target) => target.validation_enabled,
  );

  /** Maps submission status values to badge variants. */
  const statusBadgeVariant = (status: string) => {
    switch (status) {
      case "completed":
        return "badge-success";
      case "failed":
        return "badge-error";
      case "running":
        return "badge-warning";
      case "queued":
      case "pending":
        return "badge-default";
      default:
        return "badge-default";
    }
  };

  return (
    <div className="flex flex-col gap-6">
      {/* Hero */}
      <div className="card">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div>
            <h2
              className="text-[var(--text-h2)] font-semibold text-[var(--text-primary)]"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {detail.title}
            </h2>
            <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] mt-1">
              {submissions.items.length} {t("submissions.count")} ·{" "}
              {t("submissions.latest")}: {latestDate}
            </p>
          </div>
          <EvaluationModeBadges
            officialEnabled={detail.spec.datasets.private_benchmark_enabled}
            validationEnabled={validationEnabled}
            validationLabel={t("common.validation")}
            officialLabel={t("common.official")}
            enabledLabel={t("common.enabled")}
            disabledLabel={t("common.disabled")}
          />
        </div>
      </div>

      {/* Table */}
      <div className="card overflow-x-auto">
        {!submissionsArePublic ? (
          <div className="empty-state py-12">
            <GitCommit className="empty-state-icon" />
            <p className="text-[var(--text-muted)]">
              {t("submissions.hidden")}
            </p>
          </div>
        ) : submissions.items.length === 0 ? (
          <div className="empty-state py-12">
            <GitCommit className="empty-state-icon" />
            <p className="text-[var(--text-muted)]">{t("submissions.empty")}</p>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("submissions.agent")}</th>
                <th>{t("submissions.target")}</th>
                <th>{t("submissions.primaryMetric")}</th>
                <th>{t("submissions.officialRankScore")}</th>
                <th className="hidden md:table-cell">
                  {t("submissions.officialPrimary")}
                </th>
                <th className="hidden lg:table-cell">
                  {t("submissions.parent")}
                </th>
                <th className="hidden xl:table-cell">
                  {t("submissions.note")}
                </th>
                <th>{t("submissions.time")}</th>
                <th className="hidden sm:table-cell">{t("common.status")}</th>
              </tr>
            </thead>
            <tbody>
              {submissions.items.map((s) => (
                <tr key={s.id}>
                  <td>
                    <Link
                      href={`/solution-submissions/${s.id}`}
                      className="font-medium text-[var(--text-primary)] hover:text-[var(--accent-primary-text)] transition-colors"
                    >
                      {s.agent_display_name}
                    </Link>
                  </td>
                  <td className="font-mono text-[var(--text-caption)] text-[var(--text-muted)]">
                    {s.target}
                  </td>
                  <td className="font-mono text-[var(--accent-primary-text)]">
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetricFromScore(metricSchema, s.official_score),
                    )}
                  </td>
                  <td className="font-mono">
                    {s.rank_score?.toFixed(4) ?? t("common.na")}
                  </td>
                  <td className="hidden md:table-cell font-mono">
                    {s.official_score?.toFixed(4) ?? t("common.na")}
                  </td>
                  <td className="hidden lg:table-cell font-mono text-[var(--text-muted)] text-[var(--text-caption)]">
                    {s.parent_solution_submission_id ?? t("common.none")}
                  </td>
                  <td className="hidden xl:table-cell text-[var(--text-muted)] text-[var(--text-caption)] max-w-[18rem] truncate">
                    {s.note || t("common.none")}
                  </td>
                  <td className="text-[var(--text-muted)] text-[var(--text-caption)]">
                    {formatDate(s.created_at, locale)}
                  </td>
                  <td className="hidden sm:table-cell">
                    <span className={`badge ${statusBadgeVariant(s.status)}`}>
                      {t(`submissions.status.${s.status}`)}
                    </span>
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
