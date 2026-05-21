//! HTTP handlers for GitHub-backed challenge creation drafts.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

use axum::{Json, extract::State, http::StatusCode};
use tracing::warn;
use uuid::Uuid;

use shared::error::{AppError, Result};
use shared::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeCreationManifest,
    ChallengeCreationRequestKind, ChallengeDraftCleanupResponse, ChallengeDraftListResponse,
    ChallengeDraftResponse, ChallengeDraftStatus, ChallengeDraftValidationStatus,
    ChallengePrivateAssetKind, ChallengePrivateAssetResponse, CreateChallengeDraftRequest,
    CreatorChallengeDraftResponse, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
use shared::models::hashes::{GitCommitSha, Sha256Digest};
use shared::models::ids::{
    ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftPublishClaimId,
    ChallengeDraftValidationRecordId, ChallengePrivateAssetId,
};
use shared::models::names::ChallengeName;
use shared::models::paths::{RepoRelativePath, RepositoryCheckoutPath};
use shared::storage::StorageKey;
use shared::validation::archive::{
    ArchiveEnvelopePolicy, extract_zip_bytes_to_dir, inspect_zip_bytes,
};
use shared::validation::github::GithubPullRequestRef;
use shared::{challenge_bundle, challenge_creation, db};

use crate::extractors::{AdminAuth, ChallengeDraftIdPath, CreatorAuth, ValidatedJson};
use crate::state::AppState;

const CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;
const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;

/// Create a challenge draft bound to a public GitHub PR and manifest.
pub async fn create_challenge_draft(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ValidatedJson(body): ValidatedJson<CreateChallengeDraftRequest>,
) -> Result<(StatusCode, Json<CreatorChallengeDraftResponse>)> {
    challenge_creation::validate_challenge_creation_manifest(&body.manifest)?;
    validate_challenge_draft_path(&body.challenge_path, &body.manifest.challenge_name)?;
    GithubPullRequestRef::try_new(
        body.repo_url.clone(),
        body.pr_url.clone(),
        body.pr_number.clone(),
    )?;

    if creator.github_user_id != body.pr_author_github_user_id {
        return Err(AppError::BadRequest(format!(
            "PR author GitHub user id {} does not match authenticated creator GitHub user id {}",
            body.pr_author_github_user_id, creator.github_user_id
        )));
    }
    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&body.manifest)?;
    let draft_id = ChallengeDraftId::generate();
    let repo_url = body.repo_url.clone();
    let pr_number = body.pr_number.clone();
    let commit_sha = body.commit_sha;
    let draft = db::create_challenge_draft(
        &state.db,
        &db::CreateChallengeDraftInput {
            draft_id: draft_id.clone(),
            creator_agent_id: creator.agent_id.clone(),
            max_active_drafts: i64::from(state.config.max_active_challenge_drafts_per_agent),
            creator_github_user_id: creator.github_user_id,
            creator_github_login: creator.github_login.clone(),
            repo_url: body.repo_url,
            pr_number: body.pr_number,
            pr_url: body.pr_url,
            commit_sha: body.commit_sha,
            challenge_path: body.challenge_path,
            manifest_sha256,
            manifest: body.manifest,
        },
        &db::CreateChallengeDraftAuditEventInput {
            event_id: ChallengeDraftAuditEventId::generate(),
            draft_id,
            actor_agent_id: Some(creator.agent_id.clone()),
            actor_admin_username: None,
            action: "draft_created".to_string(),
            message: "challenge draft created from GitHub PR".to_string(),
            metadata: serde_json::json!({
                "repo_url": repo_url,
                "pr_number": pr_number,
                "commit_sha": commit_sha
            }),
        },
    )
    .await
    .map_err(map_unique_conflict)?;

    Ok((StatusCode::CREATED, Json(draft.into())))
}

/// Fetch a challenge draft owned by the authenticated agent.
pub async fn get_challenge_draft(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
) -> Result<Json<CreatorChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, draft_id.as_str())
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.creator_agent_id != creator.agent_id {
        return Err(AppError::NotFound);
    }
    Ok(Json(draft.into()))
}

