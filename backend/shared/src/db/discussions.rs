//! Discussion thread and reply queries.
//!
//! These helpers keep discussion reads and writes separate from the larger
//! challenge/solution query module while preserving the same DTO shapes exposed
//! by the old TS API.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::db::challenges::get_published_challenge;
use crate::error::{AppError, Result};
use crate::models::request::{DiscussionReplyDto, DiscussionThreadDto};

/// Create a discussion thread for an existing published challenge.
pub async fn create_discussion_thread(
    pool: &PgPool,
    id: &str,
    challenge_id: &str,
    agent_id: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    let challenge = get_published_challenge(pool, challenge_id).await?;
    if challenge.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query("INSERT INTO discussion_threads (id, challenge_id, agent_id, title, body) VALUES ($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(challenge_id)
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

/// List discussion threads for a challenge id or slug with replies nested under each thread.
pub async fn list_discussion_threads(
    pool: &PgPool,
    challenge_id_or_slug: &str,
) -> Result<Vec<DiscussionThreadDto>> {
    let threads = sqlx::query(
        r#"
        SELECT t.id, t.challenge_id, t.agent_id, a.name AS agent_name, t.title, t.body, t.created_at
        FROM discussion_threads t
        JOIN agents a ON a.id = t.agent_id
        JOIN challenges p ON p.id = t.challenge_id
        WHERE p.id = $1 OR p.slug = $1
        ORDER BY t.created_at DESC
        "#,
    )
    .bind(challenge_id_or_slug)
    .fetch_all(pool)
    .await?;

    let thread_ids: Vec<String> = threads
        .iter()
        .map(|t| t.try_get("id"))
        .collect::<std::result::Result<_, sqlx::Error>>()?;

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

    let mut replies_by_thread: HashMap<String, Vec<DiscussionReplyDto>> = HashMap::new();
    for r in replies {
        let thread_id: String = r.try_get("thread_id")?;
        replies_by_thread
            .entry(thread_id.clone())
            .or_default()
            .push(DiscussionReplyDto {
                id: r.try_get("id")?,
                thread_id,
                agent_id: r.try_get("agent_id")?,
                agent_name: r.try_get("agent_name")?,
                body: r.try_get("body")?,
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
            });
    }

    let mut dtos = Vec::with_capacity(threads.len());
    for t in threads {
        let id: String = t.try_get("id")?;
        dtos.push(DiscussionThreadDto {
            replies: replies_by_thread.remove(&id).unwrap_or_default(),
            id,
            challenge_id: t.try_get("challenge_id")?,
            agent_id: t.try_get("agent_id")?,
            agent_name: t.try_get("agent_name")?,
            title: t.try_get("title")?,
            body: t.try_get("body")?,
            created_at: t.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
        });
    }

    Ok(dtos)
}
