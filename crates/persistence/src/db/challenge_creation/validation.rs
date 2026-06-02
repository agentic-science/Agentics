use sqlx::{PgPool, Postgres, Transaction};

use agentics_domain::models::challenge_creation::{
    ChallengeReviewRecordStatus, ChallengeReviewValidationStatus,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{ChallengeReviewRecordId, ChallengeReviewValidationRecordId};
use agentics_error::{Result, ServiceError};

use super::rows::{ChallengeReviewValidationRecord, row_to_validation_record};
use super::{
    CreateChallengeReviewRecordAuditEventInput, clear_stale_active_validation_tx,
    create_challenge_review_audit_event_tx, lock_quota_scope,
};

/// Input for reserving one review_record validation admission slot before expensive work starts.
#[derive(Debug, Clone)]
pub struct BeginChallengeReviewRecordValidationInput {
    pub validation_record_id: ChallengeReviewValidationRecordId,
    pub review_record_id: ChallengeReviewRecordId,
    pub repository_path: String,
    pub manifest_sha256: Sha256Digest,
}

/// Input for completing a previously reserved review_record validation record.
#[derive(Debug, Clone)]
pub struct FinishChallengeReviewRecordValidationInput {
    pub validation_record_id: ChallengeReviewValidationRecordId,
    pub review_record_id: ChallengeReviewRecordId,
    pub status: ChallengeReviewValidationStatus,
    pub message: String,
    pub bundle_sha256: Option<Sha256Digest>,
}

/// Reserve one validation quota slot and record a running validation attempt.
pub async fn begin_challenge_review_record_validation(
    pool: &PgPool,
    input: &BeginChallengeReviewRecordValidationInput,
    window_seconds: i64,
    validation_limit: i64,
    validation_timeout_minutes: i32,
) -> Result<ChallengeReviewValidationRecord> {
    let mut tx = pool.begin().await?;
    let scope = format!(
        "challenge-review-record:{}:validations",
        input.review_record_id
    );
    lock_quota_scope(&mut tx, &scope).await?;

    let status: Option<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT status, active_validation_record_id::text AS active_validation_record_id
        FROM challenge_review_records
        WHERE id = $1::uuid
        FOR UPDATE
        "#,
    )
    .bind(input.review_record_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, active_validation_record_id)) = status else {
        return Err(ServiceError::NotFound);
    };
    let status = ChallengeReviewRecordStatus::from_storage_value(&status).ok_or_else(|| {
        ServiceError::Internal(format!("unknown challenge review record status `{status}`"))
    })?;
    if !matches!(
        status,
        ChallengeReviewRecordStatus::PendingReview | ChallengeReviewRecordStatus::Validated
    ) {
        return Err(ServiceError::Conflict);
    }
    let active_validation_record_id = if active_validation_record_id.is_some() {
        clear_stale_active_validation_tx(
            &mut tx,
            input.review_record_id.as_str(),
            validation_timeout_minutes,
        )
        .await?;
        let refreshed_active: Option<String> = sqlx::query_scalar(
            "SELECT active_validation_record_id::text FROM challenge_review_records WHERE id = $1::uuid",
        )
        .bind(input.review_record_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        refreshed_active
    } else {
        active_validation_record_id
    };
    if active_validation_record_id.is_some() {
        return Err(ServiceError::Conflict);
    }

    let recent_validations = count_recent_challenge_review_record_validations_tx(
        &mut tx,
        input.review_record_id.as_str(),
        window_seconds,
    )
    .await?;
    if recent_validations >= validation_limit {
        return Err(ServiceError::TooManyRequests(format!(
            "challenge review record validation quota exceeded for `{}`: {} of {} validations used in the last 24 hours",
            input.review_record_id, recent_validations, validation_limit
        )));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO challenge_review_validation_records (
            id, review_record_id, status, message, repository_path, manifest_sha256, bundle_sha256
        )
        VALUES ($1::uuid, $2::uuid, 'running', $3, $4, $5, NULL)
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.review_record_id.as_str())
    .bind("challenge review record validation is running")
    .bind(&input.repository_path)
    .bind(input.manifest_sha256.to_string())
    .fetch_one(&mut *tx)
    .await?;

    let claim = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET active_validation_record_id = $2::uuid,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(input.review_record_id.as_str())
    .bind(input.validation_record_id.as_str())
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() != 1 {
        return Err(ServiceError::Conflict);
    }

    tx.commit().await?;
    row_to_validation_record(row)
}

/// Count validation attempts for one review_record inside a rolling window under a quota lock.
async fn count_recent_challenge_review_record_validations_tx(
    tx: &mut Transaction<'_, Postgres>,
    review_record_id: &str,
    window_seconds: i64,
) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM challenge_review_validation_records
        WHERE review_record_id = $1::uuid
          AND created_at >= NOW() - ($2::TEXT || ' seconds')::INTERVAL
        "#,
    )
    .bind(review_record_id)
    .bind(window_seconds)
    .fetch_one(&mut **tx)
    .await?;

    Ok(count)
}

/// Complete a reserved review_record validation record and transition the review_record status.
pub async fn finish_challenge_review_record_validation(
    pool: &PgPool,
    input: &FinishChallengeReviewRecordValidationInput,
    audit_event: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<ChallengeReviewValidationRecord> {
    let mut tx = pool.begin().await?;
    let next_status = match input.status {
        ChallengeReviewValidationStatus::Passed => ChallengeReviewRecordStatus::Validated,
        ChallengeReviewValidationStatus::Failed => ChallengeReviewRecordStatus::PendingReview,
        ChallengeReviewValidationStatus::Running => {
            return Err(ServiceError::Internal(
                "running review record validation cannot finish as running".to_string(),
            ));
        }
    };

    let row = sqlx::query(
        r#"
        UPDATE challenge_review_validation_records
        SET status = $3,
            message = $4,
            bundle_sha256 = $5
        WHERE id = $1::uuid
          AND review_record_id = $2::uuid
          AND status = 'running'
        RETURNING *
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(input.review_record_id.as_str())
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
        UPDATE challenge_review_records
        SET status = $2,
            validation_message = $3,
            validation_repository_path = (
                SELECT repository_path
                FROM challenge_review_validation_records
                WHERE id = $1::uuid
            ),
            validation_bundle_sha256 = $4,
            active_validation_record_id = NULL,
            updated_at = NOW()
        WHERE id = $5::uuid
          AND active_validation_record_id = $1::uuid
          AND status IN ('pending_review', 'validated')
        "#,
    )
    .bind(input.validation_record_id.as_str())
    .bind(next_status.as_str())
    .bind(&input.message)
    .bind(input.bundle_sha256.map(|digest| digest.to_string()))
    .bind(input.review_record_id.as_str())
    .execute(&mut *tx)
    .await?;
    if update.rows_affected() == 0 {
        tx.commit().await?;
        return Err(ServiceError::Conflict);
    }

    create_challenge_review_audit_event_tx(&mut tx, audit_event).await?;

    tx.commit().await?;
    row_to_validation_record(row)
}
