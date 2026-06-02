//! HTTP handlers for GitHub-backed challenge creation review_records.

use axum::{Json, extract::State, http::StatusCode};

use crate::error::ApiResult as Result;
use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengePrivateAssetResponse,
    ChallengeReviewDecisionRequest, ChallengeReviewRecordCleanupResponse,
    ChallengeReviewRecordListResponse, ChallengeReviewRecordResponse,
    CreateChallengeReviewRecordRequest, CreatorChallengeReviewRecordResponse,
    UploadChallengePrivateAssetRequest, ValidateChallengeReviewRecordRequest,
};
use agentics_services::challenge_review_records::{
    self, ChallengeReviewDecisionServiceRequest, ChallengeReviewRecordAdmin,
    ChallengeReviewRecordCreator, CreateChallengeReviewRecordServiceRequest,
    PublishChallengeReviewRecordServiceRequest, UploadChallengePrivateAssetServiceRequest,
    ValidateChallengeReviewRecordServiceRequest,
};

use crate::extractors::{AdminAuth, ChallengeReviewRecordIdPath, CreatorAuth, ValidatedJson};
use crate::state::AppState;

/// Create a challenge review record bound to a public GitHub PR and manifest.
pub async fn create_challenge_review_record(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ValidatedJson(body): ValidatedJson<CreateChallengeReviewRecordRequest>,
) -> Result<(StatusCode, Json<CreatorChallengeReviewRecordResponse>)> {
    let review_record = challenge_review_records::create_challenge_review_record(
        &state.db,
        &state.config,
        CreateChallengeReviewRecordServiceRequest {
            creator: ChallengeReviewRecordCreator {
                agent_id: creator.agent_id,
                github_user_id: creator.github_user_id,
                github_login: creator.github_login,
            },
            body,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(review_record)))
}

/// Fetch a challenge review record owned by the authenticated agent.
pub async fn get_challenge_review_record(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
) -> Result<Json<CreatorChallengeReviewRecordResponse>> {
    let review_record = challenge_review_records::get_challenge_review_record(
        &state.db,
        &creator.agent_id,
        &review_record_id,
    )
    .await?;
    Ok(Json(review_record))
}

/// Upload a private benchmark asset for a review_record owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
    ValidatedJson(body): ValidatedJson<UploadChallengePrivateAssetRequest>,
) -> Result<(StatusCode, Json<ChallengePrivateAssetResponse>)> {
    let asset = challenge_review_records::upload_challenge_private_asset(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        UploadChallengePrivateAssetServiceRequest {
            creator_agent_id: creator.agent_id,
            review_record_id,
            body,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(asset)))
}

/// List GitHub-backed challenge review records for admin review.
pub async fn list_admin_challenge_review_records(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<ChallengeReviewRecordListResponse>> {
    Ok(Json(
        challenge_review_records::list_admin_challenge_review_records(&state.db).await?,
    ))
}

/// List every private asset lifecycle record for one review_record for admin review.
pub async fn list_admin_challenge_review_record_private_assets(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
) -> Result<Json<AdminChallengePrivateAssetListResponse>> {
    Ok(Json(
        challenge_review_records::list_admin_challenge_review_record_private_assets(
            &state.db,
            &review_record_id,
        )
        .await?,
    ))
}

/// Validate a review_record against a checked-out challenge repository path.
pub async fn validate_challenge_review_record(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
    ValidatedJson(body): ValidatedJson<ValidateChallengeReviewRecordRequest>,
) -> Result<Json<ChallengeReviewRecordResponse>> {
    let review_record = challenge_review_records::validate_challenge_review_record(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        ValidateChallengeReviewRecordServiceRequest {
            admin: admin_identity(admin),
            review_record_id,
            body,
        },
    )
    .await?;
    Ok(Json(review_record))
}

/// Mark a review_record abandoned when the backing PR is closed without merge or the
/// creator withdraws the request.
pub async fn abandon_challenge_review_record(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
    ValidatedJson(body): ValidatedJson<ChallengeReviewDecisionRequest>,
) -> Result<Json<ChallengeReviewRecordResponse>> {
    let review_record = challenge_review_records::abandon_challenge_review_record(
        &state.db,
        ChallengeReviewDecisionServiceRequest {
            admin: admin_identity(admin),
            review_record_id,
            body,
        },
    )
    .await?;
    Ok(Json(review_record))
}

/// Expire stale review records and purge private assets for rejected or abandoned
/// unpublished review_records after the configured grace period.
pub async fn cleanup_challenge_review_records(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<ChallengeReviewRecordCleanupResponse>> {
    Ok(Json(
        challenge_review_records::cleanup_challenge_review_records(
            &state.db,
            state.storage.as_ref(),
            &state.config,
        )
        .await?,
    ))
}

/// Approve a validated review_record for publishing.
pub async fn approve_challenge_review_record(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
    ValidatedJson(body): ValidatedJson<ChallengeReviewDecisionRequest>,
) -> Result<Json<ChallengeReviewRecordResponse>> {
    let review_record = challenge_review_records::approve_challenge_review_record(
        &state.db,
        ChallengeReviewDecisionServiceRequest {
            admin: admin_identity(admin),
            review_record_id,
            body,
        },
    )
    .await?;
    Ok(Json(review_record))
}

/// Reject a review_record with reviewer feedback.
pub async fn reject_challenge_review_record(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
    ValidatedJson(body): ValidatedJson<ChallengeReviewDecisionRequest>,
) -> Result<Json<ChallengeReviewRecordResponse>> {
    let review_record = challenge_review_records::reject_challenge_review_record(
        &state.db,
        ChallengeReviewDecisionServiceRequest {
            admin: admin_identity(admin),
            review_record_id,
            body,
        },
    )
    .await?;
    Ok(Json(review_record))
}

/// Publish an approved review_record into an immutable challenge contract.
pub async fn publish_challenge_review_record(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeReviewRecordIdPath(review_record_id): ChallengeReviewRecordIdPath,
    ValidatedJson(body): ValidatedJson<ValidateChallengeReviewRecordRequest>,
) -> Result<Json<ChallengeReviewRecordResponse>> {
    let review_record = challenge_review_records::publish_challenge_review_record(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        PublishChallengeReviewRecordServiceRequest {
            admin: admin_identity(admin),
            review_record_id,
            body,
        },
    )
    .await?;
    Ok(Json(review_record))
}

fn admin_identity(admin: AdminAuth) -> ChallengeReviewRecordAdmin {
    ChallengeReviewRecordAdmin {
        username: admin.username,
    }
}
