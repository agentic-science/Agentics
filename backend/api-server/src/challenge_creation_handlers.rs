//! HTTP handlers for GitHub-backed challenge creation drafts.

use axum::{Json, extract::State, http::StatusCode};

use crate::error::ApiResult as Result;
use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeDraftCleanupResponse,
    ChallengeDraftListResponse, ChallengeDraftResponse, ChallengePrivateAssetResponse,
    CreateChallengeDraftRequest, CreatorChallengeDraftResponse, ReviewChallengeDraftRequest,
    UploadChallengePrivateAssetRequest, ValidateChallengeDraftRequest,
};
use agentics_services::challenge_drafts::{
    self, ChallengeDraftAdmin, ChallengeDraftCreator, CreateChallengeDraftServiceRequest,
    PublishChallengeDraftServiceRequest, ReviewChallengeDraftServiceRequest,
    UploadChallengePrivateAssetServiceRequest, ValidateChallengeDraftServiceRequest,
};

use crate::extractors::{AdminAuth, ChallengeDraftIdPath, CreatorAuth, ValidatedJson};
use crate::state::AppState;

/// Create a challenge draft bound to a public GitHub PR and manifest.
pub async fn create_challenge_draft(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ValidatedJson(body): ValidatedJson<CreateChallengeDraftRequest>,
) -> Result<(StatusCode, Json<CreatorChallengeDraftResponse>)> {
    let draft = challenge_drafts::create_challenge_draft(
        &state.db,
        &state.config,
        CreateChallengeDraftServiceRequest {
            creator: ChallengeDraftCreator {
                agent_id: creator.agent_id,
                github_user_id: creator.github_user_id,
                github_login: creator.github_login,
            },
            body,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(draft)))
}

/// Fetch a challenge draft owned by the authenticated agent.
pub async fn get_challenge_draft(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
) -> Result<Json<CreatorChallengeDraftResponse>> {
    let draft =
        challenge_drafts::get_challenge_draft(&state.db, &creator.agent_id, &draft_id).await?;
    Ok(Json(draft))
}

/// Upload a private benchmark asset for a draft owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<UploadChallengePrivateAssetRequest>,
) -> Result<(StatusCode, Json<ChallengePrivateAssetResponse>)> {
    let asset = challenge_drafts::upload_challenge_private_asset(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        UploadChallengePrivateAssetServiceRequest {
            creator_agent_id: creator.agent_id,
            draft_id,
            body,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(asset)))
}

/// List GitHub-backed challenge drafts for admin review.
pub async fn list_admin_challenge_drafts(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<ChallengeDraftListResponse>> {
    Ok(Json(
        challenge_drafts::list_admin_challenge_drafts(&state.db).await?,
    ))
}

/// List every private asset lifecycle record for one draft for admin review.
pub async fn list_admin_challenge_draft_private_assets(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
) -> Result<Json<AdminChallengePrivateAssetListResponse>> {
    Ok(Json(
        challenge_drafts::list_admin_challenge_draft_private_assets(&state.db, &draft_id).await?,
    ))
}

/// Validate a draft against a checked-out challenge repository path.
pub async fn validate_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = challenge_drafts::validate_challenge_draft(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        ValidateChallengeDraftServiceRequest {
            admin: admin_identity(admin),
            draft_id,
            body,
        },
    )
    .await?;
    Ok(Json(draft))
}

/// Mark a draft abandoned when the backing PR is closed without merge or the
/// creator withdraws the request.
pub async fn abandon_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = challenge_drafts::abandon_challenge_draft(
        &state.db,
        ReviewChallengeDraftServiceRequest {
            admin: admin_identity(admin),
            draft_id,
            body,
        },
    )
    .await?;
    Ok(Json(draft))
}

/// Expire stale drafts and purge private assets for rejected or abandoned
/// unpublished drafts after the configured grace period.
pub async fn cleanup_challenge_drafts(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<ChallengeDraftCleanupResponse>> {
    Ok(Json(
        challenge_drafts::cleanup_challenge_drafts(
            &state.db,
            state.storage.as_ref(),
            &state.config,
        )
        .await?,
    ))
}

/// Approve a validated draft for publishing.
pub async fn approve_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = challenge_drafts::approve_challenge_draft(
        &state.db,
        ReviewChallengeDraftServiceRequest {
            admin: admin_identity(admin),
            draft_id,
            body,
        },
    )
    .await?;
    Ok(Json(draft))
}

/// Reject a draft with reviewer feedback.
pub async fn reject_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = challenge_drafts::reject_challenge_draft(
        &state.db,
        ReviewChallengeDraftServiceRequest {
            admin: admin_identity(admin),
            draft_id,
            body,
        },
    )
    .await?;
    Ok(Json(draft))
}

/// Publish an approved draft into an immutable challenge contract.
pub async fn publish_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = challenge_drafts::publish_challenge_draft(
        &state.db,
        state.storage.as_ref(),
        &state.config,
        PublishChallengeDraftServiceRequest {
            admin: admin_identity(admin),
            draft_id,
            body,
        },
    )
    .await?;
    Ok(Json(draft))
}

fn admin_identity(admin: AdminAuth) -> ChallengeDraftAdmin {
    ChallengeDraftAdmin {
        username: admin.username,
    }
}
