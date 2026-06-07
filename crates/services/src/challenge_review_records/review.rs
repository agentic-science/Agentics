use agentics_domain::models::challenge_creation::{
    ChallengeReviewRecordResponse, ChallengeReviewRecordStatus,
};
use agentics_domain::models::ids::ChallengeReviewAuditEventId;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};

use super::presentation::review_record_response;
use super::types::ChallengeReviewDecisionServiceRequest;
use super::utils::non_empty_message;

/// Mark a review_record abandoned when the backing PR is closed without merge or the
/// creator withdraws the request.
pub async fn abandon_challenge_review_record(
    pool: &sqlx::PgPool,
    request: ChallengeReviewDecisionServiceRequest,
) -> Result<ChallengeReviewRecordResponse> {
    let ChallengeReviewDecisionServiceRequest {
        admin,
        review_record_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let audit_event = persistence::CreateChallengeReviewRecordAuditEventInput {
        event_id: ChallengeReviewAuditEventId::generate(),
        review_record_id: review_record_id.clone(),
        actor_human_id: admin.human_id.clone(),
        actor_admin_service_token_id: admin.admin_service_token_id.clone(),
        actor_display: Some(admin.display.clone()),
        action: "review_record_abandoned".to_string(),
        message: body.message.trim().to_string(),
        metadata: serde_json::json!({}),
    };
    repos
        .challenge_review_records()
        .abandon_with_audit(
            &review_record_id,
            non_empty_message(&body.message),
            &audit_event,
        )
        .await?;

    repos
        .challenge_review_records()
        .get(&review_record_id)
        .await?
        .map(review_record_response)
        .ok_or(ServiceError::NotFound)
}

/// Approve a validated review_record for publishing.
pub async fn approve_challenge_review_record(
    pool: &sqlx::PgPool,
    request: ChallengeReviewDecisionServiceRequest,
) -> Result<ChallengeReviewRecordResponse> {
    let ChallengeReviewDecisionServiceRequest {
        admin,
        review_record_id,
        body,
    } = request;
    let expected_validation_bundle_sha256 = body
        .expected_validation_bundle_sha256
        .as_ref()
        .ok_or_else(|| {
            ServiceError::BadRequest(
                "expected_validation_bundle_sha256 is required when approving a review_record"
                    .to_string(),
            )
        })?;
    let repos = Repositories::new(pool);
    repos
        .challenge_review_records()
        .approve_validated_with_audit(
            &review_record_id,
            expected_validation_bundle_sha256,
            non_empty_message(&body.message),
            &persistence::CreateChallengeReviewRecordAuditEventInput {
                event_id: ChallengeReviewAuditEventId::generate(),
                review_record_id: review_record_id.clone(),
                actor_human_id: admin.human_id.clone(),
                actor_admin_service_token_id: admin.admin_service_token_id.clone(),
                actor_display: Some(admin.display.clone()),
                action: "review_record_approved".to_string(),
                message: body.message.trim().to_string(),
                metadata: serde_json::json!({}),
            },
        )
        .await?;
    repos
        .challenge_review_records()
        .get(&review_record_id)
        .await?
        .map(review_record_response)
        .ok_or(ServiceError::NotFound)
}

/// Reject a review_record with reviewer feedback.
pub async fn reject_challenge_review_record(
    pool: &sqlx::PgPool,
    request: ChallengeReviewDecisionServiceRequest,
) -> Result<ChallengeReviewRecordResponse> {
    let ChallengeReviewDecisionServiceRequest {
        admin,
        review_record_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let review_record = repos
        .challenge_review_records()
        .get(&review_record_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if review_record.status == ChallengeReviewRecordStatus::Published {
        return Err(ServiceError::Conflict);
    }
    let audit_event = persistence::CreateChallengeReviewRecordAuditEventInput {
        event_id: ChallengeReviewAuditEventId::generate(),
        review_record_id: review_record.id.clone(),
        actor_human_id: admin.human_id.clone(),
        actor_admin_service_token_id: admin.admin_service_token_id.clone(),
        actor_display: Some(admin.display.clone()),
        action: "review_record_rejected".to_string(),
        message: body.message.trim().to_string(),
        metadata: serde_json::json!({}),
    };
    repos
        .challenge_review_records()
        .update_status_with_audit(
            &review_record.id,
            ChallengeReviewRecordStatus::Rejected,
            non_empty_message(&body.message),
            &audit_event,
        )
        .await?;
    repos
        .challenge_review_records()
        .get(&review_record.id)
        .await?
        .map(review_record_response)
        .ok_or(ServiceError::NotFound)
}