/// Upload a private benchmark asset for a draft owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<UploadChallengePrivateAssetRequest>,
) -> Result<(StatusCode, Json<ChallengePrivateAssetResponse>)> {
    let draft = db::get_challenge_draft(&state.db, draft_id.as_str())
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.creator_agent_id != creator.agent_id {
        return Err(AppError::NotFound);
    }
    if matches!(
        draft.status,
        ChallengeDraftStatus::Rejected
            | ChallengeDraftStatus::Approved
            | ChallengeDraftStatus::Publishing
            | ChallengeDraftStatus::Published
            | ChallengeDraftStatus::Abandoned
    ) {
        return Err(AppError::Conflict);
    }

    let requirement = draft
        .manifest
        .private_assets
        .iter()
        .find(|asset| asset.asset_name == body.asset_name)
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "private asset `{}` is not declared in the challenge manifest",
                body.asset_name
            ))
        })?;
    if requirement.kind != body.kind {
        return Err(AppError::BadRequest(format!(
            "private asset `{}` kind mismatch",
            body.asset_name
        )));
    }

    let asset_bytes = base64_decode(&body.asset_base64).ok_or(AppError::Base64)?;
    let asset_size_bytes = u64::try_from(asset_bytes.len()).map_err(|_| {
        AppError::BadRequest("private asset size exceeds supported range".to_string())
    })?;
    if asset_size_bytes > state.config.challenge_private_asset_bytes_per_draft {
        return Err(AppError::BadRequest(format!(
            "private asset must be at most {} bytes",
            state.config.challenge_private_asset_bytes_per_draft
        )));
    }
    let asset_size_bytes_i64 = i64::try_from(asset_size_bytes).map_err(|_| {
        AppError::BadRequest("private asset size exceeds supported database range".to_string())
    })?;
    validate_private_asset_zip_upload(
        &asset_bytes,
        body.asset_name.as_str(),
        state.config.challenge_private_asset_bytes_per_draft,
    )
    .await?;
    let sha256 = challenge_creation::sha256_digest(&asset_bytes);
    let storage_key = StorageKey::try_new(format!(
        "challenge-drafts/{}/private-assets/{}-{}.bin",
        draft.id, body.asset_name, sha256
    ))?;
    let temporary_asset_key = StorageKey::try_new(format!(
        "_tmp/challenge-private-assets/{}-{}-{}.bin",
        draft.id,
        body.asset_name,
        Uuid::new_v4()
    ))?;
    let asset_row_id = ChallengePrivateAssetId::generate();
    db::reserve_challenge_private_asset(
        &state.db,
        &db::CreateChallengePrivateAssetInput {
            asset_row_id: asset_row_id.clone(),
            draft_id: draft.id.clone(),
            asset_name: body.asset_name.clone(),
            kind: body.kind,
            required: requirement.required,
            size_bytes: asset_size_bytes_i64,
            sha256,
            storage_key: storage_key.clone(),
            temporary_storage_key: temporary_asset_key.clone(),
            uploader_agent_id: creator.agent_id.clone(),
        },
        state.config.challenge_private_asset_bytes_per_draft,
        state.config.challenge_draft_validation_timeout_minutes,
        state.config.challenge_private_asset_pending_timeout_minutes,
    )
    .await
    .map_err(map_unique_conflict)?;

    let temporary_storage_key = match state.storage.put(&temporary_asset_key, &asset_bytes).await {
        Ok(key) => key,
        Err(error) => {
            fail_challenge_private_asset_record(&state, &asset_row_id, &error.to_string()).await;
            cleanup_storage_key(&state, &temporary_asset_key).await;
            return Err(error);
        }
    };

    if let Err(error) = cleanup_unreferenced_private_asset_object(&state, &storage_key).await {
        fail_challenge_private_asset_record(&state, &asset_row_id, &error.to_string()).await;
        cleanup_storage_key(&state, &temporary_storage_key).await;
        return Err(error);
    }

    if let Err(error) = state
        .storage
        .promote(&temporary_storage_key, &storage_key)
        .await
    {
        fail_challenge_private_asset_record(&state, &asset_row_id, &error.to_string()).await;
        cleanup_storage_key(&state, &temporary_storage_key).await;
        return Err(error);
    }
    let asset = match db::activate_challenge_private_asset_with_audit(
        &state.db,
        &asset_row_id,
        ChallengeDraftAuditEventId::generate(),
        &creator.agent_id,
    )
    .await
    {
        Ok(asset) => asset,
        Err(error) => {
            cleanup_storage_key(&state, &storage_key).await;
            return Err(error);
        }
    };

    Ok((StatusCode::CREATED, Json(asset)))
}

/// Validate a private asset ZIP before the bytes become durable storage state.
async fn validate_private_asset_zip_upload(
    bytes: &[u8],
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        validate_private_asset_zip_upload_blocking(&bytes, &asset_name, max_uncompressed_bytes)
    })
    .await
    .map_err(|e| AppError::Internal(format!("private asset validation task failed: {e}")))?
}

