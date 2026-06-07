use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use agentics_domain::models::ids::{AdminServiceTokenId, HumanId};
use agentics_error::{ErrorDetail, Result, ServiceError};

use crate::db::ids::{admin_service_token_id_from_row, human_id_from_row};

const ADMIN_SERVICE_TOKEN_ACTIVE_LABEL_INDEX: &str = "idx_admin_service_tokens_owner_active_label";

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

/// Input for creating an admin service token.
#[derive(Debug, Clone)]
pub struct CreateAdminServiceTokenInput {
    pub id: AdminServiceTokenId,
    pub token_hash: String,
    pub label: String,
    pub created_by_human_id: HumanId,
    pub expires_at: Option<DateTime<Utc>>,
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
    .await
    .map_err(map_admin_service_token_create_error)?;

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
            t.id::text AS id,
            t.label,
            t.created_by_human_id::text AS created_by_human_id,
            t.expires_at
        FROM admin_service_tokens t
        JOIN humans h ON h.id = t.created_by_human_id
        JOIN human_roles r ON r.human_id = h.id
        WHERE t.token_hash = $1
          AND t.status = 'active'
          AND (t.expires_at IS NULL OR t.expires_at > NOW())
          AND h.status = 'active'
          AND r.role = 'admin'
          AND r.revoked_at IS NULL
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

fn map_admin_service_token_create_error(error: sqlx::Error) -> ServiceError {
    match error {
        sqlx::Error::Database(db_err)
            if db_err.is_unique_violation()
                && db_err
                    .constraint()
                    .is_some_and(|name| name == ADMIN_SERVICE_TOKEN_ACTIVE_LABEL_INDEX) =>
        {
            duplicate_token_label_conflict(
                "active admin service token label already exists for this admin",
                "An active admin service token from this admin already uses this label.",
            )
        }
        error => ServiceError::Database(error),
    }
}

fn duplicate_token_label_conflict(message: &str, detail_message: &str) -> ServiceError {
    ServiceError::conflict_with_details(
        message,
        [ErrorDetail {
            field: Some("label".to_string()),
            message: detail_message.to_string(),
        }],
    )
}
