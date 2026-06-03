//! Human browser session, GitHub sign-in state, and admin service-token queries.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::db::pioneer_codes::consume_pioneer_code_for_human_tx;
use agentics_domain::models::auth::HumanRole;
use agentics_domain::models::ids::{AdminServiceTokenId, HumanId, HumanSessionId};
use agentics_domain::models::pioneer_codes::INVALID_OR_UNAVAILABLE_PIONEER_CODE;
use agentics_error::{Result, ServiceError};

use super::ids::{admin_service_token_id_from_row, human_id_from_row};

/// Persisted human identity row returned to services and admin UI.
#[derive(Debug, Clone)]
pub struct HumanRecord {
    pub human_id: HumanId,
    pub status: String,
    pub github_user_id: i64,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub created_at: DateTime<Utc>,
    pub disabled_at: Option<DateTime<Utc>>,
}

/// Persisted human identity resolved from a browser session.
#[derive(Debug, Clone)]
pub struct AuthenticatedHumanSession {
    pub session_id: HumanSessionId,
    pub human_id: HumanId,
    pub github_user_id: i64,
    pub github_login: String,
    pub roles: Vec<HumanRole>,
    pub csrf_token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Persisted admin service token resolved from a bearer token.
#[derive(Debug, Clone)]
pub struct AuthenticatedAdminServiceToken {
    pub token_id: AdminServiceTokenId,
    pub label: String,
    pub created_by_human_id: HumanId,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Persisted admin service-token metadata returned to admins.
#[derive(Debug, Clone)]
pub struct AdminServiceTokenRecord {
    pub id: AdminServiceTokenId,
    pub label: String,
    pub status: String,
    pub created_by_human_id: HumanId,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_by_human_id: Option<HumanId>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Input for resolving or creating a human from a verified GitHub identity.
#[derive(Debug, Clone)]
pub struct ResolveGithubHumanInput {
    pub fallback_human_id: HumanId,
    pub github_user_id: i64,
    pub github_login: String,
    pub pioneer_code_hash: Option<String>,
    pub pioneer_code_required_for_new_human: bool,
    pub bootstrap_admin_candidate: bool,
}

/// Input for inserting a browser session.
#[derive(Debug, Clone)]
pub struct CreateHumanSessionInput {
    pub session_id: HumanSessionId,
    pub session_token_hash: String,
    pub csrf_token_hash: String,
    pub human_id: HumanId,
    pub expires_at: DateTime<Utc>,
}

/// Input for storing a short-lived GitHub sign-in state token.
#[derive(Debug, Clone)]
pub struct CreateGithubSignInStateInput {
    pub state_hash: String,
    pub browser_nonce_hash: String,
    pub pioneer_code_hash: Option<String>,
    pub return_to: Option<String>,
    pub expires_at: DateTime<Utc>,
}

/// Stored GitHub sign-in state consumed by a callback after browser redirect.
#[derive(Debug, Clone)]
pub struct ConsumedGithubSignInState {
    pub pioneer_code_hash: Option<String>,
    pub return_to: Option<String>,
}

/// Input for creating an admin service token.
#[derive(Debug, Clone)]
pub struct CreateAdminServiceTokenInput {
    pub id: AdminServiceTokenId,
    pub token_hash: String,
    pub label: String,
    pub created_by_human_id: HumanId,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Resolve an existing active human or create one from a verified GitHub identity.
pub async fn resolve_github_human(
    pool: &PgPool,
    input: &ResolveGithubHumanInput,
) -> Result<HumanRecord> {
    let mut tx = pool.begin().await?;

    if let Some(existing) = find_github_human_for_update_tx(&mut tx, input.github_user_id).await? {
        if existing.status != "active" {
            return Err(ServiceError::Forbidden(
                "linked human account is disabled".to_string(),
            ));
        }
        if input.bootstrap_admin_candidate {
            lock_bootstrap_admin_scope_tx(&mut tx).await?;
            if !active_admin_exists_tx(&mut tx).await? {
                grant_role_tx(&mut tx, &existing.human_id, HumanRole::Admin, None).await?;
            }
        }
        sqlx::query(
            r#"
            UPDATE human_external_identities
            SET provider_login = $1,
                updated_at = NOW()
            WHERE provider = 'github'
              AND provider_user_id = $2
            "#,
        )
        .bind(input.github_login.trim())
        .bind(input.github_user_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return get_human_by_id(pool, &existing.human_id).await;
    }

    let bootstrap_admin = if input.bootstrap_admin_candidate {
        lock_bootstrap_admin_scope_tx(&mut tx).await?;
        !active_admin_exists_tx(&mut tx).await?
    } else {
        false
    };

    if !bootstrap_admin
        && input.pioneer_code_required_for_new_human
        && input.pioneer_code_hash.is_none()
    {
        return Err(ServiceError::Forbidden(
            INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string(),
        ));
    }

    insert_human_tx(&mut tx, &input.fallback_human_id).await?;
    insert_github_identity_tx(
        &mut tx,
        &input.fallback_human_id,
        input.github_user_id,
        &input.github_login,
    )
    .await?;
    grant_role_tx(&mut tx, &input.fallback_human_id, HumanRole::Creator, None).await?;

    if bootstrap_admin {
        grant_role_tx(&mut tx, &input.fallback_human_id, HumanRole::Admin, None).await?;
    } else if input.pioneer_code_required_for_new_human {
        let code_hash = input.pioneer_code_hash.as_deref().ok_or_else(|| {
            ServiceError::Forbidden(INVALID_OR_UNAVAILABLE_PIONEER_CODE.to_string())
        })?;
        consume_pioneer_code_for_human_tx(&mut tx, code_hash, input.fallback_human_id.as_str())
            .await?;
    }

    tx.commit().await?;

    get_human_by_id(pool, &input.fallback_human_id).await
}

/// Store a GitHub sign-in state token hash for callback validation.
pub async fn create_github_sign_in_state(
    pool: &PgPool,
    input: &CreateGithubSignInStateInput,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO github_sign_in_states (
            state_hash,
            browser_nonce_hash,
            pioneer_code_hash,
            return_to,
            expires_at
        )
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&input.state_hash)
    .bind(&input.browser_nonce_hash)
    .bind(&input.pioneer_code_hash)
    .bind(&input.return_to)
    .bind(input.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Consume one non-expired GitHub sign-in state token.
pub async fn consume_github_sign_in_state(
    pool: &PgPool,
    state_hash: &str,
    browser_nonce_hash: &str,
) -> Result<Option<ConsumedGithubSignInState>> {
    let row = sqlx::query(
        r#"
        DELETE FROM github_sign_in_states
        WHERE state_hash = $1
          AND browser_nonce_hash = $2
          AND expires_at > NOW()
        RETURNING pioneer_code_hash, return_to
        "#,
    )
    .bind(state_hash)
    .bind(browser_nonce_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(ConsumedGithubSignInState {
        pioneer_code_hash: row.try_get("pioneer_code_hash")?,
        return_to: row.try_get("return_to")?,
    }))
}

/// Create a browser session for a verified human.
pub async fn create_human_session(pool: &PgPool, input: &CreateHumanSessionInput) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO human_sessions (
            id,
            session_token_hash,
            csrf_token_hash,
            human_id,
            expires_at
        )
        VALUES ($1::uuid, $2, $3, $4::uuid, $5)
        "#,
    )
    .bind(input.session_id.as_str())
    .bind(&input.session_token_hash)
    .bind(&input.csrf_token_hash)
    .bind(input.human_id.as_str())
    .bind(input.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Authenticate a human session token and refresh its last-used timestamp.
pub async fn authenticate_human_session(
    pool: &PgPool,
    session_token: &str,
) -> Result<Option<AuthenticatedHumanSession>> {
    let session_token_hash = crate::auth::hash_opaque_token(session_token);
    let row = sqlx::query(
        r#"
        SELECT
            s.id::text AS session_id,
            h.id::text AS human_id,
            e.provider_user_id AS github_user_id,
            e.provider_login AS github_login,
            s.csrf_token_hash,
            s.expires_at,
            COALESCE(
              array_agg(r.role ORDER BY r.role)
                FILTER (WHERE r.revoked_at IS NULL),
              ARRAY[]::TEXT[]
            ) AS roles
        FROM human_sessions s
        JOIN humans h ON h.id = s.human_id
        JOIN human_external_identities e ON e.human_id = h.id AND e.provider = 'github'
        LEFT JOIN human_roles r ON r.human_id = h.id
        WHERE s.session_token_hash = $1
          AND s.expires_at > NOW()
          AND h.status = 'active'
        GROUP BY s.id, h.id, e.provider_user_id, e.provider_login, s.csrf_token_hash, s.expires_at
        LIMIT 1
        "#,
    )
    .bind(&session_token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let session_id = HumanSessionId::try_new(row.try_get::<String, _>("session_id")?)
        .map_err(|e| ServiceError::Internal(format!("stored invalid human session id: {e}")))?;
    sqlx::query("UPDATE human_sessions SET last_used_at = NOW() WHERE id = $1::uuid")
        .bind(session_id.as_str())
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedHumanSession {
        session_id,
        human_id: human_id_from_row(&row, "human_id")?,
        github_user_id: row.try_get("github_user_id")?,
        github_login: row.try_get("github_login")?,
        roles: roles_from_row(&row)?,
        csrf_token_hash: row.try_get("csrf_token_hash")?,
        expires_at: row.try_get("expires_at")?,
    }))
}

/// Delete a browser session by the bearer cookie token.
pub async fn delete_human_session_by_token(pool: &PgPool, session_token: &str) -> Result<()> {
    let session_token_hash = crate::auth::hash_opaque_token(session_token);
    sqlx::query("DELETE FROM human_sessions WHERE session_token_hash = $1")
        .bind(session_token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// List all human accounts for admin role management.
pub async fn list_humans(pool: &PgPool) -> Result<Vec<HumanRecord>> {
    let rows = sqlx::query(human_list_sql()).fetch_all(pool).await?;
    rows.iter().map(human_record_from_row).collect()
}

/// Fetch one human by id.
pub async fn get_human_by_id(pool: &PgPool, human_id: &HumanId) -> Result<HumanRecord> {
    let row = sqlx::query(
        r#"
        SELECT
            h.id::text AS human_id,
            h.status,
            h.created_at,
            h.disabled_at,
            e.provider_user_id AS github_user_id,
            e.provider_login AS github_login,
            COALESCE(
              array_agg(r.role ORDER BY r.role)
                FILTER (WHERE r.revoked_at IS NULL),
              ARRAY[]::TEXT[]
            ) AS roles
        FROM humans h
        JOIN human_external_identities e ON e.human_id = h.id AND e.provider = 'github'
        LEFT JOIN human_roles r ON r.human_id = h.id
        WHERE h.id = $1::uuid
        GROUP BY h.id, h.status, h.created_at, h.disabled_at, e.provider_user_id, e.provider_login
        "#,
    )
    .bind(human_id.as_str())
    .fetch_optional(pool)
    .await?
    .ok_or(ServiceError::NotFound)?;
    human_record_from_row(&row)
}

/// Grant the admin role to a human account.
pub async fn grant_admin_role(
    pool: &PgPool,
    human_id: &HumanId,
    granted_by_human_id: &HumanId,
) -> Result<HumanRecord> {
    let mut tx = pool.begin().await?;
    lock_bootstrap_admin_scope_tx(&mut tx).await?;
    ensure_active_human_tx(&mut tx, human_id).await?;
    grant_role_tx(
        &mut tx,
        human_id,
        HumanRole::Admin,
        Some(granted_by_human_id),
    )
    .await?;
    tx.commit().await?;
    get_human_by_id(pool, human_id).await
}

/// Revoke the admin role from a human account.
pub async fn revoke_admin_role(
    pool: &PgPool,
    human_id: &HumanId,
    revoked_by_human_id: &HumanId,
) -> Result<HumanRecord> {
    let mut tx = pool.begin().await?;
    lock_bootstrap_admin_scope_tx(&mut tx).await?;
    let role_exists = sqlx::query(
        r#"
        SELECT id
        FROM human_roles
        WHERE human_id = $1::uuid
          AND role = 'admin'
          AND revoked_at IS NULL
        FOR UPDATE
        "#,
    )
    .bind(human_id.as_str())
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if !role_exists {
        return Err(ServiceError::NotFound);
    }
    let active_admin_count = active_admin_count_tx(&mut tx).await?;
    if active_admin_count <= 1 {
        return Err(ServiceError::BadRequest(
            "cannot revoke the final active human admin".to_string(),
        ));
    }
    sqlx::query(
        r#"
        UPDATE human_roles
        SET revoked_at = COALESCE(revoked_at, NOW()),
            revoked_by_human_id = COALESCE(revoked_by_human_id, $2::uuid)
        WHERE human_id = $1::uuid
          AND role = 'admin'
          AND revoked_at IS NULL
        "#,
    )
    .bind(human_id.as_str())
    .bind(revoked_by_human_id.as_str())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get_human_by_id(pool, human_id).await
}

/// Create an admin service token.
pub async fn create_admin_service_token(
    pool: &PgPool,
    input: &CreateAdminServiceTokenInput,
) -> Result<AdminServiceTokenRecord> {
    let row = sqlx::query(
        r#"
        INSERT INTO admin_service_tokens (
            id,
            token_hash,
            label,
            created_by_human_id,
            expires_at
        )
        VALUES ($1::uuid, $2, $3, $4::uuid, $5)
        RETURNING
            id::text AS id,
            label,
            status,
            created_by_human_id::text AS created_by_human_id,
            created_at,
            last_used_at,
            expires_at,
            revoked_by_human_id::text AS revoked_by_human_id,
            revoked_at
        "#,
    )
    .bind(input.id.as_str())
    .bind(&input.token_hash)
    .bind(input.label.trim())
    .bind(input.created_by_human_id.as_str())
    .bind(input.expires_at)
    .fetch_one(pool)
    .await?;

    admin_service_token_record_from_row(&row)
}

/// List admin service tokens.
pub async fn list_admin_service_tokens(pool: &PgPool) -> Result<Vec<AdminServiceTokenRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id::text AS id,
            label,
            status,
            created_by_human_id::text AS created_by_human_id,
            created_at,
            last_used_at,
            expires_at,
            revoked_by_human_id::text AS revoked_by_human_id,
            revoked_at
        FROM admin_service_tokens
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(admin_service_token_record_from_row)
        .collect()
}

/// Revoke an admin service token.
pub async fn revoke_admin_service_token(
    pool: &PgPool,
    id: &AdminServiceTokenId,
    revoked_by_human_id: &HumanId,
) -> Result<AdminServiceTokenRecord> {
    let row = sqlx::query(
        r#"
        UPDATE admin_service_tokens
        SET status = 'revoked',
            revoked_at = COALESCE(revoked_at, NOW()),
            revoked_by_human_id = COALESCE(revoked_by_human_id, $2::uuid)
        WHERE id = $1::uuid
        RETURNING
            id::text AS id,
            label,
            status,
            created_by_human_id::text AS created_by_human_id,
            created_at,
            last_used_at,
            expires_at,
            revoked_by_human_id::text AS revoked_by_human_id,
            revoked_at
        "#,
    )
    .bind(id.as_str())
    .bind(revoked_by_human_id.as_str())
    .fetch_optional(pool)
    .await?
    .ok_or(ServiceError::NotFound)?;

    admin_service_token_record_from_row(&row)
}

/// Authenticate an admin service token by hashed bearer token.
pub async fn authenticate_admin_service_token(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<AuthenticatedAdminServiceToken>> {
    let row = sqlx::query(
        r#"
        SELECT
            id::text AS id,
            label,
            created_by_human_id::text AS created_by_human_id,
            expires_at
        FROM admin_service_tokens
        WHERE token_hash = $1
          AND status = 'active'
          AND (expires_at IS NULL OR expires_at > NOW())
        LIMIT 1
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let token_id = admin_service_token_id_from_row(&row, "id")?;
    sqlx::query("UPDATE admin_service_tokens SET last_used_at = NOW() WHERE id = $1::uuid")
        .bind(token_id.as_str())
        .execute(pool)
        .await?;

    Ok(Some(AuthenticatedAdminServiceToken {
        token_id,
        label: row.try_get("label")?,
        created_by_human_id: human_id_from_row(&row, "created_by_human_id")?,
        expires_at: row.try_get("expires_at")?,
    }))
}

/// Delete expired transient auth rows.
pub async fn delete_expired_web_auth_rows(pool: &PgPool) -> Result<()> {
    sqlx::query("DELETE FROM github_sign_in_states WHERE expires_at <= NOW()")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM human_sessions WHERE expires_at <= NOW()")
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete all active browser sessions for one human.
pub async fn delete_human_sessions_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &str,
) -> Result<i64> {
    let result = sqlx::query("DELETE FROM human_sessions WHERE human_id = $1::uuid")
        .bind(human_id)
        .execute(&mut **tx)
        .await?;
    i64::try_from(result.rows_affected())
        .map_err(|_| ServiceError::Internal("deleted human session count overflow".to_string()))
}

async fn find_github_human_for_update_tx(
    tx: &mut Transaction<'_, Postgres>,
    github_user_id: i64,
) -> Result<Option<HumanRecord>> {
    let row = sqlx::query(
        r#"
        WITH locked_human AS (
            SELECT
                h.id,
                h.status,
                h.created_at,
                h.disabled_at,
                e.provider_user_id,
                e.provider_login
            FROM human_external_identities e
            JOIN humans h ON h.id = e.human_id
            WHERE e.provider = 'github'
              AND e.provider_user_id = $1
            FOR UPDATE OF h, e
        )
        SELECT
            locked_human.id::text AS human_id,
            locked_human.status,
            locked_human.created_at,
            locked_human.disabled_at,
            locked_human.provider_user_id AS github_user_id,
            locked_human.provider_login AS github_login,
            COALESCE(
              array_agg(r.role ORDER BY r.role)
                FILTER (WHERE r.revoked_at IS NULL),
              ARRAY[]::TEXT[]
            ) AS roles
        FROM locked_human
        LEFT JOIN human_roles r ON r.human_id = locked_human.id
        GROUP BY
            locked_human.id,
            locked_human.status,
            locked_human.created_at,
            locked_human.disabled_at,
            locked_human.provider_user_id,
            locked_human.provider_login
        "#,
    )
    .bind(github_user_id)
    .fetch_optional(&mut **tx)
    .await?;

    row.as_ref().map(human_record_from_row).transpose()
}

async fn insert_human_tx(tx: &mut Transaction<'_, Postgres>, human_id: &HumanId) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO humans (id, status)
        VALUES ($1::uuid, 'active')
        "#,
    )
    .bind(human_id.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_github_identity_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
    github_user_id: i64,
    github_login: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO human_external_identities (
            human_id,
            provider,
            provider_user_id,
            provider_login
        )
        VALUES ($1::uuid, 'github', $2, $3)
        "#,
    )
    .bind(human_id.as_str())
    .bind(github_user_id)
    .bind(github_login.trim())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn grant_role_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
    role: HumanRole,
    granted_by_human_id: Option<&HumanId>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO human_roles (id, human_id, role, granted_by_human_id)
        VALUES ($1::uuid, $2::uuid, $3, $4::uuid)
        ON CONFLICT (human_id, role) WHERE revoked_at IS NULL DO NOTHING
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(human_id.as_str())
    .bind(role.as_str())
    .bind(granted_by_human_id.map(HumanId::as_str))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn ensure_active_human_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT status
        FROM humans
        WHERE id = $1::uuid
        FOR UPDATE
        "#,
    )
    .bind(human_id.as_str())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(ServiceError::NotFound)?;
    let status: String = row.try_get("status")?;
    if status != "active" {
        return Err(ServiceError::Forbidden(
            "human account is disabled".to_string(),
        ));
    }
    Ok(())
}

async fn lock_bootstrap_admin_scope_tx(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO quota_admission_locks (scope)
        VALUES ('global:bootstrap-admin')
        ON CONFLICT (scope) DO NOTHING
        "#,
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        SELECT scope
        FROM quota_admission_locks
        WHERE scope = 'global:bootstrap-admin'
        FOR UPDATE
        "#,
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(())
}

async fn active_admin_exists_tx(tx: &mut Transaction<'_, Postgres>) -> Result<bool> {
    Ok(active_admin_count_tx(tx).await? > 0)
}

async fn active_admin_count_tx(tx: &mut Transaction<'_, Postgres>) -> Result<i64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS count
        FROM human_roles r
        JOIN humans h ON h.id = r.human_id
        WHERE r.role = 'admin'
          AND r.revoked_at IS NULL
          AND h.status = 'active'
        "#,
    )
    .fetch_one(&mut **tx)
    .await?;
    row.try_get("count").map_err(ServiceError::from)
}

fn human_list_sql() -> &'static str {
    r#"
    SELECT
        h.id::text AS human_id,
        h.status,
        h.created_at,
        h.disabled_at,
        e.provider_user_id AS github_user_id,
        e.provider_login AS github_login,
        COALESCE(
          array_agg(r.role ORDER BY r.role)
            FILTER (WHERE r.revoked_at IS NULL),
          ARRAY[]::TEXT[]
        ) AS roles
    FROM humans h
    JOIN human_external_identities e ON e.human_id = h.id AND e.provider = 'github'
    LEFT JOIN human_roles r ON r.human_id = h.id
    GROUP BY h.id, h.status, h.created_at, h.disabled_at, e.provider_user_id, e.provider_login
    ORDER BY h.created_at DESC
    "#
}

fn human_record_from_row(row: &sqlx::postgres::PgRow) -> Result<HumanRecord> {
    Ok(HumanRecord {
        human_id: human_id_from_row(row, "human_id")?,
        status: row.try_get("status")?,
        github_user_id: row.try_get("github_user_id")?,
        github_login: row.try_get("github_login")?,
        roles: roles_from_row(row)?,
        created_at: row.try_get("created_at")?,
        disabled_at: row.try_get("disabled_at")?,
    })
}

fn roles_from_row(row: &sqlx::postgres::PgRow) -> Result<Vec<HumanRole>> {
    let roles = row.try_get::<Vec<String>, _>("roles")?;
    roles
        .into_iter()
        .map(|role| {
            HumanRole::from_storage_value(&role).ok_or_else(|| {
                ServiceError::Internal(format!("stored invalid human role `{role}`"))
            })
        })
        .collect()
}

fn admin_service_token_record_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AdminServiceTokenRecord> {
    Ok(AdminServiceTokenRecord {
        id: admin_service_token_id_from_row(row, "id")?,
        label: row.try_get("label")?,
        status: row.try_get("status")?,
        created_by_human_id: human_id_from_row(row, "created_by_human_id")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
        expires_at: row.try_get("expires_at")?,
        revoked_by_human_id: row
            .try_get::<Option<String>, _>("revoked_by_human_id")?
            .map(HumanId::try_new)
            .transpose()
            .map_err(|e| {
                ServiceError::Internal(format!("stored invalid token revoker human id: {e}"))
            })?,
        revoked_at: row.try_get("revoked_at")?,
    })
}
