import Link from "next/link";
import { fetchJson } from "@/lib/api";
import { challengeListResponseSchema } from "@/lib/schemas";

/** Home page that lists published challenges from the public API. */
export default async function HomePage() {
  let challenges: import("@/lib/schemas").ChallengeListResponse;
  let error: string | null = null;

  try {
    challenges = await fetchJson(
      "/api/public/challenges",
      challengeListResponseSchema,
    );
  } catch (e) {
    error = e instanceof Error ? e.message : "加载失败";
    challenges = { items: [] };
  }

  return (
    <div className="page-grid">
      <aside className="workspace-panel">
        <h2 className="dense-panel h2">题目目录</h2>
        <div className="stats-grid" style={{ marginTop: 12 }}>
          <div className="stat-card">
            <span>题目数</span>
            <strong>{challenges.items.length}</strong>
          </div>
          <div className="stat-card">
            <span>版本数</span>
            <strong>{challenges.items.length}</strong>
          </div>
          <div className="stat-card">
            <span>提交格式</span>
            <strong>ZIP</strong>
            <small>Python 项目压缩包</small>
          </div>
          <div className="stat-card">
            <span>数据视图</span>
            <strong>公开+私有</strong>
            <small>Public / Private Benchmark</small>
          </div>
        </div>
        <div style={{ marginTop: 16 }}>
          <p className="section-kicker">浏览建议</p>
          <ul className="bullet-list" style={{ marginTop: 8 }}>
            <li>
              <p>点击题目进入详情页，查看题面、提交记录和排行榜</p>
            </li>
            <li>
              <p>
                每个题目包含公开测试集（可查看详细结果）和隐藏测试集（仅分数）
              </p>
            </li>
            <li>
              <p>
                提交代码后系统会自动运行评测，支持公开评测和官方评测两种模式
              </p>
            </li>
          </ul>
        </div>
      </aside>

      <main className="workspace-panel">
        {error ? (
          <div className="empty-block">加载失败：{error}</div>
        ) : challenges.items.length === 0 ? (
          <div className="empty-block">暂无题目</div>
        ) : (
          <div className="catalog-list">
            {challenges.items.map((challenge) => (
              <Link
                key={challenge.id}
                href={`/challenges/${challenge.id}`}
                className="catalog-row"
              >
                <div className="catalog-main">
                  <div className="catalog-title-row">
                    <strong>{challenge.title}</strong>
                    <span className="row-slug">{challenge.slug}</span>
                  </div>
                  <p>{challenge.description}</p>
                </div>
                <div className="catalog-meta">
                  <span className="pill">
                    {challenge.current_version.version}
                  </span>
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
