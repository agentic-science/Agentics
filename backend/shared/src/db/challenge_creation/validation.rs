use sqlx::{PgPool, Postgres, Transaction};

use crate::error::{Result, ServiceError};
use crate::models::challenge_creation::{
    ChallengeDraftStatus, ChallengeDraftValidationRecordResponse, ChallengeDraftValidationStatus,
};
use crate::models::hashes::Sha256Digest;
use crate::models::ids::{ChallengeDraftId, ChallengeDraftValidationRecordId};

use super::rows::row_to_validation_record_response;
use super::{
    CreateChallengeDraftAuditEventInput, clear_stale_active_validation_tx,
    create_challenge_draft_audit_event_tx, lock_quota_scope,
};

/// Input for reserving one draft validation admission slot before expensive work starts.
#[derive(Debug, Clone)]
pub struct BeginChallengeDraftValidationInput {
    pub validation_record_id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub repository_path: String,
    pub manifest_sha256: Sha256Digest,
}

/// Input for completing a previously reserved draft validation record.
#[derive(Debug, Clone)]
pub struct FinishChallengeDraftValidationInput {
    pub validation_record_id: ChallengeDraftValidationRecordId,
    pub draft_id: ChallengeDraftId,
    pub status: ChallengeDraftValidationStatus,
    pub message: String,
    pub bundle_sha256: Option<Sha256Digest>,
}

/// Reserve one validation quota slot and record a running validation attempt.
pub async fn begin_challenge_draft_validation(
    pool: &PgPool,
    input: &BeginChallengeDraftValidationInput,
    window_seconds: i64,
    validation_limit: i64,
    validation_timeout_minutes: i32,
) -> Result<ChallengeDraftValidationRecordResponse> {
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-draft:{}:validations", input.draft_id);
    lock_quota_scope(&mut tx, &scope).await?;

    let status: Option<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT status, active_validation_record_id::text AS active_validation_record_id
        FROM challenge_drafts
        WHERE id = $1::uuid
        FOR UPDATE
        "#,
    )
    .bind(input.draft_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, active_validation_record_id)) = status else {
        return Err(ServiceError::NotFound);
    };
    let status = ChallengeDraftStatus::from_storage_value(&status).ok_or_else(|| {
        ServiceError::Internal(format!("unknown challenge draft status `{status}`"))
    })?;
    if !matches!(
        status,
        ChallengeDraftStatus::Draft | ChallengeDraftStatus::Validated
    ) {
        return Err(ServiceError::Conflict);
    }
    let active_validation_record_id = if active_validation_record_id.is_some() {
        clear_stale_active_validation_tx(
            &mut tx,
            input.draft_id.as_str(),
            validation_timeout_minutes,
        )
        .await?;
        let refreshed_active: Option<String> = sqlx::query_scalar(
            "SELECT active_validation_record_id::text FROM challenge_drafts WHERE id = $1::uuid",
        )
        .bind(input.draft_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        refreshed_active
    } else {
        active_validation_record_id
    };
    if active_validation_record_id.is_some() {
        return Err(ServiceError::Conflict);
    }

    let recent_validations = count_recent_challenge_draft_validations_tx(
        &mut tx,
        input.draft_id.as_str(),
        window_seconds,
    )
    .await?;
    if recent_validations >= validation_limit {
        return Err(ServiceError::TooManyRequests(format!(
            "challenge draft validation quota exceeded for `{}`: {} of {} validations used in the last 24 hours",
            input.draft_id, recent_validations, validation_limit
        )));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO challenge_draft_validation_records (
            id, draft_id, status, message, repository_path, manifest_sha256, bundle_sha256
        )
        VALUES ($1::uuid, $2::uuid, 'running', $3, $4, $5, NULL)
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.draft_id.as_str())
    .bind("challenge draft validation is running")
    .bind(&input.repository_path)
    .bind(input.manifest_sha256.to_string())
    .fetch_one(&mut *tx)
    .await?;

    let claim = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET active_validation_record_id = $2::uuid,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(input.draft_id.as_str())
    .bind(input.validation_record_id.as_str())
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() != 1 {
        return Err(ServiceError::Conflict);
    }

    tx.commit().await?;
    row_to_validation_record_response(row)
}

/// Count validation attempts for one draft inside a rolling window under a quota lock.
async fn count_recent_challenge_draft_validations_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM challenge_draft_validation_records
        WHERE draft_id = $1::uuid
          AND created_at >= NOW() - ($2::TEXT || ' seconds')::INTERVAL
        "#,
    )
    .bind(draft_id)
    .bind(window_seconds)
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Complete a reserved draft validation record and transition the draft status.
pub async fn finish_challenge_draft_validation(
    pool: &PgPool,
    input: &FinishChallengeDraftValidationInput,
    audit_event: &CreateChallengeDraftAuditEventInput,
) -> Result<ChallengeDraftValidationRecordResponse> {
    let mut tx = pool.begin().await?;
    let next_status = match input.status {
        ChallengeDraftValidationStatus::Passed => ChallengeDraftStatus::Validated,
        ChallengeDraftValidationStatus::Failed => ChallengeDraftStatus::Draft,
        ChallengeDraftValidationStatus::Running => {
            return Err(ServiceError::Internal(
                "running draft validation cannot finish as running".to_string(),
            ));
        }
    };

    let row = sqlx::query(
        r#"
        UPDATE challenge_draft_validation_records
        SET status = $3,
            message = $4,
            bundle_sha256 = $5
        WHERE id = $1::uuid
          AND draft_id = $2::uuid
          AND status = 'running'
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.draft_id.as_str())
    .bind(input.status.as_str())
    .bind(&input.message)
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(ServiceError::Conflict);
    };

    let update = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = $2,
            validation_message = $3,
            validation_repository_path = (
                SELECT repository_path
                FROM challenge_draft_validation_records
                WHERE id = $1::uuid
            ),
            validation_bundle_sha256 = $4,
            active_validation_record_id = NULL,
            updated_at = NOW()
        WHERE id = $5::uuid
          AND active_validation_record_id = $1::uuid
          AND status IN ('draft', 'validated')
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(next_status.as_str())
    .bind(&input.message)
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .bind(input.draft_id.as_str())
    .execute(&mut *tx)
    .await?;
    if update.rows_affected() == 0 {
        tx.commit().await?;
        return Err(ServiceError::Conflict);
    }

    create_challenge_draft_audit_event_tx(&mut tx, audit_event).await?;

    tx.commit().await?;
    row_to_validation_record_response(row)
}
