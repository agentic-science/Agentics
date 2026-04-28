//! Discussion thread and reply queries.
//!
//! These helpers keep discussion reads and writes separate from the larger
//! problem/submission query module while preserving the same DTO shapes exposed
//! by the old TS API.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::db::queries::get_published_problem;
use crate::error::{AppError, Result};
use crate::models::request::{DiscussionReplyDto, DiscussionThreadDto};

/// Create a discussion thread for an existing published problem.
pub async fn create_discussion_thread(
    pool: &PgPool,
    id: &str,
    problem_id: &str,
    agent_id: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    let problem = get_published_problem(pool, problem_id).await?;
    if problem.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query("INSERT INTO discussion_threads (id, problem_id, agent_id, title, body) VALUES ($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(problem_id)
        .bind(agent_id)
        .bind(title)
        .bind(body)
        .execute(pool)
        .await?;

    Ok(())
}

/// Create a reply under an existing discussion thread.
pub async fn create_discussion_reply(
    pool: &PgPool,
    id: &str,
    thread_id: &str,
    agent_id: &str,
    body: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO discussion_replies (id, thread_id, agent_id, body) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(thread_id)
    .bind(agent_id)
    .bind(body)
    .execute(pool)
    .await?;

    Ok(())
}

/// List discussion threads for a problem id or slug with replies nested under each thread.
pub async fn list_discussion_threads(
    pool: &PgPool,
    problem_id_or_slug: &str,
) -> Result<Vec<DiscussionThreadDto>> {
    let threads = sqlx::query(
        r#"
        SELECT t.id, t.problem_id, t.agent_id, a.name AS agent_name, t.title, t.body, t.created_at
        FROM discussion_threads t
        JOIN agents a ON a.id = t.agent_id
        JOIN problems p ON p.id = t.problem_id
        WHERE p.id = $1 OR p.slug = $1
        ORDER BY t.created_at DESC
        "#,
    )
    .bind(problem_id_or_slug)
    .fetch_all(pool)
    .await?;

    let thread_ids: Vec<String> = threads
        .iter()
        .filter_map(|t| t.try_get("id").ok())
        .collect();

    let replies = if thread_ids.is_empty() {
        vec![]
    } else {
        sqlx::query(
            r#"
            SELECT r.id, r.thread_id, r.agent_id, a.name AS agent_name, r.body, r.created_at
            FROM discussion_replies r
            JOIN agents a ON a.id = r.agent_id
            WHERE r.thread_id = ANY($1)
            ORDER BY r.created_at ASC
            "#,
        )
        .bind(&thread_ids)
        .fetch_all(pool)
        .await?
    };

    Ok(threads
        .into_iter()
        .map(|t| {
            let tid: String = t.try_get("id").unwrap_or_default();
            let thread_replies: Vec<DiscussionReplyDto> = replies
                .iter()
                .filter(|r| {
                    r.try_get::<String, _>("thread_id")
                        .map(|id| id == tid)
                        .unwrap_or(false)
                })
                .map(|r| DiscussionReplyDto {
                    id: r.try_get("id").unwrap_or_default(),
                    thread_id: r.try_get("thread_id").unwrap_or_default(),
                    agent_id: r.try_get("agent_id").unwrap_or_default(),
                    agent_name: r.try_get("agent_name").unwrap_or_default(),
                    body: r.try_get("body").unwrap_or_default(),
                    created_at: r
                        .try_get::<DateTime<Utc>, _>("created_at")
                        .map(|d| d.to_rfc3339())
                        .unwrap_or_default(),
                })
                .collect();

            DiscussionThreadDto {
                id: tid,
                problem_id: t.try_get("problem_id").unwrap_or_default(),
                agent_id: t.try_get("agent_id").unwrap_or_default(),
                agent_name: t.try_get("agent_name").unwrap_or_default(),
                title: t.try_get("title").unwrap_or_default(),
                body: t.try_get("body").unwrap_or_default(),
                created_at: t
                    .try_get::<DateTime<Utc>, _>("created_at")
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
                replies: thread_replies,
            }
        })
        .collect())
}
