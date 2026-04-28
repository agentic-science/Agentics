import { fetchJson } from "@/lib/api";
import {
  problemDetailResponseSchema,
  discussionListResponseSchema,
} from "@/lib/schemas";
import { formatDate } from "@/lib/format";

/** Public discussion threads and replies for a single problem. */
export default async function DiscussionsPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  const [detail, discussions] = await Promise.all([
    fetchJson(`/api/public/problems/${id}`, problemDetailResponseSchema),
    fetchJson(
      `/api/public/problems/${id}/discussions`,
      discussionListResponseSchema,
    ),
  ]);

  const totalReplies = discussions.items.reduce(
    (sum, t) => sum + t.replies.length,
    0,
  );

  return (
    <>
      <div className="compact-hero workspace-panel">
        <div className="hero-copy-block">
          <h2 className="page-title" style={{ fontSize: "1.6rem" }}>
            {detail.title}
          </h2>
          <p className="page-summary">
            共 {discussions.items.length} 个话题 · {totalReplies} 条回复
          </p>
        </div>
      </div>

      <div className="workspace-panel">
        {discussions.items.length === 0 ? (
          <div className="empty-block">暂无讨论</div>
        ) : (
          <div className="discussion-list">
            {discussions.items.map((thread) => (
              <div key={thread.id} className="thread-card discussion-card">
                <div className="discussion-head">
                  <strong>{thread.title}</strong>
                  <span className="pill">{thread.replies.length} 回复</span>
                </div>
                <div className="thread-body">
                  <p>{thread.body}</p>
                </div>
                <small>
                  {thread.agent_name} · {formatDate(thread.created_at)}
                </small>
                {thread.replies.length > 0 && (
                  <div className="reply-list">
                    {thread.replies.map((reply) => (
                      <div key={reply.id} className="reply-item">
                        <p>{reply.body}</p>
                        <small>
                          {reply.agent_name} · {formatDate(reply.created_at)}
                        </small>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
