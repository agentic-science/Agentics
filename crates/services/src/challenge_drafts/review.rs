use agentics_domain::models::challenge_creation::{ChallengeDraftResponse, ChallengeDraftStatus};
use agentics_domain::models::ids::ChallengeDraftAuditEventId;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};

use super::types::ReviewChallengeDraftServiceRequest;
use super::utils::non_empty_message;

/// Mark a draft abandoned when the backing PR is closed without merge or the
/// creator withdraws the request.
pub async fn abandon_challenge_draft(
    pool: &sqlx::PgPool,
    request: ReviewChallengeDraftServiceRequest,
) -> Result<ChallengeDraftResponse> {
    let ReviewChallengeDraftServiceRequest {
        admin,
        draft_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let audit_event = persistence::CreateChallengeDraftAuditEventInput {
        event_id: ChallengeDraftAuditEventId::generate(),
        draft_id: draft_id.clone(),
        actor_agent_id: None,
        actor_admin_username: Some(admin.username),
        action: "draft_abandoned".to_string(),
        message: body.message.trim().to_string(),
        metadata: serde_json::json!({}),
    };
    repos
        .challenge_drafts()
        .abandon_with_audit(&draft_id, non_empty_message(&body.message), &audit_event)
        .await?;

    repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)
}

/// Approve a validated draft for publishing.
pub async fn approve_challenge_draft(
    pool: &sqlx::PgPool,
    request: ReviewChallengeDraftServiceRequest,
) -> Result<ChallengeDraftResponse> {
    let ReviewChallengeDraftServiceRequest {
        admin,
        draft_id,
        body,
    } = request;
    let expected_validation_bundle_sha256 = body
        .expected_validation_bundle_sha256
        .as_ref()
        .ok_or_else(|| {
            ServiceError::BadRequest(
                "expected_validation_bundle_sha256 is required when approving a draft".to_string(),
            )
        })?;
    let repos = Repositories::new(pool);
    repos
        .challenge_drafts()
        .approve_validated_with_audit(
            &draft_id,
            expected_validation_bundle_sha256,
            non_empty_message(&body.message),
            admin.username,
            ChallengeDraftAuditEventId::generate(),
        )
        .await?;
    repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)
}

/// Reject a draft with reviewer feedback.
pub async fn reject_challenge_draft(
    pool: &sqlx::PgPool,
    request: ReviewChallengeDraftServiceRequest,
) -> Result<ChallengeDraftResponse> {
    let ReviewChallengeDraftServiceRequest {
        admin,
        draft_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let draft = repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    if draft.status == ChallengeDraftStatus::Published {
        return Err(ServiceError::Conflict);
    }
    let audit_event = persistence::CreateChallengeDraftAuditEventInput {
        event_id: ChallengeDraftAuditEventId::generate(),
        draft_id: draft.id.clone(),
        actor_agent_id: None,
        actor_admin_username: Some(admin.username),
        action: "draft_rejected".to_string(),
        message: body.message.trim().to_string(),
        metadata: serde_json::json!({}),
    };
    repos
        .challenge_drafts()
        .update_status_with_audit(
            &draft.id,
            ChallengeDraftStatus::Rejected,
            non_empty_message(&body.message),
            &audit_event,
        )
        .await?;
    repos
        .challenge_drafts()
        .get(draft.id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)
}
