//! Persistence for pioneer-code gated agent registration.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::ids::{AgentId, AgentPioneerCodeId};
use crate::models::pioneer_codes::INVALID_OR_UNAVAILABLE_PIONEER_CODE;

/// Registration flow that consumed a pioneer code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PioneerCodeRegistrationKind {
    /// Direct agent registration through `/api/agents/register`.
    AgentApi,
    /// Creator account creation during GitHub OAuth callback.
    CreatorOauth,
}

impl PioneerCodeRegistrationKind {
    /// Return the storage value persisted with a pioneer-code use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentApi => "agent_api",
            Self::CreatorOauth => "creator_oauth",
        }
    }
}

/// Input used by admins to create a pioneer code.
#[derive(Debug, Clone)]
pub struct CreatePioneerCodeInput {
    pub id: AgentPioneerCodeId,
    pub code_display: String,
    pub code_hash: String,
    pub label: Option<String>,
    pub note: String,
    pub max_uses: i64,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by_admin_username: String,
}

/// Persisted pioneer-code row returned to admins.
#[derive(Debug, Clone)]
pub struct PioneerCodeRecord {
    pub id: AgentPioneerCodeId,
    pub code_display: String,
    pub label: Option<String>,
    pub note: String,
    pub max_uses: i64,
    pub use_count: i64,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by_admin_username: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// One agent account that was created with a pioneer code.
#[derive(Debug, Clone)]
pub struct PioneerCodeUseRecord {
    pub agent_id: AgentId,
    pub agent_display_name: String,
    pub registration_kind: String,
    pub used_at: DateTime<Utc>,
}

/// Result returned after revoking a pioneer code and disabling derived agents.
#[derive(Debug, Clone)]
pub struct RevokePioneerCodeOutcome {
    pub revoked_agent_count: i64,
    pub revoked_token_count: i64,
}

/// Insert a newly generated or admin-supplied pioneer code.
pub async fn create_pioneer_code(
    pool: &PgPool,
    input: &CreatePioneerCodeInput,
) -> Result<PioneerCodeRecord> {
    let row = sqlx::query(
        r#"
        INSERT INTO agent_pioneer_codes (
            id,
            code_display,
            code_hash,
            label,
            note,
            max_uses,
            expires_at,
            created_by_admin_username
        )
        VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            id::text AS id,
            code_display,
            label,
            note,
            max_uses,
            use_count,
            status,
            expires_at,
            created_by_admin_username,
            created_at,
            revoked_at
        "#,
    )
    .bind(input.id.as_str())
    .bind(&input.code_display)
    .bind(&input.code_hash)
    .bind(&input.label)
    .bind(&input.note)
    .bind(input.max_uses)
    .bind(input.expires_at)
    .bind(&input.created_by_admin_username)
    .fetch_one(pool)
    .await?;

    pioneer_code_record_from_row(&row)
}

/// List pioneer codes for the admin console.
pub async fn list_pioneer_codes(pool: &PgPool) -> Result<Vec<PioneerCodeRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id::text AS id,
            code_display,
            label,
            note,
            max_uses,
            use_count,
            status,
            expires_at,
            created_by_admin_username,
            created_at,
            revoked_at
        FROM agent_pioneer_codes
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.iter().map(pioneer_code_record_from_row).collect()
}

/// Fetch a pioneer code and all agents created through it.
pub async fn get_pioneer_code_detail(
    pool: &PgPool,
    id: &AgentPioneerCodeId,
) -> Result<(PioneerCodeRecord, Vec<PioneerCodeUseRecord>)> {
    let code_row = sqlx::query(
        r#"
        SELECT
            id::text AS id,
            code_display,
            label,
            note,
            max_uses,
            use_count,
            status,
            expires_at,
            created_by_admin_username,
            created_at,
            revoked_at
        FROM agent_pioneer_codes
        WHERE id = $1::uuid
        "#,
    )
    .bind(id.as_str())
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let use_rows = sqlx::query(
        r#"
        SELECT
            u.agent_id::text AS agent_id,
            a.display_name AS agent_display_name,
            u.registration_kind,
            u.used_at
        FROM agent_pioneer_code_uses u
        JOIN agents a ON a.id = u.agent_id
        WHERE u.pioneer_code_id = $1::uuid
        ORDER BY u.used_at DESC
        "#,
    )
    .bind(id.as_str())
    .fetch_all(pool)
    .await?;

    let uses = use_rows
        .iter()
        .map(pioneer_code_use_from_row)
        .collect::<Result<_>>()?;
    Ok((pioneer_code_record_from_row(&code_row)?, uses))
}

