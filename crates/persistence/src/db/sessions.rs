//! Browser session and GitHub OAuth state queries.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::db::agents::enforce_active_agent_quota_tx;
use crate::db::pioneer_codes::{PioneerCodeRegistrationKind, consume_pioneer_code_for_agent_tx};
use agentics_domain::models::ids::AgentId;
use agentics_domain::models::pioneer_codes::INVALID_OR_UNAVAILABLE_PIONEER_CODE;
use agentics_error::{Result, ServiceError};

use super::ids::agent_id_from_row;

/// Role attached to a browser session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSessionRole {
    Creator,
    Admin,
}

impl WebSessionRole {
    /// Stable database string for this web-session role.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Creator => "creator",
            Self::Admin => "admin",
        }
    }
}

/// Persisted creator identity resolved from a browser session.
#[derive(Debug, Clone)]
pub struct AuthenticatedCreatorSession {
    pub session_id: String,
    pub agent_id: String,
    pub github_user_id: i64,
    pub github_login: String,
    pub csrf_token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Persisted admin identity resolved from a browser session.
#[derive(Debug, Clone)]
pub struct AuthenticatedAdminSession {
    pub session_id: String,
    pub admin_username: String,
    pub csrf_token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Input for inserting a browser session.
#[derive(Debug, Clone)]
pub struct CreateCreatorSessionInput {
    pub session_id: String,
    pub session_token_hash: String,
    pub csrf_token_hash: String,
    pub agent_id: String,
    pub github_user_id: i64,
    pub github_login: String,
    pub expires_at: DateTime<Utc>,
}

/// Input for inserting an admin browser session.
#[derive(Debug, Clone)]
pub struct CreateAdminSessionInput {
    pub session_id: String,
    pub session_token_hash: String,
    pub csrf_token_hash: String,
    pub admin_username: String,
    pub expires_at: DateTime<Utc>,
}

/// Input for storing a short-lived GitHub OAuth state token.
#[derive(Debug, Clone)]
pub struct CreateGithubOauthStateInput {
    pub state_hash: String,
    pub browser_nonce_hash: String,
    pub pioneer_code_hash: Option<String>,
    pub expires_at: DateTime<Utc>,
}

/// Stored OAuth state consumed by a callback after browser redirect.
#[derive(Debug, Clone)]
pub struct ConsumedGithubOauthState {
    pub pioneer_code_hash: Option<String>,
}

/// Upsert an internal account row for a verified GitHub creator.
///
/// The challenge-creation schema still stores creator ownership through the
/// existing `agents` table. OAuth sessions are the authority; this shadow row
/// keeps foreign-key ownership stable without issuing an agent bearer token.
pub async fn upsert_github_creator_agent(
    pool: &PgPool,
    agent_id: &AgentId,
    github_user_id: i64,
    github_login: &str,
    max_active_agents: i64,
) -> Result<AgentId> {
    upsert_github_creator_agent_with_pioneer_code(
        pool,
        agent_id,
        github_user_id,
        github_login,
        None,
        false,
        max_active_agents,
    )
    .await
}

/// Upsert an internal GitHub creator account and consume a pioneer code only
/// when this OAuth identity creates a new agent row.
pub async fn upsert_github_creator_agent_with_pioneer_code(
    pool: &PgPool,
    agent_id: &AgentId,
    github_user_id: i64,
    github_login: &str,
    pioneer_code_hash: Option<&str>,
    pioneer_code_required_for_new_agent: bool,
    max_active_agents: i64,
) -> Result<AgentId> {
    let mut tx = pool.begin().await?;

    let existing = sqlx::query(
        r#"
        SELECT id::text AS id, status
        FROM agents
        WHERE github_user_id = $1
        FOR UPDATE
        "#,
    )
    .bind(github_user_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(row) = existing {
        let id = agent_id_from_row(&row, "id")?;
        let status: String = row.try_get("status")?;
        if status != "active" {
            return Err(ServiceError::Forbidden(
                "linked GitHub creator agent is disabled".to_string(),
            ));
        }
        sqlx::query(
            r#"
            UPDATE agents
            SET github_login = $1,
                display_name = $1,
                owner = $2
            WHERE id = $3::uuid
            "#,
        )
        .bind(github_login.trim())
        .bind(format!("github:{github_login}"))
        .bind(id.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(id);
    }

    if pioneer_code_required_for_new_agent && pioneer_code_hash.is_none() {
        return Err(ServiceError::Forbidden(
            INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string(),
        ));
    }

    enforce_active_agent_quota_tx(&mut tx, max_active_agents).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO agents (
            id,
            display_name,
            agent_description,
            owner,
            model_info,
            status,
            github_user_id,
            github_login
        )
        VALUES ($1::uuid, $2, '', $3, '{}'::jsonb, 'active', $4, $5)
        RETURNING id::text AS id
        "#,
    )
    .bind(agent_id.as_str())
    .bind(github_login.trim())
    .bind(format!("github:{github_login}"))
    .bind(github_user_id)
    .bind(github_login.trim())
    .fetch_one(&mut *tx)
    .await?;

    if let Some(code_hash) = pioneer_code_hash {
        consume_pioneer_code_for_agent_tx(
            &mut tx,
            code_hash,
            agent_id.as_str(),
            PioneerCodeRegistrationKind::CreatorOauth,
        )
        .await?;
    }

    tx.commit().await?;

    agent_id_from_row(&row, "id")
}

/// Store a GitHub OAuth state token hash for callback validation.
pub async fn create_github_oauth_state(
    pool: &PgPool,
    input: &CreateGithubOauthStateInput,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO github_oauth_states (state_hash, browser_nonce_hash, pioneer_code_hash, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&input.state_hash)
    .bind(&input.browser_nonce_hash)
    .bind(&input.pioneer_code_hash)
    .bind(input.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Consume one non-expired GitHub OAuth state token.
pub async fn consume_github_oauth_state(
    pool: &PgPool,
    state_hash: &str,
    browser_nonce_hash: &str,
) -> Result<Option<ConsumedGithubOauthState>> {
    let row = sqlx::query(
        r#"
        DELETE FROM github_oauth_states
        WHERE state_hash = $1
          AND browser_nonce_hash = $2
          AND expires_at > NOW()
        RETURNING pioneer_code_hash
        "#,
    )
    .bind(state_hash)
    .bind(browser_nonce_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(ConsumedGithubOauthState {
        pioneer_code_hash: row.try_get("pioneer_code_hash")?,
    }))
}

/// Create a browser session for a verified GitHub creator.
pub async fn create_creator_session(
    pool: &PgPool,
    input: &CreateCreatorSessionInput,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO web_sessions (
            id,
            role,
            session_token_hash,
            csrf_token_hash,
            agent_id,
            github_user_id,
            github_login,
            expires_at
        )
        VALUES ($1::uuid, 'creator', $2, $3, $4::uuid, $5, $6, $7)
        "#,
    )
    .bind(&input.session_id)
    .bind(&input.session_token_hash)
    .bind(&input.csrf_token_hash)
    .bind(&input.agent_id)
    .bind(input.github_user_id)
    .bind(input.github_login.trim())
    .bind(input.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Create a browser session for an administrator.
pub async fn create_admin_session(pool: &PgPool, input: &CreateAdminSessionInput) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO web_sessions (
            id,
            role,
            session_token_hash,
            csrf_token_hash,
            admin_username,
            expires_at
        )
        VALUES ($1::uuid, 'admin', $2, $3, $4, $5)
        "#,
    )
    .bind(&input.session_id)
    .bind(&input.session_token_hash)
    .bind(&input.csrf_token_hash)
    .bind(input.admin_username.trim())
    .bind(input.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Authenticate a creator session token and refresh its last-used timestamp.
pub async fn authenticate_creator_session(
    pool: &PgPool,
    session_token: &str,
) -> Result<Option<AuthenticatedCreatorSession>> {
    let session_token_hash = crate::auth::hash_opaque_token(session_token);
    let row = sqlx::query(
        r#"
        SELECT id::text AS id, agent_id::text AS agent_id, github_user_id, github_login, csrf_token_hash, expires_at
        FROM web_sessions
        WHERE session_token_hash = $1
          AND role = 'creator'
          AND expires_at > NOW()
        LIMIT 1
        "#,
    )
    .bind(&session_token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let session_id: String = row.try_get("id")?;
    sqlx::query("UPDATE web_sessions SET last_used_at = NOW() WHERE id = $1::uuid")
        .bind(&session_id)
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedCreatorSession {
        session_id,
        agent_id: row
            .try_get::<Option<String>, _>("agent_id")?
            .ok_or_else(|| {
                ServiceError::Internal("creator session missing agent id".to_string())
            })?,
        github_user_id: row
            .try_get::<Option<i64>, _>("github_user_id")?
            .ok_or_else(|| {
                ServiceError::Internal("creator session missing GitHub user id".to_string())
            })?,
        github_login: row.try_get("github_login")?,
        csrf_token_hash: row.try_get("csrf_token_hash")?,
        expires_at: row.try_get("expires_at")?,
    }))
}

/// Authenticate an admin session token and refresh its last-used timestamp.
pub async fn authenticate_admin_session(
    pool: &PgPool,
    session_token: &str,
) -> Result<Option<AuthenticatedAdminSession>> {
    let session_token_hash = crate::auth::hash_opaque_token(session_token);
    let row = sqlx::query(
        r#"
        SELECT id::text AS id, admin_username, csrf_token_hash, expires_at
        FROM web_sessions
        WHERE session_token_hash = $1
          AND role = 'admin'
          AND expires_at > NOW()
        LIMIT 1
        "#,
    )
    .bind(&session_token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let session_id: String = row.try_get("id")?;
    sqlx::query("UPDATE web_sessions SET last_used_at = NOW() WHERE id = $1::uuid")
        .bind(&session_id)
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedAdminSession {
        session_id,
        admin_username: row
            .try_get::<Option<String>, _>("admin_username")?
            .ok_or_else(|| ServiceError::Internal("admin session missing username".to_string()))?,
        csrf_token_hash: row.try_get("csrf_token_hash")?,
        expires_at: row.try_get("expires_at")?,
    }))
}

/// Delete a browser session by the bearer cookie token.
pub async fn delete_web_session_by_token(pool: &PgPool, session_token: &str) -> Result<()> {
    let session_token_hash = crate::auth::hash_opaque_token(session_token);
    sqlx::query("DELETE FROM web_sessions WHERE session_token_hash = $1")
        .bind(session_token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete expired transient auth rows.
pub async fn delete_expired_web_auth_rows(pool: &PgPool) -> Result<()> {
    sqlx::query("DELETE FROM github_oauth_states WHERE expires_at <= NOW()")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM web_sessions WHERE expires_at <= NOW()")
        .execute(pool)
        .await?;
    Ok(())
}
