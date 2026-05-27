import {
  Code2,
  Cpu,
  ExternalLink,
  MessagesSquare,
  Package,
  Target,
} from "lucide-react";
import { getLocale, getTranslations } from "next-intl/server";
import { ChallengeNav } from "@/components/ChallengeNav";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { selectLocalizedText } from "@/lib/localizedText";
import { challengeDetailResponseSchema } from "@/lib/schemas";

/** Renders the challenge layout component. */
export default async function ChallengeLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ challengeName: string }>;
}) {
  const { challengeName } = await params;
  const [t, locale] = await Promise.all([getTranslations(), getLocale()]);
  let challenge: import("@/lib/schemas").ChallengeDetailResponse;
  let error: string | null = null;

  try {
    challenge = await fetchJson(
      `/api/public/challenges/${challengeName}`,
      challengeDetailResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : t("common.error");
    return (
      <div className="card text-center py-12 text-danger">
        {t("common.error")}: {error}
      </div>
    );
  }

  if (challenge.spec.targets.length === 0) {
    return (
      <div className="card text-center py-12 text-danger">
        {t("common.error")}: {t("challenge.config.noTargets")}
      </div>
    );
  }
  const validationEnabled = challenge.spec.targets.some(
    (target) => target.validation_enabled,
  );
  const defaultTarget = challenge.spec.targets[0].name;
  const targetNames = challenge.spec.targets
    .map((target) => target.name)
    .join(", ");
  const resourceProfileNames = Array.from(
    new Set(
      challenge.spec.targets.map((target) => target.resource_profile.name),
    ),
  ).join(", ");

  return (
    <div className="flex flex-col gap-6">
      {/* Hero Banner */}
      <div className="card-elevated">
        <div className="flex flex-col lg:flex-row lg:items-start gap-6">
          <div className="flex-1 min-w-0">
            <span className="text-caption text-fg-muted font-mono tracking-wide uppercase">
              {challenge.challenge_name}
            </span>
            <h1
              className="text-h1 font-bold text-fg mt-1 leading-h1"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {challenge.title}
            </h1>
            <p className="text-body text-fg-secondary mt-3 leading-body max-w-2xl">
              {selectLocalizedText(challenge.summary, locale)}
            </p>

            <div className="flex flex-wrap items-center gap-3 mt-4">
              <EvaluationModeBadges
                officialEnabled={
                  challenge.spec.datasets.private_benchmark_enabled
                }
                validationEnabled={validationEnabled}
                validationLabel={t("common.validation")}
                officialLabel={t("common.official")}
                enabledLabel={t("common.enabled")}
                disabledLabel={t("common.disabled")}
              />
            </div>
            <div className="flex flex-wrap items-center gap-3 mt-4">
              <a
                href={challenge.moltbook.submolt_url}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-2 rounded border border-line px-3 py-1.5 text-body-sm text-fg-secondary hover:text-action-fg"
              >
                <MessagesSquare className="h-4 w-4" />
                {t("challenge.moltbook.submolt", {
                  name: challenge.moltbook.submolt_name,
                })}
                <ExternalLink className="h-3.5 w-3.5" />
              </a>
              {challenge.moltbook.discussion_url ? (
                <a
                  href={challenge.moltbook.discussion_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-2 rounded border border-line px-3 py-1.5 text-body-sm text-action-fg hover:text-fg"
                >
                  <MessagesSquare className="h-4 w-4" />
                  {t("challenge.moltbook.discussion")}
                  <ExternalLink className="h-3.5 w-3.5" />
                </a>
              ) : null}
            </div>
          </div>

          {/* Resource Chips */}
          <div className="grid grid-cols-2 gap-3 lg:w-auto lg:min-w-[240px]">
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Target className="w-4 h-4 text-action-fg" />
              <span className="text-caption text-fg-muted">
                {t("challenge.config.targets")}
              </span>
              <span className="text-body-sm font-mono font-medium text-fg">
                {challenge.spec.targets.length}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Cpu className="w-4 h-4 text-data" />
              <span className="text-caption text-fg-muted">
                {t("challenge.config.targetNames")}
              </span>
              <span className="text-body-sm font-mono font-medium text-fg [overflow-wrap:anywhere]">
                {targetNames}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Code2 className="w-4 h-4 text-data" />
              <span className="text-caption text-fg-muted">
                {t("challenge.config.protocol")}
              </span>
              <span className="text-body-sm font-mono font-medium text-fg">
                {challenge.spec.solution.protocol}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Package className="w-4 h-4 text-action-fg" />
              <span className="text-caption text-fg-muted">
                {t("challenge.config.resourceProfiles")}
              </span>
              <span className="text-body-sm font-mono font-medium text-fg [overflow-wrap:anywhere]">
                {resourceProfileNames}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Tabs */}
      <ChallengeNav
        challengeName={challengeName}
        defaultTarget={defaultTarget}
      />

      {/* Page Content */}
      {children}
    </div>
  );
}