/// Consume a pioneer code inside the transaction that creates the agent.
pub async fn consume_pioneer_code_for_agent_tx(
    tx: &mut Transaction<'_, Postgres>,
    code_hash: &str,
    agent_id: &str,
    registration_kind: PioneerCodeRegistrationKind,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT id::text AS id, max_uses, use_count, status, expires_at
        FROM agent_pioneer_codes
        WHERE code_hash = $1
        FOR UPDATE
        "#,
    )
    .bind(code_hash)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else {
        return Err(unavailable_pioneer_code());
    };

    let status: String = row.try_get("status")?;
    let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at")?;
    let max_uses: i64 = row.try_get("max_uses")?;
    let use_count: i64 = row.try_get("use_count")?;
    if status != "active"
        || expires_at.is_some_and(|expires_at| Utc::now() >= expires_at)
        || (max_uses != -1 && use_count >= max_uses)
    {
        return Err(unavailable_pioneer_code());
    }

    let pioneer_code_id: String = row.try_get("id")?;
    sqlx::query(
        r#"
        INSERT INTO agent_pioneer_code_uses (pioneer_code_id, agent_id, registration_kind)
        VALUES ($1::uuid, $2::uuid, $3)
        "#,
    )
    .bind(&pioneer_code_id)
    .bind(agent_id)
    .bind(registration_kind.as_str())
    .execute(&mut **tx)
    .await?;

    sqlx::query("UPDATE agent_pioneer_codes SET use_count = use_count + 1 WHERE id = $1::uuid")
        .bind(&pioneer_code_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Revoke a pioneer code and disable every agent account created through it.
pub async fn revoke_pioneer_code(
    pool: &PgPool,
    id: &AgentPioneerCodeId,
) -> Result<RevokePioneerCodeOutcome> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        UPDATE agent_pioneer_codes
        SET status = 'revoked',
            revoked_at = COALESCE(revoked_at, NOW())
        WHERE id = $1::uuid
        RETURNING id
        "#,
    )
    .bind(id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    if row.is_none() {
        return Err(AppError::NotFound);
    }

    let agent_id_rows = sqlx::query(
        r#"
        SELECT agent_id
        FROM agent_pioneer_code_uses
        WHERE pioneer_code_id = $1::uuid
        "#,
    )
    .bind(id.as_str())
    .fetch_all(&mut *tx)
    .await?;
    let agent_ids = agent_id_rows
        .iter()
        .map(|row| row.try_get::<Uuid, _>("agent_id"))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let revoked_agent_count = if agent_ids.is_empty() {
        0
    } else {
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET status = 'disabled'
            WHERE id = ANY($1::uuid[])
              AND status = 'active'
            "#,
        )
        .bind(&agent_ids)
        .execute(&mut *tx)
        .await?;
        i64::try_from(result.rows_affected())
            .map_err(|_| AppError::Internal("revoked agent count overflow".to_string()))?
    };

    let revoked_token_count = if agent_ids.is_empty() {
        0
    } else {
        let result = sqlx::query(
            r#"
            UPDATE agent_tokens
            SET revoked_at = COALESCE(revoked_at, NOW())
            WHERE agent_id = ANY($1::uuid[])
              AND revoked_at IS NULL
            "#,
        )
        .bind(&agent_ids)
        .execute(&mut *tx)
        .await?;
        i64::try_from(result.rows_affected())
            .map_err(|_| AppError::Internal("revoked token count overflow".to_string()))?
    };

    tx.commit().await?;

    Ok(RevokePioneerCodeOutcome {
        revoked_agent_count,
        revoked_token_count,
    })
}

/// Convert a unavailable-code condition into the public generic error.
fn unavailable_pioneer_code() -> AppError {
    AppError::Forbidden(INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string())
}

/// Parse a pioneer-code row into the typed DB record.
fn pioneer_code_record_from_row(row: &sqlx::postgres::PgRow) -> Result<PioneerCodeRecord> {
    let id: String = row.try_get("id")?;
    Ok(PioneerCodeRecord {
        id: AgentPioneerCodeId::try_new(id)
            .map_err(|e| AppError::Internal(format!("stored invalid pioneer code id: {e}")))?,
        code_display: row.try_get("code_display")?,
        label: row.try_get("label")?,
        note: row.try_get("note")?,
        max_uses: row.try_get("max_uses")?,
        use_count: row.try_get("use_count")?,
        status: row.try_get("status")?,
        expires_at: row.try_get("expires_at")?,
        created_by_admin_username: row.try_get("created_by_admin_username")?,
        created_at: row.try_get("created_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

/// Parse a pioneer-code use row into the typed DB record.
fn pioneer_code_use_from_row(row: &sqlx::postgres::PgRow) -> Result<PioneerCodeUseRecord> {
    let agent_id: String = row.try_get("agent_id")?;
    Ok(PioneerCodeUseRecord {
        agent_id: AgentId::try_new(agent_id).map_err(|e| {
            AppError::Internal(format!("stored invalid pioneer-code agent id: {e}"))
        })?,
        agent_display_name: row.try_get("agent_display_name")?,
        registration_kind: row.try_get("registration_kind")?,
        used_at: row.try_get("used_at")?,
    })
}
