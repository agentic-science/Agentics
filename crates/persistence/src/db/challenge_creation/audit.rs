use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

use agentics_domain::models::ids::{AgentId, ChallengeReviewAuditEventId, ChallengeReviewRecordId};
use agentics_error::Result;

/// Input for appending a review_record audit event.
#[derive(Debug, Clone)]
pub struct CreateChallengeReviewRecordAuditEventInput {
    pub event_id: ChallengeReviewAuditEventId,
    pub review_record_id: ChallengeReviewRecordId,
    pub actor_agent_id: Option<AgentId>,
    pub actor_admin_username: Option<String>,
    pub action: String,
    pub message: String,
    pub metadata: Value,
}

/// Append a review_record audit event.
pub async fn create_challenge_review_audit_event(
    pool: &PgPool,
    input: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    create_challenge_review_audit_event_tx(&mut tx, input).await?;
    tx.commit().await?;
    Ok(())
}

/// Creates challenge review record audit event tx after validating caller inputs.
pub(super) async fn create_challenge_review_audit_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    input: &CreateChallengeReviewRecordAuditEventInput,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO challenge_review_audit_events (
            id, review_record_id, actor_agent_id, actor_admin_username, action, message, metadata_json
        )
        VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7)
        "#,
    )
    .bind(input.event_id.as_str())
    .bind(input.review_record_id.as_str())
    .bind(input.actor_agent_id.as_ref().map(AgentId::as_str))
    .bind(&input.actor_admin_username)
    .bind(&input.action)
    .bind(&input.message)
    .bind(&input.metadata)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
