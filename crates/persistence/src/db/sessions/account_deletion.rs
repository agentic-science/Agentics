//! Human account deletion transaction.

use sqlx::{PgPool, Postgres, Row, Transaction};

use agentics_domain::models::auth::{HumanRole, HumanStatus};
use agentics_domain::models::ids::HumanId;
use agentics_error::{Result, ServiceError};

use super::{active_admin_count_tx, active_role_exists_tx, lock_bootstrap_admin_scope_tx};

/// Counts returned after soft-deleting one human account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteHumanAccountOutcome {
    pub revoked_human_session_count: i64,
    pub revoked_role_count: i64,
    pub revoked_admin_service_token_count: i64,
    pub revoked_creator_api_token_count: i64,
}

/// Soft-delete a human account while preserving public/provenance rows.
pub async fn delete_human_account(
    pool: &PgPool,
    human_id: &HumanId,
) -> Result<DeleteHumanAccountOutcome> {
    let mut tx = pool.begin().await?;
    lock_bootstrap_admin_scope_tx(&mut tx).await?;

    let row = sqlx::query(
        r#"
        SELECT status
        FROM humans
        WHERE id = $1::uuid
        FOR UPDATE
        "#,
    )
    .bind(human_id.as_str())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ServiceError::NotFound)?;
    let status: String = row.try_get("status")?;
    if status == HumanStatus::Deleted.as_str() {
        return Err(ServiceError::Forbidden(
            "human account has already been deleted".to_string(),
        ));
    }
    if status == HumanStatus::Disabled.as_str() {
        return Err(ServiceError::Forbidden(
            "human account is disabled".to_string(),
        ));
    }

    if active_role_exists_tx(&mut tx, human_id, HumanRole::Admin).await?
        && active_admin_count_tx(&mut tx).await? <= 1
    {
        return Err(ServiceError::BadRequest(
            "cannot delete the final active human admin".to_string(),
        ));
    }

    sqlx::query(
        r#"
        UPDATE humans
        SET status = 'deleted',
            deleted_at = COALESCE(deleted_at, NOW()),
            disabled_at = NULL
        WHERE id = $1::uuid
        "#,
    )
    .bind(human_id.as_str())
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE human_external_identities
        SET provider_login = $2,
            updated_at = NOW()
        WHERE human_id = $1::uuid
          AND provider = 'github'
        "#,
    )
    .bind(human_id.as_str())
    .bind(deleted_github_login(human_id))
    .execute(&mut *tx)
    .await?;

    let revoked_human_session_count = delete_human_sessions_tx(&mut tx, human_id.as_str()).await?;
    let revoked_role_count = revoke_human_roles_tx(&mut tx, human_id).await?;
    let revoked_admin_service_token_count =
        revoke_admin_service_tokens_created_by_human_tx(&mut tx, human_id).await?;
    let revoked_creator_api_token_count =
        revoke_creator_api_tokens_created_by_human_tx(&mut tx, human_id).await?;

    tx.commit().await?;

    Ok(DeleteHumanAccountOutcome {
        revoked_human_session_count,
        revoked_role_count,
        revoked_admin_service_token_count,
        revoked_creator_api_token_count,
    })
}

async fn revoke_human_roles_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE human_roles
        SET revoked_at = COALESCE(revoked_at, NOW()),
            revoked_by_human_id = COALESCE(revoked_by_human_id, $1::uuid)
        WHERE human_id = $1::uuid
          AND revoked_at IS NULL
        "#,
    )
    .bind(human_id.as_str())
    .execute(&mut **tx)
    .await?;
    rows_affected_i64(result.rows_affected(), "revoked human role count")
}

async fn revoke_admin_service_tokens_created_by_human_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE admin_service_tokens
        SET status = 'revoked',
            revoked_at = COALESCE(revoked_at, NOW()),
            revoked_by_human_id = COALESCE(revoked_by_human_id, $1::uuid)
        WHERE created_by_human_id = $1::uuid
          AND status = 'active'
        "#,
    )
    .bind(human_id.as_str())
    .execute(&mut **tx)
    .await?;
    rows_affected_i64(result.rows_affected(), "revoked admin service token count")
}

async fn revoke_creator_api_tokens_created_by_human_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &HumanId,
) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE creator_api_tokens
        SET status = 'revoked',
            revoked_at = COALESCE(revoked_at, NOW())
        WHERE created_by_human_id = $1::uuid
          AND status = 'active'
        "#,
    )
    .bind(human_id.as_str())
    .execute(&mut **tx)
    .await?;
    rows_affected_i64(result.rows_affected(), "revoked creator API token count")
}

fn deleted_github_login(human_id: &HumanId) -> String {
    let short_id = human_id.as_str().chars().take(8).collect::<String>();
    format!("deleted-user-{short_id}")
}

fn rows_affected_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| ServiceError::Internal(format!("{label} overflow")))
}

async fn delete_human_sessions_tx(
    tx: &mut Transaction<'_, Postgres>,
    human_id: &str,
) -> Result<i64> {
    let result = sqlx::query("DELETE FROM human_sessions WHERE human_id = $1::uuid")
        .bind(human_id)
        .execute(&mut **tx)
        .await?;
    rows_affected_i64(result.rows_affected(), "deleted human session count")
}
