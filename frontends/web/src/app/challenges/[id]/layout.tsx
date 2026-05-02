import { ChallengeTabs } from "@/components/ChallengeTabs";
import { fetchJson } from "@/lib/api";
import { challengeDetailResponseSchema } from "@/lib/schemas";

/** Shared challenge detail shell with header metadata and subpage tabs. */
export default async function ChallengeLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  let challenge: import("@/lib/schemas").ChallengeDetailResponse;
  let error: string | null = null;

  try {
    challenge = await fetchJson(
      `/api/public/challenges/${id}`,
      challengeDetailResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : "加载失败";
    return (
      <div className="workspace-panel">
        <div className="empty-block">加载失败：{error}</div>
      </div>
    );
  }

  return (
    <div className="page-stack">
      <div className="hero-panel workspace-panel">
        <div className="hero-copy-block">
          <span className="section-kicker">{challenge.slug}</span>
          <h1 className="page-title">{challenge.title}</h1>
          <p className="page-summary">{challenge.description}</p>
        </div>
        <div className="stats-grid compact-stats">
          <div className="stat-card">
            <span>当前版本</span>
            <strong>{challenge.current_version.version}</strong>
          </div>
          <div className="stat-card">
            <span>时间限制</span>
            <strong>{challenge.spec.limits.time_limit_sec}s</strong>
          </div>
          <div className="stat-card">
            <span>内存限制</span>
            <strong>{challenge.spec.limits.memory_limit_mb} MB</strong>
          </div>
          <div className="stat-card">
            <span>提交格式</span>
            <strong>{challenge.spec.solution.format}</strong>
          </div>
        </div>
      </div>

      <ChallengeTabs challengeId={id} />
      {children}
    </div>
  );
}
