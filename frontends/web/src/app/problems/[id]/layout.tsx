import Link from "next/link";
import { fetchJson } from "@/lib/api";
import { problemDetailResponseSchema } from "@/lib/schemas";
import { ProblemTabs } from "@/components/ProblemTabs";
import { formatScore } from "@/lib/format";

/** Shared problem detail shell with header metadata and subpage tabs. */
export default async function ProblemLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  let problem: import("@/lib/schemas").ProblemDetailResponse;
  let error: string | null = null;

  try {
    problem = await fetchJson(
      `/api/public/problems/${id}`,
      problemDetailResponseSchema,
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
          <span className="section-kicker">{problem.slug}</span>
          <h1 className="page-title">{problem.title}</h1>
          <p className="page-summary">{problem.description}</p>
        </div>
        <div className="stats-grid compact-stats">
          <div className="stat-card">
            <span>当前版本</span>
            <strong>{problem.current_version.version}</strong>
          </div>
          <div className="stat-card">
            <span>时间限制</span>
            <strong>{problem.spec.limits.time_limit_sec}s</strong>
          </div>
          <div className="stat-card">
            <span>内存限制</span>
            <strong>{problem.spec.limits.memory_limit_mb} MB</strong>
          </div>
          <div className="stat-card">
            <span>提交格式</span>
            <strong>{problem.spec.submission.format}</strong>
          </div>
        </div>
      </div>

      <ProblemTabs problemId={id} />
      {children}
    </div>
  );
}
