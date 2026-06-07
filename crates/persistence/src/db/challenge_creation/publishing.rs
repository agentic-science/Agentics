use sqlx::{PgPool, Postgres, Transaction};

use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::challenge_creation::ChallengeReviewRecordStatus;
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{
    AdminServiceTokenId, ChallengeReviewAuditEventId, ChallengeReviewPublishClaimId,
    ChallengeReviewRecordId, HumanId,
};
use agentics_domain::models::localization::LocalizedText;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};

use super::super::challenges::{
    PublishChallengeInput, add_challenge_owner_tx, publish_challenge_tx,
};
use super::ChallengeReviewRecordRecord;
use super::{
    CreateChallengeReviewRecordAuditEventInput, create_challenge_review_audit_event_tx,
    get_challenge_review_record, lock_quota_scope,
};

/// Input for atomically publishing one approved new-challenge review record.
#[derive(Debug, Clone)]
pub struct PublishNewChallengeReviewRecordInput {
    pub review_record_id: ChallengeReviewRecordId,
    pub publish_claim_id: ChallengeReviewPublishClaimId,
    pub challenge_name: ChallengeName,
    pub bundle_key: StorageKey,
    pub public_bundle_key: StorageKey,
    pub statement_key: StorageKey,
    pub spec: ChallengeBundleSpec,
    pub title: String,
    pub summary: LocalizedText,
    pub owner_human_id: HumanId,
    pub audit_event_id: ChallengeReviewAuditEventId,
    pub actor_human_id: Option<HumanId>,
    pub actor_admin_service_token_id: Option<AdminServiceTokenId>,
    pub actor_display: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Input for atomically publishing one approved archive review_record.
#[derive(Debug, Clone)]
pub struct PublishArchiveChallengeReviewRecordInput {
    pub review_record_id: ChallengeReviewRecordId,
    pub publish_claim_id: ChallengeReviewPublishClaimId,
    pub challenge_name: ChallengeName,
    pub owner_human_id: HumanId,
    pub audit_event_id: ChallengeReviewAuditEventId,
    pub actor_human_id: Option<HumanId>,
    pub actor_admin_service_token_id: Option<AdminServiceTokenId>,
    pub actor_display: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Review record claimed for a single publish attempt.
#[derive(Debug, Clone)]
pub struct ClaimedChallengeReviewRecordForPublish {
    pub review_record: ChallengeReviewRecordRecord,
    pub publish_claim_id: Option<ChallengeReviewPublishClaimId>,
}

/// Claim an approved review_record for publishing before filesystem work starts.
pub async fn claim_challenge_review_record_for_publish(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
    publish_timeout_minutes: i32,
) -> Result<ClaimedChallengeReviewRecordForPublish> {
    let mut tx = pool.begin().await?;
    let scope = format!(
        "challenge-review-record:{}:publish",
        review_record_id.as_str()
    );
    lock_quota_scope(&mut tx, &scope).await?;
    reset_stale_publishing_review_record_tx(&mut tx, review_record_id, publish_timeout_minutes)
        .await?;

    let current: Option<String> = sqlx::query_scalar(
        "SELECT status FROM challenge_review_records WHERE id = $1::uuid FOR UPDATE",
    )
    .bind(review_record_id.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current) = current else {
        return Err(ServiceError::NotFound);
    };
    let current = ChallengeReviewRecordStatus::from_storage_value(&current).ok_or_else(|| {
        ServiceError::Internal(format!(
            "unknown challenge review record status `{current}`"
        ))
    })?;
    match current {
        ChallengeReviewRecordStatus::Published => {
            tx.commit().await?;
            let review_record = get_challenge_review_record(pool, review_record_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            return Ok(ClaimedChallengeReviewRecordForPublish {
                review_record,
                publish_claim_id: None,
            });
        }
        ChallengeReviewRecordStatus::Approved => {}
        _ => return Err(ServiceError::Conflict),
    }

    let publish_claim_id = ChallengeReviewPublishClaimId::generate();
    let claim = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'publishing',
            publish_claim_id = $2::uuid,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'approved'
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(publish_claim_id.as_str())
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() != 1 {
        return Err(ServiceError::Conflict);
    }
    tx.commit().await?;

    let review_record = get_challenge_review_record(pool, review_record_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok(ClaimedChallengeReviewRecordForPublish {
        review_record,
        publish_claim_id: Some(publish_claim_id),
    })
}

/// Reset a stale publishing claim back to approved so a reviewer can retry.
async fn reset_stale_publishing_review_record_tx(
    tx: &mut Transaction<'_, Postgres>,
    review_record_id: &ChallengeReviewRecordId,
    timeout_minutes: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'approved',
            publish_claim_id = NULL,
            validation_message = 'previous publish attempt expired',
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND updated_at < NOW() - INTERVAL '1 minute' * $2
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(timeout_minutes.max(1))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Release a publishing claim after filesystem or DB publication fails.
pub async fn fail_challenge_review_record_publish(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
    publish_claim_id: &ChallengeReviewPublishClaimId,
    message: &str,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'approved',
            publish_claim_id = NULL,
            validation_message = $2,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(message)
    .bind(publish_claim_id.as_str())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Mark a review_record published and bind it to the published challenge row.
pub async fn mark_challenge_review_record_published(
    pool: &PgPool,
    review_record_id: &ChallengeReviewRecordId,
    publish_claim_id: &ChallengeReviewPublishClaimId,
    published_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'published',
            published_challenge_name = $2,
            publish_claim_id = NULL,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(published_challenge_name.map(ChallengeName::as_str))
    .bind(publish_claim_id.as_str())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Publish an approved new-challenge review record as one retry-safe database unit.
pub async fn publish_new_challenge_review_record(
    pool: &PgPool,
    input: &PublishNewChallengeReviewRecordInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let published = publish_challenge_tx(
        &mut tx,
        &PublishChallengeInput {
            challenge_name: &input.challenge_name,
            bundle_key: &input.bundle_key,
            public_bundle_key: &input.public_bundle_key,
            statement_key: &input.statement_key,
            spec: &input.spec,
            title: &input.title,
            summary: &input.summary,
        },
    )
    .await?;
    add_challenge_owner_tx(&mut tx, &published.challenge_name, &input.owner_human_id).await?;
    mark_challenge_review_record_published_tx(
        &mut tx,
        &input.review_record_id,
        &input.publish_claim_id,
        Some(&published.challenge_name),
    )
    .await?;
    create_challenge_review_audit_event_tx(
        &mut tx,
        &CreateChallengeReviewRecordAuditEventInput {
            event_id: input.audit_event_id.clone(),
            review_record_id: input.review_record_id.clone(),
            actor_human_id: input.actor_human_id.clone(),
            actor_admin_service_token_id: input.actor_admin_service_token_id.clone(),
            actor_display: Some(input.actor_display.clone()),
            action: "review_record_published".to_string(),
            message: "challenge review record published".to_string(),
            metadata: serde_json::json!({
                "challenge_name": &input.challenge_name,
                "published_challenge_name": &published.challenge_name,
                "repository_path": &input.repository_path,
                "bundle_sha256": input.bundle_sha256
            }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Publish an approved archive review_record as one retry-safe database unit.
pub async fn publish_archive_challenge_review_record(
    pool: &PgPool,
    input: &PublishArchiveChallengeReviewRecordInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let challenge_name =
        resolve_active_challenge_name_by_name_tx(&mut tx, &input.challenge_name).await?;
    ensure_human_owns_challenge_tx(&mut tx, &challenge_name, &input.owner_human_id).await?;
    archive_challenge_tx(&mut tx, &challenge_name).await?;
    mark_challenge_review_record_published_tx(
        &mut tx,
        &input.review_record_id,
        &input.publish_claim_id,
        Some(&challenge_name),
    )
    .await?;
    create_challenge_review_audit_event_tx(
        &mut tx,
        &CreateChallengeReviewRecordAuditEventInput {
            event_id: input.audit_event_id.clone(),
            review_record_id: input.review_record_id.clone(),
            actor_human_id: input.actor_human_id.clone(),
            actor_admin_service_token_id: input.actor_admin_service_token_id.clone(),
            actor_display: Some(input.actor_display.clone()),
            action: "review_record_published".to_string(),
            message: "challenge review record published".to_string(),
            metadata: serde_json::json!({
                "challenge_name": &input.challenge_name,
                "published_challenge_name": &challenge_name,
                "repository_path": &input.repository_path,
                "bundle_sha256": input.bundle_sha256
            }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Resolve an active published challenge by its unique challenge name.
async fn resolve_active_challenge_name_by_name_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<ChallengeName> {
    let row = sqlx::query(
        r#"
        SELECT challenge_name
        FROM challenges
        WHERE challenge_name = $1
          AND status = 'active'
          AND spec_json IS NOT NULL
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_optional(&mut **tx)
    .await?;

    let row = row.ok_or(ServiceError::NotFound)?;
    super::super::ids::challenge_name_from_row(&row, "challenge_name")
}

/// Require that an archive review_record creator currently owns the target challenge.
async fn ensure_human_owns_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
    human_id: &HumanId,
) -> Result<()> {
    let owns_challenge = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_owners
            WHERE challenge_name = $1 AND human_id = $2::uuid
        )
        "#,
    )
    .bind(challenge_name.as_str())
    .bind(human_id.as_str())
    .fetch_one(&mut **tx)
    .await?;
    if !owns_challenge {
        return Err(ServiceError::Forbidden(
            "only a challenge owner can publish an archive review_record for this challenge"
                .to_string(),
        ));
    }

    Ok(())
}

/// Marks challenge review record published tx in persistent state.
async fn mark_challenge_review_record_published_tx(
    tx: &mut Transaction<'_, Postgres>,
    review_record_id: &ChallengeReviewRecordId,
    publish_claim_id: &ChallengeReviewPublishClaimId,
    published_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'published',
            published_challenge_name = $2,
            publish_claim_id = NULL,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(review_record_id.as_str())
    .bind(published_challenge_name.map(ChallengeName::as_str))
    .bind(publish_claim_id.as_str())
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Handles archive challenge tx for this module.
async fn archive_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE challenge_name = $1
        "#,
    )
    .bind(challenge_name.as_str())
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::NotFound);
    }
    Ok(())
}
