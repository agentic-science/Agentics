use sqlx::{PgPool, Postgres, Transaction};

use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::challenge_creation::{ChallengeDraftResponse, ChallengeDraftStatus};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{
    AgentId, ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftPublishClaimId,
    ChallengeId,
};
use agentics_domain::models::localization::LocalizedText;
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::paths::{ManagedBundlePath, ManagedStatementPath};

use super::super::challenges::{
    PublishChallengeInput, add_challenge_owner_tx, publish_challenge_tx,
};
use super::{
    CreateChallengeDraftAuditEventInput, create_challenge_draft_audit_event_tx,
    get_challenge_draft, lock_quota_scope,
};

/// Input for atomically publishing one approved new-challenge draft.
#[derive(Debug, Clone)]
pub struct PublishNewChallengeDraftInput {
    pub draft_id: ChallengeDraftId,
    pub publish_claim_id: ChallengeDraftPublishClaimId,
    pub challenge_name: ChallengeName,
    pub bundle_path: ManagedBundlePath,
    pub public_bundle_path: ManagedBundlePath,
    pub statement_path: ManagedStatementPath,
    pub spec: ChallengeBundleSpec,
    pub title: String,
    pub summary: LocalizedText,
    pub owner_agent_id: AgentId,
    pub audit_event_id: ChallengeDraftAuditEventId,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Input for atomically publishing one approved archive draft.
#[derive(Debug, Clone)]
pub struct PublishArchiveChallengeDraftInput {
    pub draft_id: ChallengeDraftId,
    pub publish_claim_id: ChallengeDraftPublishClaimId,
    pub challenge_name: ChallengeName,
    pub owner_agent_id: AgentId,
    pub audit_event_id: ChallengeDraftAuditEventId,
    pub admin_username: String,
    pub repository_path: String,
    pub bundle_sha256: Sha256Digest,
}

/// Draft record claimed for a single publish attempt.
#[derive(Debug, Clone)]
pub struct ClaimedChallengeDraftForPublish {
    pub draft: ChallengeDraftResponse,
    pub publish_claim_id: Option<ChallengeDraftPublishClaimId>,
}

/// Claim an approved draft for publishing before filesystem work starts.
pub async fn claim_challenge_draft_for_publish(
    pool: &PgPool,
    draft_id: &str,
    publish_timeout_minutes: i32,
) -> Result<ClaimedChallengeDraftForPublish> {
    let mut tx = pool.begin().await?;
    let scope = format!("challenge-draft:{draft_id}:publish");
    lock_quota_scope(&mut tx, &scope).await?;
    reset_stale_publishing_draft_tx(&mut tx, draft_id, publish_timeout_minutes).await?;

    let current: Option<String> =
        sqlx::query_scalar("SELECT status FROM challenge_drafts WHERE id = $1::uuid FOR UPDATE")
            .bind(draft_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(current) = current else {
        return Err(ServiceError::NotFound);
    };
    let current = ChallengeDraftStatus::from_storage_value(&current).ok_or_else(|| {
        ServiceError::Internal(format!("unknown challenge draft status `{current}`"))
    })?;
    match current {
        ChallengeDraftStatus::Published => {
            tx.commit().await?;
            let draft = get_challenge_draft(pool, draft_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            return Ok(ClaimedChallengeDraftForPublish {
                draft,
                publish_claim_id: None,
            });
        }
        ChallengeDraftStatus::Approved => {}
        _ => return Err(ServiceError::Conflict),
    }

    let publish_claim_id = ChallengeDraftPublishClaimId::generate();
    let claim = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'publishing',
            publish_claim_id = $2::uuid,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'approved'
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(publish_claim_id.as_str())
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() != 1 {
        return Err(ServiceError::Conflict);
    }
    tx.commit().await?;

    let draft = get_challenge_draft(pool, draft_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok(ClaimedChallengeDraftForPublish {
        draft,
        publish_claim_id: Some(publish_claim_id),
    })
}

/// Reset a stale publishing claim back to approved so a reviewer can retry.
async fn reset_stale_publishing_draft_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    timeout_minutes: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'approved',
            publish_claim_id = NULL,
            validation_message = 'previous publish attempt expired',
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND updated_at < NOW() - INTERVAL '1 minute' * $2
        "#,
    )
    .bind(draft_id)
    .bind(timeout_minutes.max(1))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Release a publishing claim after filesystem or DB publication fails.
pub async fn fail_challenge_draft_publish(
    pool: &PgPool,
    draft_id: &str,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    message: &str,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'approved',
            publish_claim_id = NULL,
            validation_message = $2,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
        "#,
    )
    .bind(draft_id)
    .bind(message)
    .bind(publish_claim_id.as_str())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Mark a draft published and bind it to the published challenge row.
pub async fn mark_challenge_draft_published(
    pool: &PgPool,
    draft_id: &str,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    published_challenge_id: Option<&ChallengeId>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_id = $2::uuid,
            publish_claim_id = NULL,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_id.map(ChallengeId::as_str))
    .bind(publish_claim_id.as_str())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::Conflict);
    }
    Ok(())
}

