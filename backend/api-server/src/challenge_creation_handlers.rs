//! HTTP handlers for GitHub-backed challenge creation drafts.

use std::{
    collections::HashSet,
    io::Cursor,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::StatusCode,
};
use tracing::warn;
use uuid::Uuid;

use shared::error::{AppError, Result};
use shared::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeCreationRequestKind, ChallengeDraftCleanupResponse,
    ChallengeDraftListResponse, ChallengeDraftResponse, ChallengeDraftStatus,
    ChallengeDraftValidationStatus, ChallengePrivateAssetKind, ChallengePrivateAssetResponse,
    CreateChallengeDraftRequest, ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest,
    ValidateChallengeDraftRequest,
};
use shared::{challenge_bundle, challenge_creation, db};

use crate::extractors::{AdminAuth, CreatorAuth, ValidatedJson};
use crate::state::AppState;

const CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;
const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;

/// Create a challenge draft bound to a public GitHub PR and manifest.
pub async fn create_challenge_draft(
    State(state): State<AppState>,
    creator: CreatorAuth,
    ValidatedJson(body): ValidatedJson<CreateChallengeDraftRequest>,
) -> Result<(StatusCode, Json<ChallengeDraftResponse>)> {
    validate_github_pr_metadata(&body)?;
    challenge_creation::validate_challenge_creation_manifest(&body.manifest)?;
    validate_challenge_draft_path(&body.challenge_path, &body.manifest.challenge_id)?;

    if creator.github_user_id != body.pr_author_github_user_id {
        return Err(AppError::BadRequest(format!(
            "PR author GitHub user id {} does not match authenticated creator GitHub user id {}",
            body.pr_author_github_user_id, creator.github_user_id
        )));
    }
    let active_drafts =
        db::count_active_challenge_drafts_for_agent(&state.db, &creator.agent_id).await?;
    let max_active_drafts = i64::from(state.config.max_active_challenge_drafts_per_agent);
    if active_drafts >= max_active_drafts {
        return Err(AppError::TooManyRequests(format!(
            "challenge draft quota exceeded: {active_drafts} of {max_active_drafts} active drafts are already open"
        )));
    }

    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&body.manifest)?;
    let draft = db::create_challenge_draft(
        &state.db,
        &db::CreateChallengeDraftInput {
            draft_id: Uuid::new_v4().to_string(),
            creator_agent_id: creator.agent_id.clone(),
            creator_github_user_id: creator.github_user_id,
            creator_github_login: creator.github_login.clone(),
            repo_url: body.repo_url.trim().to_string(),
            pr_number: body.pr_number,
            pr_url: body.pr_url.trim().to_string(),
            commit_sha: body.commit_sha.trim().to_string(),
            challenge_path: body.challenge_path.trim().to_string(),
            manifest_sha256,
            manifest: body.manifest,
        },
    )
    .await
    .map_err(map_unique_conflict)?;

    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            actor_agent_id: Some(creator.agent_id.clone()),
            actor_admin_username: None,
            action: "draft_created".to_string(),
            message: "challenge draft created from GitHub PR".to_string(),
            metadata: serde_json::json!({
                "repo_url": &draft.repo_url,
                "pr_number": draft.pr_number,
                "commit_sha": &draft.commit_sha
            }),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(draft)))
}

/// Fetch a challenge draft owned by the authenticated agent.
pub async fn get_challenge_draft(
    State(state): State<AppState>,
    creator: CreatorAuth,
    AxumPath(draft_id): AxumPath<String>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.creator_agent_id != creator.agent_id {
        return Err(AppError::NotFound);
    }
    Ok(Json(draft))
}

