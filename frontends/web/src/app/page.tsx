import Link from "next/link";
import { fetchJson } from "@/lib/api";
import { problemListResponseSchema } from "@/lib/schemas";
import { formatDate } from "@/lib/format";

export default async function HomePage() {
  let problems: import("@/lib/schemas").ProblemListResponse;
  let error: string | null = null;

  try {
    problems = await fetchJson("/api/public/problems", problemListResponseSchema);
  } catch (e) {
    error = e instanceof Error ? e.message : "加载失败";
    problems = { items: [] };
  }

  return (
    <div className="page-grid">
      <aside className="workspace-panel">
        <h2 className="dense-panel h2">题目目录</h2>
        <div className="stats-grid" style={{ marginTop: 12 }}>
          <div className="stat-card">
            <span>题目数</span>
            <strong>{problems.items.length}</strong>
          </div>
          <div className="stat-card">
            <span>版本数</span>
            <strong>{problems.items.length}</strong>
          </div>
          <div className="stat-card">
            <span>提交格式</span>
            <strong>ZIP</strong>
            <small>Python 项目压缩包</small>
          </div>
          <div className="stat-card">
            <span>数据视图</span>
            <strong>公开+隐藏</strong>
            <small>Shown / Hidden / Heldout</small>
          </div>
        </div>
        <div style={{ marginTop: 16 }}>
          <p className="section-kicker">浏览建议</p>
          <ul className="bullet-list" style={{ marginTop: 8 }}>
            <li>
              <p>点击题目进入详情页，查看题面、提交记录和排行榜</p>
            </li>
            <li>
              <p>每个题目包含公开测试集（可查看详细结果）和隐藏测试集（仅分数）</p>
            </li>
            <li>
              <p>提交代码后系统会自动运行评测，支持公开评测和官方评测两种模式</p>
            </li>
          </ul>
        </div>
      </aside>

      <main className="workspace-panel">
        {error ? (
          <div className="empty-block">加载失败：{error}</div>
        ) : problems.items.length === 0 ? (
          <div className="empty-block">暂无题目</div>
        ) : (
          <div className="catalog-list">
            {problems.items.map((problem) => (
              <Link
                key={problem.id}
                href={`/problems/${problem.id}`}
                className="catalog-row"
              >
                <div className="catalog-main">
                  <div className="catalog-title-row">
                    <strong>{problem.title}</strong>
                    <span className="row-slug">{problem.slug}</span>
                  </div>
                  <p>{problem.description}</p>
                </div>
                <div className="catalog-meta">
                  <span className="pill">{problem.current_version.version}</span>
                  <span className="meta-link">进入详情 →</span>
                </div>
              </Link>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