/// Publish an approved new-challenge draft as one retry-safe database unit.
pub async fn publish_new_challenge_draft(
    pool: &PgPool,
    input: &PublishNewChallengeDraftInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let challenge_id = ChallengeId::generate();
    let published = publish_challenge_tx(
        &mut tx,
        &PublishChallengeInput {
            challenge_id: &challenge_id,
            challenge_name: &input.challenge_name,
            bundle_path: &input.bundle_path,
            public_bundle_path: &input.public_bundle_path,
            statement_path: &input.statement_path,
            spec: &input.spec,
            title: &input.title,
            summary: &input.summary,
        },
    )
    .await?;
    add_challenge_owner_tx(&mut tx, &published.challenge_id, &input.owner_agent_id).await?;
    mark_challenge_draft_published_tx(
        &mut tx,
        input.draft_id.as_str(),
        &input.publish_claim_id,
        Some(&published.challenge_id),
    )
    .await?;
    create_challenge_draft_audit_event_tx(
        &mut tx,
        &CreateChallengeDraftAuditEventInput {
            event_id: input.audit_event_id.clone(),
            draft_id: input.draft_id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(input.admin_username.clone()),
            action: "draft_published".to_string(),
            message: "challenge draft published".to_string(),
            metadata: serde_json::json!({
                "challenge_name": &input.challenge_name,
                "published_challenge_id": &published.challenge_id,
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

/// Publish an approved archive draft as one retry-safe database unit.
pub async fn publish_archive_challenge_draft(
    pool: &PgPool,
    input: &PublishArchiveChallengeDraftInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let challenge_id =
        resolve_active_challenge_id_by_name_tx(&mut tx, &input.challenge_name).await?;
    ensure_agent_owns_challenge_tx(&mut tx, &challenge_id, &input.owner_agent_id).await?;
    archive_challenge_tx(&mut tx, &challenge_id).await?;
    mark_challenge_draft_published_tx(
        &mut tx,
        input.draft_id.as_str(),
        &input.publish_claim_id,
        Some(&challenge_id),
    )
    .await?;
    create_challenge_draft_audit_event_tx(
        &mut tx,
        &CreateChallengeDraftAuditEventInput {
            event_id: input.audit_event_id.clone(),
            draft_id: input.draft_id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(input.admin_username.clone()),
            action: "draft_published".to_string(),
            message: "challenge draft published".to_string(),
            metadata: serde_json::json!({
                "challenge_name": &input.challenge_name,
                "published_challenge_id": &challenge_id,
                "published_challenge_name": &input.challenge_name,
                "repository_path": &input.repository_path,
                "bundle_sha256": input.bundle_sha256
            }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Resolve an active published challenge id by its unique challenge name.
async fn resolve_active_challenge_id_by_name_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_name: &ChallengeName,
) -> Result<ChallengeId> {
    let row = sqlx::query(
        r#"
        SELECT challenge_id
        FROM challenges
        WHERE name = $1
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
    super::super::ids::challenge_id_from_row(&row, "challenge_id")
}

/// Require that an archive draft creator currently owns the target challenge.
async fn ensure_agent_owns_challenge_tx(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: &ChallengeId,
    agent_id: &AgentId,
) -> Result<()> {
    let owns_challenge = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM challenge_owners
            WHERE challenge_id = $1::uuid AND agent_id = $2::uuid
        )
        "#,
    )
    .bind(challenge_id.as_str())
    .bind(agent_id.as_str())
    .fetch_one(&mut **tx)
    .await?;
    if !owns_challenge {
        return Err(ServiceError::Forbidden(
            "only a challenge owner can publish an archive draft for this challenge".to_string(),
        ));
    }

    Ok(())
}

/// Marks challenge draft published tx in persistent state.
async fn mark_challenge_draft_published_tx(
    tx: &mut Transaction<'_, Postgres>,
    draft_id: &str,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    published_challenge_id: Option<&ChallengeId>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'published',
            published_challenge_id = $2::uuid,
            publish_claim_id = NULL,
            updated_at = NOW()
        WHERE id = $1::uuid
          AND status = 'publishing'
          AND publish_claim_id = $3::uuid
          AND active_validation_record_id IS NULL
        "#,
    )
    .bind(draft_id)
    .bind(published_challenge_id.map(ChallengeId::as_str))
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
    challenge_id: &ChallengeId,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE challenges
        SET status = 'archived',
            updated_at = NOW()
        WHERE challenge_id = $1::uuid
        "#,
    )
    .bind(challenge_id.as_str())
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ServiceError::NotFound);
    }
    Ok(())
}