/// Upload a private benchmark asset for a draft owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    State(state): State<AppState>,
    creator: CreatorAuth,
    AxumPath(draft_id): AxumPath<String>,
    ValidatedJson(body): ValidatedJson<UploadChallengePrivateAssetRequest>,
) -> Result<(StatusCode, Json<ChallengePrivateAssetResponse>)> {
    validate_private_asset_id(&body.asset_id)?;

    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.creator_agent_id != creator.agent_id {
        return Err(AppError::NotFound);
    }
    if matches!(
        draft.status,
        ChallengeDraftStatus::Rejected
            | ChallengeDraftStatus::Approved
            | ChallengeDraftStatus::Published
            | ChallengeDraftStatus::Abandoned
    ) {
        return Err(AppError::Conflict);
    }

    let requirement = draft
        .manifest
        .private_assets
        .iter()
        .find(|asset| asset.asset_id == body.asset_id)
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "private asset `{}` is not declared in the challenge manifest",
                body.asset_id
            ))
        })?;
    if requirement.kind != body.kind {
        return Err(AppError::BadRequest(format!(
            "private asset `{}` kind mismatch",
            body.asset_id
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
    let sha256 = challenge_creation::sha256_hex(&asset_bytes);
    let storage_path = format!(
        "challenge-drafts/{}/private-assets/{}-{}.bin",
        draft.id, body.asset_id, sha256
    );
    let temporary_storage_path = format!(
        "_tmp/challenge-private-assets/{}-{}-{}.bin",
        draft.id,
        body.asset_id,
        Uuid::new_v4()
    );
    let temporary_storage_uri = state
        .storage
        .put(&temporary_storage_path, &asset_bytes)
        .await?;
    let asset = db::create_challenge_private_asset(
        &state.db,
        &db::CreateChallengePrivateAssetInput {
            asset_id_row: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            asset_id: body.asset_id,
            kind: body.kind,
            required: requirement.required,
            size_bytes: asset_size_bytes_i64,
            sha256,
            storage_uri: storage_path.clone(),
            uploader_agent_id: creator.agent_id.clone(),
        },
        state.config.challenge_private_asset_bytes_per_draft,
    )
    .await;

    let asset = match asset {
        Ok(asset) => asset,
        Err(error) => {
            cleanup_storage_key(&state, &temporary_storage_uri).await;
            return Err(map_unique_conflict(error));
        }
    };

    if let Err(error) = state
        .storage
        .promote(&temporary_storage_uri, &storage_path)
        .await
    {
        cleanup_challenge_private_asset_record(&state, &asset.id).await;
        cleanup_storage_key(&state, &temporary_storage_uri).await;
        return Err(error);
    }

    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            actor_agent_id: Some(creator.agent_id.clone()),
            actor_admin_username: None,
            action: "private_asset_uploaded".to_string(),
            message: "private benchmark asset uploaded".to_string(),
            metadata: serde_json::json!({
                "asset_id": &asset.asset_id,
                "kind": asset.kind,
                "size_bytes": asset.size_bytes,
                "sha256": &asset.sha256
            }),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(asset)))
}

async fn cleanup_challenge_private_asset_record(state: &AppState, asset_row_id: &str) {
    if let Err(error) = db::delete_challenge_private_asset(&state.db, asset_row_id).await {
        warn!(
            asset_row_id,
            error = %error,
            "failed to clean up private asset record after storage promotion failure"
        );
    }
}

async fn cleanup_storage_key(state: &AppState, storage_key: &str) {
    if let Err(error) = state.storage.delete(storage_key).await {
        warn!(
            storage_key,
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

/// Validate a draft against a checked-out challenge repository path.
pub async fn validate_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    AxumPath(draft_id): AxumPath<String>,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !matches!(
        draft.status,
        ChallengeDraftStatus::Draft | ChallengeDraftStatus::Validated
    ) {
        return Err(AppError::Conflict);
    }
    let recent_validations = db::count_recent_challenge_draft_validations(
        &state.db,
        &draft.id,
        CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS,
    )
    .await?;
    let validation_limit = i64::from(state.config.challenge_draft_validations_per_day);
    if recent_validations >= validation_limit {
        return Err(AppError::TooManyRequests(format!(
            "challenge draft validation quota exceeded for `{}`: {} of {} validations used in the last 24 hours",
            draft.id, recent_validations, validation_limit
        )));
    }
    let repository_path = body.repository_path.trim();
    let validation = validate_draft_repository(&draft, repository_path).await;

    match validation {
        Ok((_, bundle_sha256)) => {
            let message = "challenge draft validation passed".to_string();
            db::record_challenge_draft_validation(
                &state.db,
                &db::RecordChallengeDraftValidationInput {
                    validation_record_id: Uuid::new_v4().to_string(),
                    draft_id: draft.id.clone(),
                    status: ChallengeDraftValidationStatus::Passed,
                    message: message.clone(),
                    repository_path: repository_path.to_string(),
                    manifest_sha256: draft.manifest_sha256.clone(),
                    bundle_sha256: Some(bundle_sha256.clone()),
                },
            )
            .await?;
            db::create_challenge_draft_audit_event(
                &state.db,
                &db::CreateChallengeDraftAuditEventInput {
                    event_id: Uuid::new_v4().to_string(),
                    draft_id: draft.id.clone(),
                    actor_agent_id: None,
                    actor_admin_username: Some(admin.username.clone()),
                    action: "draft_validated".to_string(),
                    message: message.clone(),
                    metadata: serde_json::json!({
                        "repository_path": repository_path,
                        "bundle_sha256": &bundle_sha256
                    }),
                },
            )
            .await?;
            let draft = db::get_challenge_draft(&state.db, &draft.id)
                .await?
                .ok_or(AppError::NotFound)?;
            Ok(Json(draft))
        }
        Err(error) => {
            let message = error.to_string();
            db::record_challenge_draft_validation(
                &state.db,
                &db::RecordChallengeDraftValidationInput {
                    validation_record_id: Uuid::new_v4().to_string(),
                    draft_id: draft.id.clone(),
                    status: ChallengeDraftValidationStatus::Failed,
                    message: message.clone(),
                    repository_path: repository_path.to_string(),
                    manifest_sha256: draft.manifest_sha256.clone(),
                    bundle_sha256: None,
                },
            )
            .await?;
            db::create_challenge_draft_audit_event(
                &state.db,
                &db::CreateChallengeDraftAuditEventInput {
                    event_id: Uuid::new_v4().to_string(),
                    draft_id: draft.id.clone(),
                    actor_agent_id: None,
                    actor_admin_username: Some(admin.username.clone()),
                    action: "draft_validation_failed".to_string(),
                    message,
                    metadata: serde_json::json!({ "repository_path": repository_path }),
                },
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
    AxumPath(draft_id): AxumPath<String>,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    db::abandon_challenge_draft(&state.db, &draft_id, non_empty_message(&body.message)).await?;
    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft_id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(admin.username),
            action: "draft_abandoned".to_string(),
            message: body.message.trim().to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await?;

    Ok(Json(
        db::get_challenge_draft(&state.db, &draft_id)
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
        state.storage.delete(&asset.storage_uri).await?;
        db::delete_challenge_private_asset(&state.db, &asset.id).await?;
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
    AxumPath(draft_id): AxumPath<String>,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.status != ChallengeDraftStatus::Validated {
        return Err(AppError::Conflict);
    }
    db::approve_validated_challenge_draft(&state.db, &draft.id, non_empty_message(&body.message))
        .await?;
    let approved_bundle_sha256 = draft.validation_bundle_sha256.clone();
    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(admin.username),
            action: "draft_approved".to_string(),
            message: body.message.trim().to_string(),
            metadata: serde_json::json!({ "approved_bundle_sha256": approved_bundle_sha256 }),
        },
    )
    .await?;
    Ok(Json(
        db::get_challenge_draft(&state.db, &draft.id)
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Reject a draft with reviewer feedback.
pub async fn reject_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    AxumPath(draft_id): AxumPath<String>,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.status == ChallengeDraftStatus::Published {
        return Err(AppError::Conflict);
    }
    db::update_challenge_draft_status(
        &state.db,
        &draft.id,
        ChallengeDraftStatus::Rejected,
        non_empty_message(&body.message),
    )
    .await?;
    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(admin.username),
            action: "draft_rejected".to_string(),
            message: body.message.trim().to_string(),
            metadata: serde_json::json!({}),
        },
    )
    .await?;
    Ok(Json(
        db::get_challenge_draft(&state.db, &draft.id)
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

/// Publish an approved draft into an immutable challenge contract.
pub async fn publish_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    AxumPath(draft_id): AxumPath<String>,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.status == ChallengeDraftStatus::Published {
        return Ok(Json(draft));
    }
    if draft.status != ChallengeDraftStatus::Approved {
        return Err(AppError::Conflict);
    }

    let repository_path = body.repository_path.trim();
    let (manifest, bundle_sha256) = validate_draft_repository(&draft, repository_path).await?;
    let approved_bundle_sha256 = draft
        .approved_bundle_sha256
        .as_deref()
        .ok_or_else(|| AppError::Conflict)?;
    if approved_bundle_sha256 != bundle_sha256 {
        return Err(AppError::Validation(
            "challenge draft content changed after approval; validate and approve the draft again before publishing"
                .to_string(),
        ));
    }
    let proposal_root = Path::new(repository_path).join(&draft.challenge_path);
    let published_challenge_id = match manifest.request {
        ChallengeCreationRequestKind::ArchiveChallenge => {
            db::archive_challenge(&state.db, &manifest.challenge_id).await?;
            None
        }
        ChallengeCreationRequestKind::NewChallenge => {
            let bundle_path =
                assemble_runtime_bundle(&state, &draft, &proposal_root, &manifest).await?;
            challenge_bundle::validate_challenge_bundle(&bundle_path).await?;
            let spec = challenge_bundle::read_challenge_bundle_spec(&bundle_path).await?;
            if state.config.require_digest_pinned_images {
                challenge_bundle::validate_digest_pinned_images(&spec)?;
            }
            db::create_or_update_challenge(
                &state.db,
                &manifest.challenge_id,
                &manifest.challenge_id,
                &manifest.title,
                &manifest.summary,
            )
            .await?;
            let statement_path = bundle_path.join("statement.md");
            let published = db::publish_challenge(
                &state.db,
                &manifest.challenge_id,
                &bundle_path.to_string_lossy(),
                &statement_path.to_string_lossy(),
                &spec,
                &manifest.title,
                &manifest.summary,
            )
            .await?;
            Some(published.challenge_id)
        }
    };
    db::mark_challenge_draft_published(&state.db, &draft.id, published_challenge_id.as_deref())
        .await?;
    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            actor_agent_id: None,
            actor_admin_username: Some(admin.username),
            action: "draft_published".to_string(),
            message: "challenge draft published".to_string(),
            metadata: serde_json::json!({
                "challenge_id": &manifest.challenge_id,
                "published_challenge_id": &published_challenge_id,
                "repository_path": repository_path,
                "bundle_sha256": &bundle_sha256
            }),
        },
    )
    .await?;

    Ok(Json(
        db::get_challenge_draft(&state.db, &draft.id)
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}

fn map_unique_conflict(error: AppError) -> AppError {
    match error {
        AppError::Database(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            AppError::Conflict
        }
        error => error,
    }
}

fn validate_github_pr_metadata(body: &CreateChallengeDraftRequest) -> Result<()> {
    validate_urlish(&body.repo_url, "repo_url")?;
    validate_urlish(&body.pr_url, "pr_url")?;
    validate_commit_sha(&body.commit_sha)?;
    Ok(())
}

fn validate_urlish(value: &str, field: &str) -> Result<()> {
    let value = value.trim();
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::BadRequest(format!(
            "{field} must not contain whitespace or control characters"
        )));
    }
    if !(value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("git@github.com:"))
    {
        return Err(AppError::BadRequest(format!(
            "{field} must be an HTTP(S) URL or GitHub SSH URL"
        )));
    }
    Ok(())
}

fn validate_commit_sha(value: &str) -> Result<()> {
    let value = value.trim();
    if !(7..=64).contains(&value.len()) || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest(
            "commit_sha must be a 7-64 character hexadecimal Git commit id".to_string(),
        ));
    }
    Ok(())
}

fn validate_challenge_draft_path(path: &str, challenge_id: &str) -> Result<()> {
    let path = path.trim();
    if !challenge_bundle::is_safe_relative_path(path) {
        return Err(AppError::BadRequest(
            "challenge_path must be a safe relative path".to_string(),
        ));
    }
    let expected = format!("challenges/{challenge_id}");
    if path != expected {
        return Err(AppError::BadRequest(format!(
            "challenge_path must be `{expected}`"
        )));
    }
    Ok(())
}

fn validate_private_asset_id(value: &str) -> Result<()> {
    if value.trim().is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::BadRequest(
            "asset_id must contain only ASCII letters, digits, underscores, hyphens, or dots"
                .to_string(),
        ));
    }
    Ok(())
}

async fn validate_draft_repository(
    draft: &ChallengeDraftResponse,
    repository_path: &str,
) -> Result<(ChallengeCreationManifest, String)> {
    let proposal_root = Path::new(repository_path).join(&draft.challenge_path);
    let manifest =
        challenge_creation::validate_challenge_creation_repository(&proposal_root).await?;
    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&manifest)?;
    if manifest_sha256 != draft.manifest_sha256 {
        return Err(AppError::Validation(format!(
            "manifest hash mismatch: draft has {}, repository has {}",
            draft.manifest_sha256, manifest_sha256
        )));
    }
    if manifest.challenge_id != draft.challenge_id {
        return Err(AppError::Validation(format!(
            "manifest challenge_id mismatch: draft has {}, repository has {}",
            draft.challenge_id, manifest.challenge_id
        )));
    }
    let bundle_sha256 = challenge_creation::draft_review_bundle_sha256(
        &proposal_root,
        &manifest,
        &draft.private_assets,
    )
    .await?;
    Ok((manifest, bundle_sha256))
}

async fn assemble_runtime_bundle(
    state: &AppState,
    draft: &ChallengeDraftResponse,
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<PathBuf> {
    let bundle_path = manifest.bundle_path.as_deref().ok_or_else(|| {
        AppError::BadRequest("bundle_path is required for publishable drafts".to_string())
    })?;
    let public_bundle_path = proposal_root.join(bundle_path);
    let public_spec = challenge_bundle::read_challenge_bundle_spec(&public_bundle_path).await?;
    validate_private_assets_for_publish(draft, manifest, &public_spec)?;

    let runtime_bundle_path = Path::new(&state.config.storage_root)
        .join("challenge-bundles")
        .join(&manifest.challenge_id)
        .join(&draft.id);
    challenge_bundle::copy_challenge_bundle_dir(&public_bundle_path, &runtime_bundle_path, true)
        .await?;

    for asset in &draft.private_assets {
        let bytes = state.storage.get(&asset.storage_uri).await?;
        extract_private_asset_overlay(
            &bytes,
            &runtime_bundle_path,
            &asset.asset_id,
            state.config.challenge_private_asset_bytes_per_draft,
        )
        .await?;
    }

    Ok(runtime_bundle_path)
}

fn validate_private_assets_for_publish(
    draft: &ChallengeDraftResponse,
    manifest: &ChallengeCreationManifest,
    spec: &shared::models::challenge::ChallengeBundleSpec,
) -> Result<()> {
    let uploaded: HashSet<&str> = draft
        .private_assets
        .iter()
        .map(|asset| asset.asset_id.as_str())
        .collect();
    for requirement in &manifest.private_assets {
        if requirement.required && !uploaded.contains(requirement.asset_id.as_str()) {
            return Err(AppError::BadRequest(format!(
                "required private asset `{}` has not been uploaded",
                requirement.asset_id
            )));
        }
    }

    let uses_static_private_benchmark = spec.datasets.private_benchmark_enabled
        && spec.execution.official_runs.is_some()
        && spec.execution.official_prepare.is_none();
    let private_benchmark_uploaded = draft
        .private_assets
        .iter()
        .any(|asset| asset.kind == ChallengePrivateAssetKind::PrivateBenchmarkData);
    if uses_static_private_benchmark && !private_benchmark_uploaded {
        return Err(AppError::BadRequest(
            "static official_runs challenges must upload a private_benchmark_data asset"
                .to_string(),
        ));
    }

    Ok(())
}

async fn extract_private_asset_overlay(
    bytes: &[u8],
    target_dir: &Path,
    asset_id: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    let asset_id = asset_id.to_string();
    tokio::task::spawn_blocking(move || {
        extract_private_asset_overlay_blocking(
            &bytes,
            &target_dir,
            &asset_id,
            max_uncompressed_bytes,
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("private asset extraction task failed: {e}")))?
}

fn extract_private_asset_overlay_blocking(
    bytes: &[u8],
    target_dir: &Path,
    asset_id: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_PRIVATE_ASSET_FILE_COUNT {
        return Err(AppError::BadRequest(format!(
            "private asset `{asset_id}` must contain at most {MAX_PRIVATE_ASSET_FILE_COUNT} entries"
        )));
    }

    let mut total_uncompressed_size = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(AppError::BadRequest(format!(
                "private asset `{asset_id}` must not contain symlinks"
            )));
        }

        let Some(relative_path) = file.enclosed_name() else {
            continue;
        };
        let relative_path = relative_path.to_path_buf();
        let relative_path_string = relative_path.to_string_lossy();
        if !challenge_bundle::is_safe_relative_path(&relative_path_string) {
            return Err(AppError::BadRequest(format!(
                "private asset `{asset_id}` contains unsafe path `{relative_path_string}`"
            )));
        }
        let output_path = target_dir.join(&relative_path);

        total_uncompressed_size = total_uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| {
                AppError::BadRequest(format!("private asset `{asset_id}` is too large"))
            })?;
        if total_uncompressed_size > max_uncompressed_bytes {
            return Err(AppError::BadRequest(format!(
                "private asset `{asset_id}` must expand to at most {max_uncompressed_bytes} bytes"
            )));
        }

        if file.is_dir() {
            std::fs::create_dir_all(&output_path)?;
        } else {
            if output_path.exists() {
                return Err(AppError::BadRequest(format!(
                    "private asset `{asset_id}` cannot overwrite bundle file `{relative_path_string}`"
                )));
            }
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&output_path)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

fn non_empty_message(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.decode(input.trim()).ok()
}
