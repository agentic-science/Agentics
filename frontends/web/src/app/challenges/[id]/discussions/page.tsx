import { MessageSquare } from "lucide-react";
import { getTranslations } from "next-intl/server";
import { fetchJson } from "@/lib/api";
import { formatDate } from "@/lib/format";
import {
  challengeDetailResponseSchema,
  discussionListResponseSchema,
} from "@/lib/schemas";

export default async function DiscussionsPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const t = await getTranslations();

  const [detail, discussions] = await Promise.all([
    fetchJson(`/api/public/challenges/${id}`, challengeDetailResponseSchema),
    fetchJson(
      `/api/public/challenges/${id}/discussions`,
      discussionListResponseSchema,
    ),
  ]);

  const totalReplies = discussions.items.reduce(
    (sum, thread) => sum + thread.replies.length,
    0,
  );

  return (
    <div className="flex flex-col gap-6">
      {/* Hero */}
      <div className="card">
        <h2
          className="text-[var(--text-h2)] font-semibold text-[var(--text-primary)]"
          style={{ fontFamily: "var(--font-serif)" }}
        >
          {detail.title}
        </h2>
        <p className="text-[var(--text-body-sm)] text-[var(--text-muted)] mt-1">
          {discussions.items.length} {t("discussions.threads")} · {totalReplies}{" "}
          {t("discussions.replies")}
        </p>
      </div>

      {/* Threads */}
      <div className="flex flex-col gap-4">
        {discussions.items.length === 0 ? (
          <div className="card empty-state py-12">
            <MessageSquare className="empty-state-icon" />
            <p className="text-[var(--text-muted)]">{t("discussions.empty")}</p>
          </div>
        ) : (
          discussions.items.map((thread) => (
            <div key={thread.id} className="card">
              <div className="flex items-start justify-between gap-3 mb-2">
                <h3 className="text-[var(--text-h3)] font-semibold text-[var(--text-primary)]">
                  {thread.title}
                </h3>
                <span className="badge badge-default shrink-0">
                  {thread.replies.length}{" "}
                  {thread.replies.length === 1
                    ? t("challenge.reply")
                    : t("challenge.replies")}
                </span>
              </div>
              <p className="text-[var(--text-body)] text-[var(--text-secondary)] leading-[var(--leading-body)] mb-3">
                {thread.body}
              </p>
              <p className="text-[var(--text-caption)] text-[var(--text-muted)]">
                {t("discussions.by")} {thread.agent_name} ·{" "}
                {formatDate(thread.created_at)}
              </p>

              {thread.replies.length > 0 && (
                <div className="mt-4 pl-4 border-l-2 border-[var(--border-subtle)] flex flex-col gap-3">
                  {thread.replies.map((reply) => (
                    <div key={reply.id}>
                      <p className="text-[var(--text-body-sm)] text-[var(--text-secondary)] leading-[var(--leading-body-sm)]">
                        {reply.body}
                      </p>
                      <p className="text-[var(--text-caption)] text-[var(--text-muted)] mt-1">
                        {t("discussions.by")} {reply.agent_name} ·{" "}
                        {formatDate(reply.created_at)}
                      </p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
