import {
  Activity,
  ArrowLeft,
  Award,
  Calendar,
  GitCommit,
  ShieldCheck,
  Trophy,
  User,
} from "lucide-react";
import Link from "next/link";
import { getLocale, getTranslations } from "next-intl/server";
import { codeToHtml } from "shiki";
import { CodeBrowser } from "@/components/CodeBrowser";
import { StatusBadge } from "@/components/StatusBadge";
import { fetchJson } from "@/lib/api";
import { artifactIsPublic } from "@/lib/challengeVisibility";
import { formatDate, formatScore } from "@/lib/format";
import {
  displayPrimaryMetric,
  formatDeclaredMetric,
  metricDirectionLabel,
  metricLabel,
  primaryMetricLabel,
} from "@/lib/metrics";
import {
  challengeDetailResponseSchema,
  solutionSubmissionArtifactResponseSchema,
  solutionSubmissionResponseSchema,
} from "@/lib/schemas";
import { selectSubmissionDisplayEvaluation } from "@/lib/submissionEvaluation";

/** Renders the solution submission page component. */
export default async function SolutionSubmissionPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);
  const metricDirectionLabels = {
    maximize: t("challenge.metrics.higherIsBetter"),
    minimize: t("challenge.metrics.lowerIsBetter"),
  };
  const publicCaseStatusLabels = {
    passed: t("common.statuses.passed"),
    failed: t("common.statuses.failed"),
    pending: t("common.statuses.pending"),
  };

  const submission = await fetchJson(
    `/api/public/solution-submissions/${id}`,
    solutionSubmissionResponseSchema,
  );

  const detail = await fetchJson(
    `/api/public/challenges/${submission.challenge_name}`,
    challengeDetailResponseSchema,
  );
  const artifact = artifactIsPublic(detail.spec)
    ? await fetchJson(
        `/api/public/solution-submissions/${id}/artifact`,
        solutionSubmissionArtifactResponseSchema,
      )
    : null;

  // Highlight only the initially selected text file during SSR. The browser can
  // still show unhighlighted text for other files without doing expensive
  // server-side work for every artifact entry.
  const initiallyHighlightedPath = artifact?.files.find(
    (file) => file.is_text && file.content,
  )?.path;
  const filesWithHighlight = artifact
    ? await Promise.all(
        artifact.files.map(async (file) => {
          if (!file.is_text || !file.content) {
            return {
              path: file.path,
              size: file.size,
              is_text: file.is_text,
              content: file.content,
              highlightedHtml: null,
            };
          }

          const html =
            file.path === initiallyHighlightedPath
              ? await codeToHtml(file.content, {
                  lang: file.language || detectLanguage(file.path) || "text",
                  theme: "github-dark",
                })
              : null;

          return {
            path: file.path,
            size: file.size,
            is_text: file.is_text,
            content: file.content,
            highlightedHtml: html,
          };
        }),
      )
    : [];

  const evalDto = selectSubmissionDisplayEvaluation(submission);
  const metricSchema = detail.spec.metric_schema;
  const officialPrimary = submission.official_primary_metric;
  const primary = displayPrimaryMetric(
    metricSchema,
    evalDto?.aggregate_metrics ?? [],
    officialPrimary,
  );

  return (
    <div className="flex flex-col gap-6">
      {/* Hero Card */}
      <div className="card-elevated">
        <Link
          href={`/challenges/${submission.challenge_name}`}
          className="inline-flex items-center gap-1.5 text-[var(--text-body-sm)] text-[var(--text-muted)] hover:text-[var(--accent-primary-text)] transition-colors mb-4"
        >
          <ArrowLeft className="w-3.5 h-3.5" />
          {submission.challenge_title ?? submission.challenge_name}
        </Link>

        <div className="flex flex-col lg:flex-row lg:items-start gap-6">
          <div className="flex-1 min-w-0">
            <h1
              className="text-[var(--text-h1)] font-bold text-[var(--text-primary)] leading-[var(--leading-h1)]"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {t("submissionDetail.title", { id: submission.id.slice(0, 8) })}
            </h1>
            <p className="text-[var(--text-body)] text-[var(--text-secondary)] mt-2 leading-[var(--leading-body)]">
              {submission.explanation}
            </p>
            {submission.note ? (
              <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] mt-2 whitespace-pre-wrap">
                {submission.note}
              </p>
            ) : null}

            <div className="flex flex-wrap gap-2 mt-4">
              {submission.validation_evaluation ? (
                <span className="badge badge-validation inline-flex items-center gap-1.5">
                  <ShieldCheck className="w-3 h-3" />
                  {t("submissionDetail.evaluation.validationFeedback")}
                </span>
              ) : null}
              {submission.official_evaluation ? (
                <span className="badge badge-official inline-flex items-center gap-1.5">
                  <Trophy className="w-3 h-3" />
                  {t("submissionDetail.evaluation.officialResult")}
                </span>
              ) : null}
              <StatusBadge status={submission.status}>
                {t(`submissions.status.${submission.status}`)}
              </StatusBadge>
            </div>
          </div>

          {/* Stats Grid */}
          <div className="grid grid-cols-2 gap-3 lg:w-auto lg:min-w-[280px]">
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Award className="w-4 h-4 text-[var(--accent-primary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {primaryMetricLabel(
                  metricSchema,
                  t("leaderboard.primaryMetric"),
                )}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {formatDeclaredMetric(metricSchema, primary)}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Activity className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("leaderboard.rankScore")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {formatScore(evalDto?.rank_score)}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Trophy className="w-4 h-4 text-[var(--accent-primary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("submissions.officialPrimary")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {formatDeclaredMetric(metricSchema, officialPrimary)}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Calendar className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("submissionDetail.metadata.created")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {formatDate(submission.created_at, locale)}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Two Column Layout */}
      <div className="grid grid-cols-1 lg:grid-cols-[1fr_1.2fr] gap-6">
        {/* Left: Metrics & Metadata */}
        <div className="flex flex-col gap-5">
          {/* Metadata */}
          <div className="card">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4 flex items-center gap-2">
              <User className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              {t("submissionDetail.metadata.title")}
            </h3>
            <div className="grid grid-cols-2 gap-x-4 gap-y-3">
              <div>
                <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                  {t("submissionDetail.metadata.agent")}
                </span>
                <span className="text-[var(--text-body-sm)] font-medium text-[var(--text-primary)]">
                  {submission.agent_display_name ?? submission.agent_id}
                </span>
              </div>
              <div>
                <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                  {t("submissionDetail.metadata.parent")}
                </span>
                <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                  {submission.parent_solution_submission_id ?? t("common.none")}
                </span>
              </div>
              <div>
                <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                  {t("submissionDetail.metadata.created")}
                </span>
                <span className="text-[var(--text-body-sm)] font-mono text-[var(--text-primary)]">
                  {formatDate(submission.created_at, locale)}
                </span>
              </div>
              <div>
                <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                  {t("submissionDetail.metadata.credit")}
                </span>
                <span className="text-[var(--text-body-sm)] text-[var(--text-primary)]">
                  {submission.credit_text || t("common.none")}
                </span>
              </div>
              <div className="col-span-2">
                <span className="block text-[var(--text-caption)] text-[var(--text-muted)] uppercase tracking-wide">
                  {t("submissionDetail.metadata.note")}
                </span>
                <span className="text-[var(--text-body-sm)] text-[var(--text-primary)] whitespace-pre-wrap">
                  {submission.note || t("common.none")}
                </span>
              </div>
            </div>
          </div>

          {/* Aggregate Metrics */}
          <div className="card">
            <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-2 flex items-center gap-2">
              <Activity className="w-4 h-4 text-[var(--accent-primary-text)]" />
              {t("submissionDetail.aggregateMetrics.title")}
            </h3>
            <p className="text-[var(--text-caption)] text-[var(--text-muted)] mb-4">
              {t("submissionDetail.aggregateMetrics.note")}
            </p>
            {evalDto && evalDto.aggregate_metrics.length > 0 ? (
              <div className="grid grid-cols-2 gap-x-4 gap-y-3">
                {evalDto.aggregate_metrics.map((metric) => {
                  const definition = detail.spec.metric_schema.metrics.find(
                    (item) => item.name === metric.metric_name,
                  );
                  return (
                    <div key={metric.metric_name}>
                      <span className="block text-[var(--text-caption)] text-[var(--text-muted)]">
                        {definition?.label ?? metric.metric_name}
                        {definition
                          ? ` · ${metricDirectionLabel(
                              definition.direction,
                              metricDirectionLabels,
                            )}`
                          : ""}
                      </span>
                      <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                        {formatDeclaredMetric(metricSchema, metric)}
                      </span>
                    </div>
                  );
                })}
              </div>
            ) : (
              <p className="text-[var(--text-muted)] text-[var(--text-body-sm)]">
                {t("common.empty")}
              </p>
            )}
          </div>

          {/* Run Metrics */}
          {evalDto && evalDto.run_metrics.length > 0 && (
            <div className="card overflow-x-auto">
              <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4">
                {t("submissionDetail.runMetrics.title")}
              </h3>
              <table className="data-table">
                <thead>
                  <tr>
                    <th>{t("submissionDetail.runMetrics.run")}</th>
                    <th>{t("submissionDetail.runMetrics.metrics")}</th>
                  </tr>
                </thead>
                <tbody>
                  {evalDto.run_metrics.map((run) => (
                    <tr key={run.run_name}>
                      <td className="font-mono text-[var(--text-caption)]">
                        {run.run_name}
                      </td>
                      <td className="text-[var(--text-caption)] text-[var(--text-muted)]">
                        {run.metrics
                          .map(
                            (metric) =>
                              `${metricLabel(metricSchema, metric.metric_name)}: ${formatDeclaredMetric(metricSchema, metric)}`,
                          )
                          .join(" · ")}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {/* Public Cases */}
          {evalDto && evalDto.public_results.length > 0 && (
            <div className="card overflow-x-auto">
              <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] mb-4">
                {t("submissionDetail.publicCases.title")}
              </h3>
              <table className="data-table">
                <thead>
                  <tr>
                    <th>{t("submissionDetail.publicCases.case")}</th>
                    <th>{t("submissionDetail.publicCases.status")}</th>
                    <th>{t("submissionDetail.publicCases.score")}</th>
                    <th>{t("submissionDetail.publicCases.message")}</th>
                  </tr>
                </thead>
                <tbody>
                  {evalDto.public_results.map((c) => (
                    <tr key={c.case_name}>
                      <td className="font-mono text-[var(--text-caption)]">
                        {c.case_name}
                      </td>
                      <td>
                        <span
                          className={`badge ${
                            c.status === "passed"
                              ? "badge-success"
                              : c.status === "failed"
                                ? "badge-error"
                                : "badge-warning"
                          }`}
                        >
                          {publicCaseStatusLabels[
                            c.status as keyof typeof publicCaseStatusLabels
                          ] ?? c.status}
                        </span>
                      </td>
                      <td className="font-mono">{formatScore(c.score)}</td>
                      <td className="text-[var(--text-muted)] text-[var(--text-caption)]">
                        {c.message ?? t("common.none")}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        {/* Right: Code Browser */}
        {artifact ? (
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)] flex items-center gap-2">
                <GitCommit className="w-4 h-4 text-[var(--accent-secondary-text)]" />
                {t("submissionDetail.codeBrowser.title")}
              </h3>
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {artifact.archive_name} · {artifact.file_count}{" "}
                {t("submissionDetail.codeBrowser.files")} ·{" "}
                {artifact.total_uncompressed_size.toLocaleString()}{" "}
                {t("submissionDetail.codeBrowser.bytes")}
              </span>
            </div>
            <CodeBrowser
              files={filesWithHighlight.map((f) => ({
                path: f.path,
                size: f.size,
                is_text: f.is_text,
                content: f.content,
                highlightedHtml: f.highlightedHtml,
              }))}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** Detects language for presentation. */
function detectLanguage(path: string): string {
  if (path.endsWith(".py")) return "python";
  if (path.endsWith(".json")) return "json";
  if (path.endsWith(".md")) return "markdown";
  if (path.endsWith(".sh")) return "bash";
  if (path.endsWith(".js") || path.endsWith(".ts")) return "typescript";
  if (path.endsWith(".yaml") || path.endsWith(".yml")) return "yaml";
  if (path.endsWith(".toml")) return "toml";
  return "text";
}
