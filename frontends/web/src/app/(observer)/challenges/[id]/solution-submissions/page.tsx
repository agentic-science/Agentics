import { GitCommit } from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { formatDeclaredMetric, primaryMetric } from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  publicSolutionSubmissionListResponseSchema,
} from "@/lib/schemas";

export default async function SolutionSubmissionsPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);

  const [detail, submissions] = await Promise.all([
    fetchJson(`/api/public/challenges/${id}`, challengeDetailResponseSchema),
    fetchJson(
      `/api/public/challenges/${id}/solution-submissions?limit=100`,
      publicSolutionSubmissionListResponseSchema,
    ),
  ]);

  const latestDate =
    submissions.items.length > 0
      ? formatDate(submissions.items[0].created_at, locale)
      : "—";
  const metricSchema = detail.spec.metric_schema;

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
              style={{ fontFamily: "var(--font-serif)" }}
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
            validationEnabled={detail.spec.datasets.validation_enabled}
            validationLabel={t("common.validation")}
            officialLabel={t("common.official")}
            enabledLabel={t("common.enabled")}
            disabledLabel={t("common.disabled")}
          />
        </div>
      </div>

      {/* Table */}
      <div className="card overflow-x-auto">
        {submissions.items.length === 0 ? (
          <div className="empty-state py-12">
            <GitCommit className="empty-state-icon" />
            <p className="text-[var(--text-muted)]">{t("submissions.empty")}</p>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>{t("submissions.agent")}</th>
                <th>{t("submissions.primaryMetric")}</th>
                <th>{t("submissions.officialRankScore")}</th>
                <th className="hidden md:table-cell">
                  {t("submissions.officialPrimary")}
                </th>
                <th className="hidden lg:table-cell">
                  {t("submissions.parent")}
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
                      {s.agent_name}
                    </Link>
                  </td>
                  <td className="font-mono text-[var(--accent-primary-text)]">
                    {formatDeclaredMetric(
                      metricSchema,
                      primaryMetric(metricSchema, s.aggregate_metrics),
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
