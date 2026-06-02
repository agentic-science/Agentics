//! Persistence for pioneer-code gated agent registration.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use agentics_domain::models::ids::{AdminServiceTokenId, AgentId, HumanId, PioneerCodeId};
use agentics_domain::models::pioneer_codes::{
    INVALID_OR_UNAVAILABLE_PIONEER_CODE, PioneerCodeSubjectKind,
};
use agentics_error::{Result, ServiceError};

/// Registration flow that consumed a pioneer code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PioneerCodeRegistrationKind {
    /// Human account creation during GitHub OAuth callback.
    HumanGithubOauth,
    /// Direct agent registration through `/api/agents/register`.
    AgentApi,
}

impl PioneerCodeRegistrationKind {
    /// Return the storage value persisted with a pioneer-code use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HumanGithubOauth => "human_github_oauth",
            Self::AgentApi => "agent_api",
        }
    }
}

/// Input used by admins to create a pioneer code.
#[derive(Debug, Clone)]
pub struct CreatePioneerCodeInput {
    pub id: PioneerCodeId,
    pub code_display: String,
    pub code_hash: String,
    pub label: Option<String>,
    pub note: String,
    pub max_uses: i64,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by_human_id: Option<HumanId>,
    pub created_by_admin_service_token_id: Option<AdminServiceTokenId>,
    pub created_by_display: String,
}

/// Persisted pioneer-code row returned to admins.
#[derive(Debug, Clone)]
pub struct PioneerCodeRecord {
    pub id: PioneerCodeId,
    pub code_display: String,
    pub label: Option<String>,
    pub note: String,
    pub max_uses: i64,
    pub use_count: i64,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by_display: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// One account that was created with a pioneer code.
#[derive(Debug, Clone)]
pub struct PioneerCodeUseRecord {
    pub subject_kind: PioneerCodeSubjectKind,
    pub human_id: Option<HumanId>,
    pub human_github_login: Option<String>,
    pub agent_id: Option<AgentId>,
    pub agent_display_name: Option<String>,
    pub registration_kind: String,
    pub used_at: DateTime<Utc>,
}

/// Result returned after revoking a pioneer code and disabling derived agents.
#[derive(Debug, Clone)]
pub struct RevokePioneerCodeOutcome {
    pub revoked_human_count: i64,
    pub revoked_human_session_count: i64,
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
        INSERT INTO pioneer_codes (
            id,
            code_display,
            code_hash,
            label,
            note,
            max_uses,
            expires_at,
            created_by_human_id,
            created_by_admin_service_token_id,
            created_by_display
        )
        VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8::uuid, $9::uuid, $10)
        RETURNING
            id::text AS id,
            code_display,
            label,
            note,
            max_uses,
            use_count,
            status,
            expires_at,
            created_by_display,
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
    .bind(input.created_by_human_id.as_ref().map(HumanId::as_str))
    .bind(
        input
            .created_by_admin_service_token_id
            .as_ref()
            .map(AdminServiceTokenId::as_str),
    )
    .bind(&input.created_by_display)
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
            created_by_display,
            created_at,
            revoked_at
        FROM pioneer_codes
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
    id: &PioneerCodeId,
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
            created_by_display,
            created_at,
            revoked_at
        FROM pioneer_codes
        WHERE id = $1::uuid
        "#,
    )
    .bind(id.as_str())
    .fetch_optional(pool)
    .await?
    .ok_or(ServiceError::NotFound)?;

    let use_rows = sqlx::query(
        r#"
        SELECT
            u.subject_kind,
            u.human_id::text AS human_id,
            h_e.provider_login AS human_github_login,
            u.agent_id::text AS agent_id,
            a.display_name AS agent_display_name,
            u.registration_kind,
            u.used_at
        FROM pioneer_code_uses u
        LEFT JOIN agents a ON a.id = u.agent_id
        LEFT JOIN human_external_identities h_e ON h_e.human_id = u.human_id AND h_e.provider = 'github'
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

/// Verify that a pioneer code can currently start a registration flow.
pub async fn ensure_pioneer_code_available(pool: &PgPool, code_hash: &str) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT max_uses, use_count, status, expires_at
        FROM pioneer_codes
        WHERE code_hash = $1
        "#,
    )
    .bind(code_hash)
    .fetch_optional(pool)
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

    Ok(())
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
        FROM pioneer_codes
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
        INSERT INTO pioneer_code_uses (
            id,
            pioneer_code_id,
            subject_kind,
            agent_id,
            registration_kind
        )
        VALUES ($1::uuid, $2::uuid, 'agent', $3::uuid, $4)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&pioneer_code_id)
    .bind(agent_id)
    .bind(registration_kind.as_str())
    .execute(&mut **tx)
    .await?;

    sqlx::query("UPDATE pioneer_codes SET use_count = use_count + 1 WHERE id = $1::uuid")
        .bind(&pioneer_code_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Consume a pioneer code inside the transaction that creates the human.
pub async fn consume_pioneer_code_for_human_tx(
    tx: &mut Transaction<'_, Postgres>,
    code_hash: &str,
    human_id: &str,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT id::text AS id, max_uses, use_count, status, expires_at
        FROM pioneer_codes
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
        INSERT INTO pioneer_code_uses (
            id,
            pioneer_code_id,
            subject_kind,
            human_id,
            registration_kind
        )
        VALUES ($1::uuid, $2::uuid, 'human', $3::uuid, $4)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&pioneer_code_id)
    .bind(human_id)
    .bind(PioneerCodeRegistrationKind::HumanGithubOauth.as_str())
    .execute(&mut **tx)
    .await?;

    sqlx::query("UPDATE pioneer_codes SET use_count = use_count + 1 WHERE id = $1::uuid")
        .bind(&pioneer_code_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Revoke a pioneer code and disable every account created through it.
pub async fn revoke_pioneer_code(
    pool: &PgPool,
    id: &PioneerCodeId,
) -> Result<RevokePioneerCodeOutcome> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"
        UPDATE pioneer_codes
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
        return Err(ServiceError::NotFound);
    }

    let agent_id_rows = sqlx::query(
        r#"
        SELECT agent_id
        FROM pioneer_code_uses
        WHERE pioneer_code_id = $1::uuid
          AND agent_id IS NOT NULL
        "#,
    )
    .bind(id.as_str())
    .fetch_all(&mut *tx)
    .await?;
    let agent_ids = agent_id_rows
        .iter()
        .map(|row| row.try_get::<Uuid, _>("agent_id"))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let human_id_rows = sqlx::query(
        r#"
        SELECT human_id
        FROM pioneer_code_uses
        WHERE pioneer_code_id = $1::uuid
          AND human_id IS NOT NULL
        "#,
    )
    .bind(id.as_str())
    .fetch_all(&mut *tx)
    .await?;
    let human_ids = human_id_rows
        .iter()
        .map(|row| row.try_get::<Uuid, _>("human_id"))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let revoked_human_count = if human_ids.is_empty() {
        0
    } else {
        let result = sqlx::query(
            r#"
            UPDATE humans
            SET status = 'disabled',
                disabled_at = COALESCE(disabled_at, NOW())
            WHERE id = ANY($1::uuid[])
              AND status = 'active'
            "#,
        )
        .bind(&human_ids)
        .execute(&mut *tx)
        .await?;
        i64::try_from(result.rows_affected())
            .map_err(|_| ServiceError::Internal("revoked human count overflow".to_string()))?
    };

    let revoked_human_session_count = if human_ids.is_empty() {
        0
    } else {
        let result = sqlx::query(
            r#"
            DELETE FROM human_sessions
            WHERE human_id = ANY($1::uuid[])
            "#,
        )
        .bind(&human_ids)
        .execute(&mut *tx)
        .await?;
        i64::try_from(result.rows_affected()).map_err(|_| {
            ServiceError::Internal("revoked human session count overflow".to_string())
        })?
    };

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
            .map_err(|_| ServiceError::Internal("revoked agent count overflow".to_string()))?
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
            .map_err(|_| ServiceError::Internal("revoked token count overflow".to_string()))?
    };

    tx.commit().await?;

    Ok(RevokePioneerCodeOutcome {
        revoked_human_count,
        revoked_human_session_count,
        revoked_agent_count,
        revoked_token_count,
    })
}

