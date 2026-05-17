import { Clock, Code2, MemoryStick, Package } from "lucide-react";
import { getTranslations } from "next-intl/server";
import { ChallengeNav } from "@/components/ChallengeNav";
import { EvaluationModeBadges } from "@/components/EvaluationModeBadges";
import { fetchJson } from "@/lib/api";
import { challengeDetailResponseSchema } from "@/lib/schemas";

/** Renders the challenge layout component. */
export default async function ChallengeLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ name: string }>;
}) {
  const { name } = await params;
  const t = await getTranslations();
  let challenge: import("@/lib/schemas").ChallengeDetailResponse;
  let error: string | null = null;

  try {
    challenge = await fetchJson(
      `/api/public/challenges/${name}`,
      challengeDetailResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : t("common.error");
    return (
      <div className="card text-center py-12 text-[var(--status-error)]">
        {t("common.error")}: {error}
      </div>
    );
  }

  if (challenge.spec.targets.length === 0) {
    return (
      <div className="card text-center py-12 text-[var(--status-error)]">
        {t("common.error")}: challenge has no configured targets.
      </div>
    );
  }
  const primaryTarget = challenge.spec.targets[0];
  const validationEnabled = challenge.spec.targets.some(
    (target) => target.validation_enabled,
  );

  return (
    <div className="flex flex-col gap-6">
      {/* Hero Banner */}
      <div className="card-elevated">
        <div className="flex flex-col lg:flex-row lg:items-start gap-6">
          <div className="flex-1 min-w-0">
            <span className="text-[var(--text-caption)] text-[var(--text-muted)] font-mono tracking-wide uppercase">
              {challenge.name}
            </span>
            <h1
              className="text-[var(--text-h1)] font-bold text-[var(--text-primary)] mt-1 leading-[var(--leading-h1)]"
              style={{ fontFamily: "var(--font-serif)" }}
            >
              {challenge.title}
            </h1>
            <p className="text-[var(--text-body)] text-[var(--text-secondary)] mt-3 leading-[var(--leading-body)] max-w-2xl">
              {challenge.summary}
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
          </div>

          {/* Resource Chips */}
          <div className="grid grid-cols-2 gap-3 lg:w-auto lg:min-w-[240px]">
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Clock className="w-4 h-4 text-[var(--accent-primary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("challenge.limits.timeLimit")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {primaryTarget.resource_profile.timeout_sec}
                {t("challenge.limits.seconds")}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <MemoryStick className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("challenge.limits.memoryLimit")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {primaryTarget.resource_profile.memory_limit_mb}{" "}
                {t("challenge.limits.mb")}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Code2 className="w-4 h-4 text-[var(--accent-secondary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("challenge.config.protocol")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {challenge.spec.solution.protocol}
              </span>
            </div>
            <div className="card flex flex-col gap-1 py-3 px-4">
              <Package className="w-4 h-4 text-[var(--accent-primary-text)]" />
              <span className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("challenge.config.resourceProfile")}
              </span>
              <span className="text-[var(--text-body-sm)] font-mono font-medium text-[var(--text-primary)]">
                {primaryTarget.name}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Tabs */}
      <ChallengeNav challengeName={name} defaultTarget={primaryTarget.name} />

      {/* Page Content */}
      {children}
    </div>
  );
}
