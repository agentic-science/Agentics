//! Agent registration and authentication queries.

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::db::pioneer_codes::{PioneerCodeRegistrationKind, consume_pioneer_code_for_agent_tx};
use agentics_domain::models::ids::{AgentId, AgentTokenId};
use agentics_error::{Result, ServiceError};

use super::ids::{agent_id_from_row, agent_token_id_from_row};

/// Input for creating an agent and its initial bearer token in one transaction.
#[derive(Debug, Clone)]
pub struct RegisterAgentInput {
    pub agent_id: AgentId,
    pub token_id: AgentTokenId,
    pub token_hash: String,
    pub display_name: String,
    pub agent_description: String,
    pub owner: String,
    pub model_info: Value,
}

/// Persisted agent row returned after registration.
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: AgentId,
    pub display_name: String,
    pub agent_description: String,
    pub owner: String,
    pub model_info: Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Agent identity resolved from a valid, active bearer token.
#[derive(Debug, Clone)]
pub struct AuthenticatedAgent {
    pub agent_id: AgentId,
    pub token_id: AgentTokenId,
    pub display_name: String,
}

/// Register an active agent and insert its first token.
pub async fn register_agent(
    pool: &PgPool,
    input: &RegisterAgentInput,
    max_active_agents: i64,
) -> Result<AgentRecord> {
    let mut tx = pool.begin().await?;
    enforce_active_agent_quota_tx(&mut tx, max_active_agents).await?;

    let agent = insert_agent_tx(&mut tx, input).await?;
    insert_agent_token_tx(&mut tx, input).await?;

    tx.commit().await?;

    Ok(agent)
}

/// Register an active agent while atomically consuming a pioneer code.
pub async fn register_agent_with_pioneer_code(
    pool: &PgPool,
    input: &RegisterAgentInput,
    pioneer_code_hash: &str,
    registration_kind: PioneerCodeRegistrationKind,
    max_active_agents: i64,
) -> Result<AgentRecord> {
    let mut tx = pool.begin().await?;
    enforce_active_agent_quota_tx(&mut tx, max_active_agents).await?;

    let agent = insert_agent_tx(&mut tx, input).await?;
    consume_pioneer_code_for_agent_tx(
        &mut tx,
        pioneer_code_hash,
        input.agent_id.as_str(),
        registration_kind,
    )
    .await?;
    insert_agent_token_tx(&mut tx, input).await?;

    tx.commit().await?;

    Ok(agent)
}

/// Serialize active-agent quota admission within the registration transaction.
pub(crate) async fn enforce_active_agent_quota_tx(
    tx: &mut Transaction<'_, Postgres>,
    max_active_agents: i64,
) -> Result<()> {
    lock_quota_scope(tx, "global:active-agents").await?;
    let active = count_active_agents_tx(tx).await?;
    if active >= max_active_agents {
        return Err(ServiceError::TooManyRequests(format!(
            "agent registration quota exceeded: {active} of {max_active_agents} active agents are already registered"
        )));
    }
    Ok(())
}

/// Lock one quota-admission scope for the lifetime of the current transaction.
async fn lock_quota_scope(tx: &mut Transaction<'_, Postgres>, scope: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO quota_admission_locks (scope)
        VALUES ($1)
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .bind(scope)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        SELECT scope
        FROM quota_admission_locks
        WHERE scope = $1
        FOR UPDATE
        "#,
    )
    .bind(scope)
    .fetch_one(&mut **tx)
    .await?;

    Ok(())
}

/// Count active agents inside a quota-locked registration transaction.
async fn count_active_agents_tx(tx: &mut Transaction<'_, Postgres>) -> Result<i64> {
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM agents WHERE status = 'active'")
            .fetch_one(&mut **tx)
            .await?;
    Ok(count)
}

/// Insert the agent row used by both public and pioneer-code registration.
async fn insert_agent_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &RegisterAgentInput,
) -> Result<AgentRecord> {
    let row = sqlx::query(
        r#"
        INSERT INTO agents (id, display_name, agent_description, owner, model_info, status)
        VALUES ($1::uuid, $2, $3, $4, $5, 'active')
        RETURNING id::text AS id, display_name, agent_description, owner, model_info, status, created_at
        "#,
    )
    .bind(input.agent_id.as_str())
    .bind(&input.display_name)
    .bind(&input.agent_description)
    .bind(&input.owner)
    .bind(&input.model_info)
    .fetch_one(&mut **tx)
    .await?;

    Ok(AgentRecord {
        id: agent_id_from_row(&row, "id")?,
        display_name: row.try_get("display_name")?,
        agent_description: row.try_get("agent_description")?,
        owner: row.try_get("owner")?,
        model_info: row.try_get("model_info")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Insert the first bearer token for a newly registered agent.
async fn insert_agent_token_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &RegisterAgentInput,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO agent_tokens (id, agent_id, token_hash) VALUES ($1::uuid, $2::uuid, $3)",
    )
    .bind(input.token_id.as_str())
    .bind(input.agent_id.as_str())
    .bind(&input.token_hash)
    .execute(&mut **tx)
    .await?;
    Ok(())
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
    token: &SecretString,
) -> Result<Option<AuthenticatedAgent>> {
    let token_hash = crate::auth::hash_agent_token(token.expose_secret());

    let row = sqlx::query(
        r#"
        SELECT a.id::text AS agent_id, t.id::text AS token_id, a.display_name
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

    let token_id = agent_token_id_from_row(&row, "token_id")?;
    sqlx::query("UPDATE agent_tokens SET last_used_at = NOW() WHERE id = $1::uuid")
        .bind(token_id.as_str())
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedAgent {
        agent_id: agent_id_from_row(&row, "agent_id")?,
        token_id,
        display_name: row.try_get("display_name")?,
    }))
}

/// Disable an agent and revoke all of its tokens.
pub async fn disable_agent(pool: &PgPool, agent_id: &str) -> Result<()> {
    let row = sqlx::query("UPDATE agents SET status = 'disabled' WHERE id = $1::uuid RETURNING id")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;

    if row.is_none() {
        return Err(ServiceError::NotFound);
    }

    sqlx::query(
        "UPDATE agent_tokens SET revoked_at = COALESCE(revoked_at, NOW()) WHERE agent_id = $1::uuid",
    )
    .bind(agent_id)
    .execute(pool)
    .await?;

    Ok(())
}