/// Convert a unavailable-code condition into the public generic error.
fn unavailable_pioneer_code() -> ServiceError {
    ServiceError::Forbidden(INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string())
}

/// Parse a pioneer-code row into the typed DB record.
fn pioneer_code_record_from_row(row: &sqlx::postgres::PgRow) -> Result<PioneerCodeRecord> {
    let id: String = row.try_get("id")?;
    Ok(PioneerCodeRecord {
        id: PioneerCodeId::try_new(id)
            .map_err(|e| ServiceError::Internal(format!("stored invalid pioneer code id: {e}")))?,
        code_display: row.try_get("code_display")?,
        label: row.try_get("label")?,
        note: row.try_get("note")?,
        max_uses: row.try_get("max_uses")?,
        use_count: row.try_get("use_count")?,
        status: row.try_get("status")?,
        expires_at: row.try_get("expires_at")?,
        created_by_display: row.try_get("created_by_display")?,
        created_at: row.try_get("created_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

/// Parse a pioneer-code use row into the typed DB record.
fn pioneer_code_use_from_row(row: &sqlx::postgres::PgRow) -> Result<PioneerCodeUseRecord> {
    let subject_kind: String = row.try_get("subject_kind")?;
    let subject_kind =
        PioneerCodeSubjectKind::from_storage_value(&subject_kind).ok_or_else(|| {
            ServiceError::Internal(format!(
                "stored invalid pioneer-code subject `{subject_kind}`"
            ))
        })?;
    let human_id = row
        .try_get::<Option<String>, _>("human_id")?
        .map(HumanId::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!("stored invalid pioneer-code human id: {e}"))
        })?;
    let agent_id = row
        .try_get::<Option<String>, _>("agent_id")?
        .map(AgentId::try_new)
        .transpose()
        .map_err(|e| {
            ServiceError::Internal(format!("stored invalid pioneer-code agent id: {e}"))
        })?;
    Ok(PioneerCodeUseRecord {
        subject_kind,
        human_id,
        human_github_login: row.try_get("human_github_login")?,
        agent_id,
        agent_display_name: row.try_get("agent_display_name")?,
        registration_kind: row.try_get("registration_kind")?,
        used_at: row.try_get("used_at")?,
    })
}