/// Inspect a private asset ZIP for envelope safety without extracting it.
fn validate_private_asset_zip_upload_blocking(
    bytes: &[u8],
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let policy = ArchiveEnvelopePolicy::new(
        format!("private asset `{asset_name}`"),
        max_uncompressed_bytes,
        MAX_PRIVATE_ASSET_FILE_COUNT,
        max_uncompressed_bytes,
    );
    inspect_zip_bytes(bytes, &policy)?;
    Ok(())
}

/// Marks the pending private asset failed when storage writes cannot complete.
async fn fail_challenge_private_asset_record(
    state: &AppState,
    asset_row_id: &ChallengePrivateAssetId,
    message: &str,
) {
    if let Err(error) = db::fail_challenge_private_asset(&state.db, asset_row_id, message).await {
        warn!(
            asset_row_id = %asset_row_id,
            error = %error,
            "failed to mark private asset upload failed after storage error"
        );
    }
}

/// Remove an unreferenced durable object left by a stale pending private asset.
async fn cleanup_unreferenced_private_asset_object(
    state: &AppState,
    storage_key: &StorageKey,
) -> Result<()> {
    if !state.storage.exists(storage_key).await? {
        return Ok(());
    }
    if db::private_asset_storage_key_has_active_reference(&state.db, storage_key).await? {
        return Err(AppError::Conflict);
    }
    cleanup_storage_key(state, storage_key).await;
    Ok(())
}

/// Deletes a private-asset storage object after a failed or repaired upload path.
async fn cleanup_storage_key(state: &AppState, storage_key: &StorageKey) {
    if let Err(error) = state.storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up private asset temporary storage object"
        );
    }
}

/// List GitHub-backed challenge drafts for admin review.
pub async fn list_admin_challenge_drafts(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<ChallengeDraftListResponse>> {
    let items = db::list_challenge_drafts(&state.db, 100).await?;
    Ok(Json(ChallengeDraftListResponse { items }))
}

/// List every private asset lifecycle record for one draft for admin review.
pub async fn list_admin_challenge_draft_private_assets(
    _admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
) -> Result<Json<AdminChallengePrivateAssetListResponse>> {
    db::get_challenge_draft(&state.db, draft_id.as_str())
        .await?
        .ok_or(AppError::NotFound)?;
    let items = db::list_challenge_private_asset_states(&state.db, draft_id.as_str()).await?;
    Ok(Json(AdminChallengePrivateAssetListResponse { items }))
}

/// Validate a draft against a checked-out challenge repository path.
pub async fn validate_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, draft_id.as_str())
        .await?
        .ok_or(AppError::NotFound)?;
    if !matches!(
        draft.status,
        ChallengeDraftStatus::Draft | ChallengeDraftStatus::Validated
    ) {
        return Err(AppError::Conflict);
    }
    let validation_limit = i64::from(state.config.challenge_draft_validations_per_day);
    let repository_path = RepositoryCheckoutPath::from_existing_dir(&body.repository_path)?;
    let validation_record_id = ChallengeDraftValidationRecordId::generate();
    db::begin_challenge_draft_validation(
        &state.db,
        &db::BeginChallengeDraftValidationInput {
            validation_record_id: validation_record_id.clone(),
            draft_id: draft.id.clone(),
            repository_path: repository_path.to_string(),
            manifest_sha256: draft.manifest_sha256,
        },
        CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS,
        validation_limit,
        state.config.challenge_draft_validation_timeout_minutes,
    )
    .await?;
    let validation = validate_draft_repository(&state, &draft, &repository_path).await;

    match validation {
        Ok((_, bundle_sha256)) => {
            let message = "challenge draft validation passed".to_string();
            let audit_event = db::CreateChallengeDraftAuditEventInput {
                event_id: ChallengeDraftAuditEventId::generate(),
                draft_id: draft.id.clone(),
                actor_agent_id: None,
                actor_admin_username: Some(admin.username.clone()),
                action: "draft_validated".to_string(),
                message: message.clone(),
                metadata: serde_json::json!({
                    "repository_path": repository_path.to_string(),
                    "bundle_sha256": &bundle_sha256
                }),
            };
            db::finish_challenge_draft_validation(
                &state.db,
                &db::FinishChallengeDraftValidationInput {
                    validation_record_id,
                    draft_id: draft.id.clone(),
                    status: ChallengeDraftValidationStatus::Passed,
                    message: message.clone(),
                    bundle_sha256: Some(bundle_sha256),
                },
                &audit_event,
            )
            .await?;
            let draft = db::get_challenge_draft(&state.db, draft.id.as_str())
                .await?
                .ok_or(AppError::NotFound)?;
            Ok(Json(draft))
        }
        Err(error) => {
            let message = error.to_string();
            let audit_event = db::CreateChallengeDraftAuditEventInput {
                event_id: ChallengeDraftAuditEventId::generate(),
                draft_id: draft.id.clone(),
                actor_agent_id: None,
                actor_admin_username: Some(admin.username.clone()),
                action: "draft_validation_failed".to_string(),
                message: message.clone(),
                metadata: serde_json::json!({ "repository_path": repository_path.to_string() }),
            };
            db::finish_challenge_draft_validation(
                &state.db,
                &db::FinishChallengeDraftValidationInput {
                    validation_record_id,
                    draft_id: draft.id.clone(),
                    status: ChallengeDraftValidationStatus::Failed,
                    message: message.clone(),
                    bundle_sha256: None,
                },
                &audit_event,
            )
            .await?;
            Err(error)
        }
    }
}

