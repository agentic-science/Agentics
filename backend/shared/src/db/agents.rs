//! Agent registration and authentication queries.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::{AppError, Result};

/// Input for creating an agent and its initial bearer token in one transaction.
#[derive(Debug, Clone)]
pub struct RegisterAgentInput {
    pub agent_id: String,
    pub token_id: String,
    pub token_hash: String,
    pub name: String,
    pub agent_description: String,
    pub owner: String,
    pub model_info: Value,
}

/// Persisted agent row returned after registration.
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub agent_description: String,
    pub owner: String,
    pub model_info: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Agent identity resolved from a valid, active bearer token.
#[derive(Debug, Clone)]
pub struct AuthenticatedAgent {
    pub agent_id: String,
    pub token_id: String,
    pub name: String,
}

/// Register an active agent and insert its first token.
pub async fn register_agent(pool: &PgPool, input: &RegisterAgentInput) -> Result<AgentRecord> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        INSERT INTO agents (id, name, agent_description, owner, model_info, status)
        VALUES ($1::uuid, $2, $3, $4, $5, 'active')
        RETURNING id::text AS id, name, agent_description, owner, model_info, status, created_at
        "#,
    )
    .bind(&input.agent_id)
    .bind(&input.name)
    .bind(&input.agent_description)
    .bind(&input.owner)
    .bind(&input.model_info)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO agent_tokens (id, agent_id, token_hash) VALUES ($1::uuid, $2::uuid, $3)",
    )
    .bind(&input.token_id)
    .bind(&input.agent_id)
    .bind(&input.token_hash)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(AgentRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        agent_description: row.try_get("agent_description")?,
        owner: row.try_get("owner")?,
        model_info: row.try_get("model_info")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Count currently active agents for coarse registration abuse controls.
pub async fn count_active_agents(pool: &PgPool) -> Result<i64> {
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM agents WHERE status = 'active'")
            .fetch_one(pool)
            .await?;

    Ok(count)
}

/// Authenticate a bearer token and refresh its `last_used_at` timestamp.
pub async fn authenticate_agent_token(
    pool: &PgPool,
    token: &str,
) -> Result<Option<AuthenticatedAgent>> {
    let token_hash = crate::auth::hash_agent_token(token);

    let row = sqlx::query(
        r#"
        SELECT a.id::text AS agent_id, t.id::text AS token_id, a.name
        FROM agent_tokens t
        JOIN agents a ON a.id = t.agent_id
        WHERE t.token_hash = $1
          AND t.revoked_at IS NULL
          AND a.status = 'active'
        LIMIT 1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let token_id: String = row.try_get("token_id")?;
    sqlx::query("UPDATE agent_tokens SET last_used_at = NOW() WHERE id = $1::uuid")
        .bind(&token_id)
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedAgent {
        agent_id: row.try_get("agent_id")?,
        token_id,
        name: row.try_get("name")?,
    }))
}

/// Disable an agent and revoke all of its tokens.
pub async fn disable_agent(pool: &PgPool, agent_id: &str) -> Result<()> {
    let row = sqlx::query("UPDATE agents SET status = 'disabled' WHERE id = $1::uuid RETURNING id")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;

    if row.is_none() {
        return Err(AppError::NotFound);
    }

    sqlx::query(
        "UPDATE agent_tokens SET revoked_at = COALESCE(revoked_at, NOW()) WHERE agent_id = $1::uuid",
    )
    .bind(agent_id)
    .execute(pool)
    .await?;

    Ok(())
}