/// Mark a draft abandoned when the backing PR is closed without merge or the
/// creator withdraws the request.
pub async fn abandon_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let audit_event = db::CreateChallengeDraftAuditEventInput {
        event_id: ChallengeDraftAuditEventId::generate(),
        draft_id: draft_id.clone(),
        actor_agent_id: None,
        actor_admin_username: Some(admin.username),
        action: "draft_abandoned".to_string(),
        message: body.message.trim().to_string(),
        metadata: serde_json::json!({}),
    };
    db::abandon_challenge_draft_with_audit(
        &state.db,
        &draft_id,
        non_empty_message(&body.message),
        &audit_event,
    )
    .await?;

    Ok(Json(
        db::get_challenge_draft(&state.db, draft_id.as_str())
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Expire stale drafts and purge private assets for rejected or abandoned
/// unpublished drafts after the configured grace period.
pub async fn cleanup_challenge_drafts(
    _admin: AdminAuth,
    State(state): State<AppState>,
) -> Result<Json<ChallengeDraftCleanupResponse>> {
    let abandoned =
        db::abandon_stale_challenge_drafts(&state.db, state.config.challenge_draft_ttl_days)
            .await?;
    let purge_candidates = db::list_unpublished_private_assets_for_purge(
        &state.db,
        state.config.unpublished_challenge_asset_grace_days,
    )
    .await?;

    let mut purged = 0_i64;
    for asset in purge_candidates {
        state.storage.delete(&asset.storage_key).await?;
        if let Some(temporary_storage_key) = &asset.temporary_storage_key {
            state.storage.delete(temporary_storage_key).await?;
        }
        db::delete_challenge_private_asset(&state.db, asset.id.as_str()).await?;
        purged = purged
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("private asset purge count overflow".to_string()))?;
    }

    Ok(Json(ChallengeDraftCleanupResponse {
        abandoned_drafts: abandoned,
        purged_private_assets: purged,
    }))
}

/// Approve a validated draft for publishing.
pub async fn approve_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let expected_validation_bundle_sha256 = body
        .expected_validation_bundle_sha256
        .as_ref()
        .ok_or_else(|| {
            AppError::BadRequest(
                "expected_validation_bundle_sha256 is required when approving a draft".to_string(),
            )
        })?;
    db::approve_validated_challenge_draft_with_audit(
        &state.db,
        &draft_id,
        expected_validation_bundle_sha256,
        non_empty_message(&body.message),
        admin.username,
        ChallengeDraftAuditEventId::generate(),
    )
    .await?;
    Ok(Json(
        db::get_challenge_draft(&state.db, draft_id.as_str())
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Reject a draft with reviewer feedback.
pub async fn reject_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, draft_id.as_str())
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.status == ChallengeDraftStatus::Published {
        return Err(AppError::Conflict);
    }
    let audit_event = db::CreateChallengeDraftAuditEventInput {
        event_id: ChallengeDraftAuditEventId::generate(),
        draft_id: draft.id.clone(),
        actor_agent_id: None,
        actor_admin_username: Some(admin.username),
        action: "draft_rejected".to_string(),
        message: body.message.trim().to_string(),
        metadata: serde_json::json!({}),
    };
    db::update_challenge_draft_status_with_audit(
        &state.db,
        &draft.id,
        ChallengeDraftStatus::Rejected,
        non_empty_message(&body.message),
        &audit_event,
    )
    .await?;
    Ok(Json(
        db::get_challenge_draft(&state.db, draft.id.as_str())
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Publish an approved draft into an immutable challenge contract.
pub async fn publish_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    ChallengeDraftIdPath(draft_id): ChallengeDraftIdPath,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let repository_path = RepositoryCheckoutPath::from_existing_dir(&body.repository_path)?;
    let claim = db::claim_challenge_draft_for_publish(
        &state.db,
        draft_id.as_str(),
        state.config.challenge_draft_publish_timeout_minutes,
    )
    .await?;
    let draft = claim.draft;
    if draft.status == ChallengeDraftStatus::Published {
        return Ok(Json(draft));
    }
    let publish_claim_id = claim.publish_claim_id.ok_or_else(|| {
        AppError::Internal("publishing draft claim missing publish claim id".to_string())
    })?;
    let publish_result = publish_claimed_challenge_draft(
        &state,
        &admin.username,
        &draft,
        &publish_claim_id,
        &repository_path,
    )
    .await;
    if let Err(error) = publish_result {
        db::fail_challenge_draft_publish(
            &state.db,
            draft.id.as_str(),
            &publish_claim_id,
            &error.to_string(),
        )
        .await?;
        return Err(error);
    }

    Ok(Json(
        db::get_challenge_draft(&state.db, draft.id.as_str())
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Publish a draft that has already been claimed with `publishing` status.
async fn publish_claimed_challenge_draft(
    state: &AppState,
    admin_username: &str,
    draft: &ChallengeDraftResponse,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    repository_path: &RepositoryCheckoutPath,
) -> Result<()> {
    let (manifest, bundle_sha256) =
        validate_draft_repository(state, draft, repository_path).await?;
    let approved_bundle_sha256 = draft
        .approved_bundle_sha256
        .as_ref()
        .ok_or_else(|| AppError::Conflict)?;
    if *approved_bundle_sha256 != bundle_sha256 {
        return Err(AppError::Validation(
            "challenge draft content changed after approval; validate and approve the draft again before publishing"
                .to_string(),
        ));
    }
    let proposal_root = repository_path
        .as_path()
        .join(draft.challenge_path.as_path());
    match manifest.request {
        ChallengeCreationRequestKind::ArchiveChallenge => {
            db::publish_archive_challenge_draft(
                &state.db,
                &db::PublishArchiveChallengeDraftInput {
                    draft_id: draft.id.clone(),
                    publish_claim_id: publish_claim_id.clone(),
                    challenge_name: manifest.challenge_name.clone(),
                    owner_agent_id: draft.creator_agent_id.clone(),
                    audit_event_id: ChallengeDraftAuditEventId::generate(),
                    admin_username: admin_username.to_string(),
                    repository_path: repository_path.to_string(),
                    bundle_sha256,
                },
            )
            .await?;
        }
        ChallengeCreationRequestKind::NewChallenge => {
            let final_bundle_path = runtime_bundle_path(state, draft, &manifest, publish_claim_id);
            let temporary_bundle_path =
                temporary_runtime_bundle_path(state, draft, publish_claim_id);

            let publish_new_result = prepare_and_publish_new_challenge_draft(
                state,
                PublishNewChallengeDraftContext {
                    admin_username,
                    draft,
                    publish_claim_id,
                    repository_path,
                    proposal_root: &proposal_root,
                    manifest: &manifest,
                    bundle_sha256,
                    temporary_bundle_path: &temporary_bundle_path,
                    final_bundle_path: &final_bundle_path,
                },
            )
            .await;
            if let Err(error) = publish_new_result {
                cleanup_runtime_bundle(&temporary_bundle_path).await;
                cleanup_runtime_bundle(&final_bundle_path).await;
                return Err(error);
            }
        }
    };
    Ok(())
}

/// Borrowed inputs for one publish-new-challenge attempt.
struct PublishNewChallengeDraftContext<'a> {
    admin_username: &'a str,
    draft: &'a ChallengeDraftResponse,
    publish_claim_id: &'a ChallengeDraftPublishClaimId,
    repository_path: &'a RepositoryCheckoutPath,
    proposal_root: &'a Path,
    manifest: &'a ChallengeCreationManifest,
    bundle_sha256: Sha256Digest,
    temporary_bundle_path: &'a Path,
    final_bundle_path: &'a Path,
}

/// Assemble, validate, promote, and commit a new challenge publish attempt.
async fn prepare_and_publish_new_challenge_draft(
    state: &AppState,
    ctx: PublishNewChallengeDraftContext<'_>,
) -> Result<()> {
    assemble_runtime_bundle(
        state,
        ctx.draft,
        ctx.proposal_root,
        ctx.manifest,
        ctx.temporary_bundle_path,
    )
    .await?;
    challenge_bundle::validate_challenge_bundle(ctx.temporary_bundle_path).await?;
    let spec = challenge_bundle::read_challenge_bundle_spec(ctx.temporary_bundle_path).await?;
    if state.config.requires_digest_pinned_images() {
        challenge_bundle::validate_digest_pinned_images(&spec)?;
    }
    promote_runtime_bundle(ctx.temporary_bundle_path, ctx.final_bundle_path).await?;

    let statement_path = ctx.final_bundle_path.join("statement.md");
    let managed_bundle_path =
        shared::models::paths::ManagedBundlePath::from_existing_dir(ctx.final_bundle_path)?;
    let managed_statement_path =
        shared::models::paths::ManagedStatementPath::from_existing_file(&statement_path)?;
    db::publish_new_challenge_draft(
        &state.db,
        &db::PublishNewChallengeDraftInput {
            draft_id: ctx.draft.id.clone(),
            publish_claim_id: ctx.publish_claim_id.clone(),
            challenge_name: ctx.manifest.challenge_name.clone(),
            bundle_path: managed_bundle_path,
            statement_path: managed_statement_path,
            spec,
            title: ctx.manifest.title.clone(),
            summary: ctx.manifest.summary.clone(),
            owner_agent_id: ctx.draft.creator_agent_id.clone(),
            audit_event_id: ChallengeDraftAuditEventId::generate(),
            admin_username: ctx.admin_username.to_string(),
            repository_path: ctx.repository_path.to_string(),
            bundle_sha256: ctx.bundle_sha256,
        },
    )
    .await
}

/// Maps database unique-constraint failures to the API conflict error used by draft creation.
fn map_unique_conflict(error: AppError) -> AppError {
    match error {
        AppError::Database(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            AppError::Conflict
        }
        error => error,
    }
}

/// Ensures a draft path follows the canonical `challenges/{challenge_name}` repository layout.
fn validate_challenge_draft_path(
    path: &RepoRelativePath,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let expected = format!("challenges/{challenge_name}");
    if path.as_str() != expected {
        return Err(AppError::BadRequest(format!(
            "challenge_path must be `{expected}`"
        )));
    }
    Ok(())
}

/// Validates the checked-out proposal against the manifest hash recorded at draft creation.
async fn validate_draft_repository(
    state: &AppState,
    draft: &ChallengeDraftResponse,
    repository_path: &RepositoryCheckoutPath,
) -> Result<(ChallengeCreationManifest, Sha256Digest)> {
    ensure_repository_checkout_matches_commit(repository_path, &draft.commit_sha).await?;
    let proposal_root = repository_path
        .as_path()
        .join(draft.challenge_path.as_path());
    let manifest =
        challenge_creation::validate_challenge_creation_repository(&proposal_root).await?;
    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&manifest)?;
    if manifest_sha256 != draft.manifest_sha256 {
        return Err(AppError::Validation(format!(
            "manifest hash mismatch: draft has {}, repository has {}",
            draft.manifest_sha256, manifest_sha256
        )));
    }
    if manifest.challenge_name != draft.challenge_name {
        return Err(AppError::Validation(format!(
            "manifest challenge_name mismatch: draft has {}, repository has {}",
            draft.challenge_name, manifest.challenge_name
        )));
    }
    let bundle_sha256 = match manifest.request {
        ChallengeCreationRequestKind::ArchiveChallenge => {
            challenge_creation::draft_review_bundle_sha256(
                &proposal_root,
                &manifest,
                &draft.private_assets,
            )
            .await?
        }
        ChallengeCreationRequestKind::NewChallenge => {
            validate_and_hash_runtime_bundle(state, draft, &proposal_root, &manifest).await?
        }
    };
    Ok((manifest, bundle_sha256))
}

/// Assemble private overlays, validate the runtime bundle, and return its review digest.
async fn validate_and_hash_runtime_bundle(
    state: &AppState,
    draft: &ChallengeDraftResponse,
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<Sha256Digest> {
    let validation_claim_id = ChallengeDraftPublishClaimId::generate();
    let runtime_bundle_path = temporary_runtime_bundle_path(state, draft, &validation_claim_id);
    let validation = async {
        assemble_runtime_bundle(state, draft, proposal_root, manifest, &runtime_bundle_path)
            .await?;
        challenge_bundle::validate_challenge_bundle(&runtime_bundle_path).await?;
        let spec = challenge_bundle::read_challenge_bundle_spec(&runtime_bundle_path).await?;
        if state.config.requires_digest_pinned_images() {
            challenge_bundle::validate_digest_pinned_images(&spec)?;
        }
        challenge_creation::draft_review_runtime_bundle_sha256(&runtime_bundle_path, manifest).await
    }
    .await;
    cleanup_runtime_bundle(&runtime_bundle_path).await;
    validation
}

/// Ensures validation and publication use the exact reviewed Git commit and a clean tree.
async fn ensure_repository_checkout_matches_commit(
    repository_path: &RepositoryCheckoutPath,
    expected_commit: &GitCommitSha,
) -> Result<()> {
    let repository_path = repository_path.as_path().to_path_buf();
    let expected_commit = *expected_commit;
    tokio::task::spawn_blocking(move || {
        let head = run_git(&repository_path, &["rev-parse", "--verify", "HEAD"])?;
        let head = GitCommitSha::try_new(head.trim()).map_err(|e| {
            AppError::Validation(format!("repository HEAD is not a valid Git commit: {e}"))
        })?;
        if head != expected_commit {
            return Err(AppError::Validation(format!(
                "repository HEAD commit {} does not match reviewed draft commit {}",
                head, expected_commit
            )));
        }

        let status = run_git(&repository_path, &["status", "--porcelain=v1"])?;
        if !status.trim().is_empty() {
            return Err(AppError::Validation(
                "repository checkout has uncommitted changes; validate and publish from a clean checkout at the reviewed commit"
                    .to_string(),
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("repository Git inspection task failed: {e}")))?
}

/// Run one Git command inside the reviewed repository checkout and return stdout as UTF-8.
fn run_git(repository_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository_path)
        .args(args)
        .output()
        .map_err(|e| AppError::Validation(format!("failed to inspect repository with git: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Validation(format!(
            "failed to inspect repository with git: {}",
            stderr.trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| AppError::Validation(format!("git output was not UTF-8: {e}")))
}

/// Builds the managed runtime bundle by combining public bundle files and private overlays.
async fn assemble_runtime_bundle(
    state: &AppState,
    draft: &ChallengeDraftResponse,
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    runtime_bundle_path: &Path,
) -> Result<()> {
    let bundle_path = manifest.bundle_path.as_ref().ok_or_else(|| {
        AppError::BadRequest("bundle_path is required for publishable drafts".to_string())
    })?;
    let public_bundle_path = proposal_root.join(bundle_path.as_path());
    let public_spec = challenge_bundle::read_challenge_bundle_spec(&public_bundle_path).await?;
    validate_private_assets_for_publish(draft, manifest, &public_spec)?;

    challenge_bundle::copy_challenge_bundle_dir(&public_bundle_path, runtime_bundle_path, true)
        .await?;

    for asset in &draft.private_assets {
        let bytes = state.storage.get(&asset.storage_key).await?;
        extract_private_asset_overlay(
            &bytes,
            runtime_bundle_path,
            &asset.asset_name,
            state.config.challenge_private_asset_bytes_per_draft,
        )
        .await?;
    }
    validate_private_asset_required_paths(draft, manifest, runtime_bundle_path).await?;

    Ok(())
}

/// Final managed runtime-bundle path for a published draft.
fn runtime_bundle_path(
    state: &AppState,
    draft: &ChallengeDraftResponse,
    manifest: &ChallengeCreationManifest,
    publish_claim_id: &ChallengeDraftPublishClaimId,
) -> PathBuf {
    Path::new(&state.config.storage_root)
        .join("challenge-bundles")
        .join(manifest.challenge_name.as_str())
        .join(draft.id.as_str())
        .join(publish_claim_id.as_str())
}

/// Attempt-scoped temporary runtime-bundle path under the same storage root.
fn temporary_runtime_bundle_path(
    state: &AppState,
    draft: &ChallengeDraftResponse,
    publish_claim_id: &ChallengeDraftPublishClaimId,
) -> PathBuf {
    Path::new(&state.config.storage_root)
        .join("_tmp")
        .join("challenge-bundles")
        .join(format!(
            "{}-{}-{}",
            draft.id,
            publish_claim_id,
            Uuid::new_v4()
        ))
}

/// Atomically move a validated temporary bundle into its final managed path.
async fn promote_runtime_bundle(
    temporary_bundle_path: &Path,
    final_bundle_path: &Path,
) -> Result<()> {
    if let Some(parent) = final_bundle_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::try_exists(final_bundle_path).await? {
        return Err(AppError::Conflict);
    }
    tokio::fs::rename(temporary_bundle_path, final_bundle_path).await?;
    Ok(())
}

/// Best-effort cleanup for failed runtime bundle assembly or publish.
async fn cleanup_runtime_bundle(path: &Path) {
    if let Err(error) = tokio::fs::remove_dir_all(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            path = %path.display(),
            error = %error,
            "failed to clean up challenge runtime bundle"
        );
    }
}

/// Verifies every private asset required by the manifest and bundle shape is present.
fn validate_private_assets_for_publish(
    draft: &ChallengeDraftResponse,
    manifest: &ChallengeCreationManifest,
    spec: &shared::models::challenge::ChallengeBundleSpec,
) -> Result<()> {
    let uploaded: HashSet<&str> = draft
        .private_assets
        .iter()
        .map(|asset| asset.asset_name.as_str())
        .collect();
    for requirement in &manifest.private_assets {
        if requirement.required && !uploaded.contains(requirement.asset_name.as_str()) {
            return Err(AppError::BadRequest(format!(
                "required private asset `{}` has not been uploaded",
                requirement.asset_name
            )));
        }
    }

    let uses_static_private_benchmark = spec.datasets.private_benchmark_enabled
        && match &spec.execution {
            shared::models::challenge::ChallengeExecutionSpec::SeparatedEvaluator(execution) => {
                execution.official_runs.is_some() && execution.official_prepare.is_none()
            }
            shared::models::challenge::ChallengeExecutionSpec::PipedStdio(execution) => {
                execution.official_session.is_some() && execution.official_prepare.is_none()
            }
        };
    let private_benchmark_uploaded = draft
        .private_assets
        .iter()
        .any(|asset| asset.kind == ChallengePrivateAssetKind::PrivateBenchmarkData);
    if uses_static_private_benchmark && !private_benchmark_uploaded {
        return Err(AppError::BadRequest(
            "static official benchmark challenges must upload a private_benchmark_data asset"
                .to_string(),
        ));
    }

    Ok(())
}

/// Confirms uploaded private overlays produced every manifest-declared runtime path.
async fn validate_private_asset_required_paths(
    draft: &ChallengeDraftResponse,
    manifest: &ChallengeCreationManifest,
    runtime_bundle_path: &Path,
) -> Result<()> {
    let uploaded: HashSet<&str> = draft
        .private_assets
        .iter()
        .map(|asset| asset.asset_name.as_str())
        .collect();

    for requirement in &manifest.private_assets {
        if !uploaded.contains(requirement.asset_name.as_str()) {
            continue;
        }
        for required_path in &requirement.required_paths {
            let path = runtime_bundle_path.join(required_path.as_path());
            if tokio::fs::try_exists(&path).await? {
                continue;
            }
            return Err(AppError::BadRequest(format!(
                "private asset `{}` did not provide required runtime path `{required_path}`",
                requirement.asset_name
            )));
        }
    }

    Ok(())
}

/// Extracts one private asset ZIP overlay on a blocking worker thread.
async fn extract_private_asset_overlay(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        extract_private_asset_overlay_blocking(
            &bytes,
            &target_dir,
            &asset_name,
            max_uncompressed_bytes,
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("private asset extraction task failed: {e}")))?
}

/// Expands a private asset ZIP while enforcing containment, size, and no-overwrite rules.
fn extract_private_asset_overlay_blocking(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let policy = ArchiveEnvelopePolicy::new(
        format!("private asset `{asset_name}`"),
        max_uncompressed_bytes,
        MAX_PRIVATE_ASSET_FILE_COUNT,
        max_uncompressed_bytes,
    );
    extract_zip_bytes_to_dir(bytes, target_dir, &policy)
}

/// Returns the trimmed message only when it carries non-whitespace content.
fn non_empty_message(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Decodes user-provided base64 payloads after trimming transport whitespace.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.decode(input.trim()).ok()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::extract_private_asset_overlay_blocking;

    /// Builds a small in-memory ZIP archive for private asset extraction tests.
    fn zip_with_file(path: &str, content: &[u8]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            archive
                .start_file(path, options)
                .expect("test ZIP path should start");
            archive
                .write_all(content)
                .expect("test ZIP content should write");
            archive.finish().expect("test ZIP should finish");
        }
        cursor.into_inner()
    }

    /// Rejects traversal-like private asset entries instead of silently skipping them.
    #[test]
    fn private_asset_overlay_rejects_unsafe_zip_entry_path() {
        let target = std::env::temp_dir().join(format!(
            "agentics-private-asset-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&target).expect("target tempdir");
        let escape_target = target.join("escape.txt");
        let bytes = zip_with_file("../escape.txt", b"escape");

        let error = extract_private_asset_overlay_blocking(&bytes, &target, "official-cases", 1024)
            .expect_err("unsafe ZIP path should fail extraction");

        assert!(error.to_string().contains("contains unsafe path"));
        assert!(
            !escape_target.exists(),
            "unsafe private asset extraction must not write outside the bundle"
        );
        std::fs::remove_dir_all(&target).expect("target tempdir cleanup");
    }
}
