//! HTTP handlers for GitHub-backed challenge creation drafts.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use shared::error::{AppError, Result};
use shared::models::challenge_creation::{
    ChallengeCreationRequestKind, ChallengeDraftListResponse, ChallengeDraftResponse,
    ChallengeDraftStatus, ChallengeDraftValidationStatus, ChallengePrivateAssetResponse,
    CreateChallengeDraftRequest, GithubIdentityResponse, LinkGithubIdentityRequest,
    ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest, ValidateChallengeDraftRequest,
};
use shared::{challenge_bundle, challenge_creation, db};

use crate::extractors::{AdminAuth, AgentAuth, ValidatedJson};
use crate::state::AppState;

const MAX_PRIVATE_ASSET_BYTES: u64 = 100 * 1024 * 1024;

/// Link the authenticated agent to a GitHub identity that admins have verified.
pub async fn link_github_identity(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<LinkGithubIdentityRequest>,
) -> Result<Json<GithubIdentityResponse>> {
    let identity = db::link_agent_github_identity(
        &state.db,
        &db::LinkGithubIdentityInput {
            agent_id: agent.agent_id,
            github_user_id: body.github_user_id,
            github_login: body.github_login.trim().to_string(),
        },
    )
    .await
    .map_err(map_unique_conflict)?;

    Ok(Json(identity))
}

/// Create a challenge draft bound to a public GitHub PR and manifest.
pub async fn create_challenge_draft(
    State(state): State<AppState>,
    agent: AgentAuth,
    ValidatedJson(body): ValidatedJson<CreateChallengeDraftRequest>,
) -> Result<(StatusCode, Json<ChallengeDraftResponse>)> {
    validate_github_pr_metadata(&body)?;
    challenge_creation::validate_challenge_creation_manifest(&body.manifest)?;
    validate_challenge_draft_path(&body.challenge_path, &body.manifest.challenge_id)?;

    let identity = db::get_agent_github_identity(&state.db, &agent.agent_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(
                "agent must link a GitHub identity before creating challenge drafts".to_string(),
            )
        })?;
    if identity.github_user_id != body.pr_author_github_user_id {
        return Err(AppError::BadRequest(format!(
            "PR author GitHub user id {} does not match linked agent GitHub user id {}",
            body.pr_author_github_user_id, identity.github_user_id
        )));
    }

    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&body.manifest)?;
    let draft = db::create_challenge_draft(
        &state.db,
        &db::CreateChallengeDraftInput {
            draft_id: Uuid::new_v4().to_string(),
            creator_agent_id: agent.agent_id.clone(),
            creator_github_user_id: identity.github_user_id,
            creator_github_login: identity.github_login,
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
            actor_agent_id: Some(agent.agent_id.clone()),
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
    agent: AgentAuth,
    Path(draft_id): Path<String>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.creator_agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    Ok(Json(draft))
}

/// Upload a private benchmark asset for a draft owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    State(state): State<AppState>,
    agent: AgentAuth,
    Path(draft_id): Path<String>,
    ValidatedJson(body): ValidatedJson<UploadChallengePrivateAssetRequest>,
) -> Result<(StatusCode, Json<ChallengePrivateAssetResponse>)> {
    validate_private_asset_id(&body.asset_id)?;

    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if draft.creator_agent_id != agent.agent_id {
        return Err(AppError::NotFound);
    }
    if matches!(
        draft.status,
        ChallengeDraftStatus::Rejected
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
    if asset_bytes.len() as u64 > MAX_PRIVATE_ASSET_BYTES {
        return Err(AppError::BadRequest(format!(
            "private asset must be at most {} bytes",
            MAX_PRIVATE_ASSET_BYTES
        )));
    }
    let sha256 = challenge_creation::sha256_hex(&asset_bytes);
    let storage_path = format!(
        "challenge-drafts/{}/private-assets/{}-{}.bin",
        draft.id, body.asset_id, sha256
    );
    let storage_uri = state.storage.put(&storage_path, &asset_bytes).await?;
    let asset = db::create_challenge_private_asset(
        &state.db,
        &db::CreateChallengePrivateAssetInput {
            asset_id_row: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            asset_id: body.asset_id,
            kind: body.kind,
            required: requirement.required,
            size_bytes: asset_bytes.len() as i64,
            sha256,
            storage_uri,
            uploader_agent_id: agent.agent_id.clone(),
        },
    )
    .await;

    let asset = match asset {
        Ok(asset) => asset,
        Err(error) => {
            let _ = state.storage.delete(&storage_path).await;
            return Err(map_unique_conflict(error));
        }
    };

    db::create_challenge_draft_audit_event(
        &state.db,
        &db::CreateChallengeDraftAuditEventInput {
            event_id: Uuid::new_v4().to_string(),
            draft_id: draft.id.clone(),
            actor_agent_id: Some(agent.agent_id.clone()),
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
    Path(draft_id): Path<String>,
    ValidatedJson(body): ValidatedJson<ValidateChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let repository_path = body.repository_path.trim();
    let validation = validate_draft_repository(&draft, repository_path).await;

    match validation {
        Ok(message) => {
            db::record_challenge_draft_validation(
                &state.db,
                &Uuid::new_v4().to_string(),
                &draft.id,
                ChallengeDraftValidationStatus::Passed,
                &message,
                repository_path,
                &draft.manifest_sha256,
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
                    metadata: serde_json::json!({ "repository_path": repository_path }),
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
                &Uuid::new_v4().to_string(),
                &draft.id,
                ChallengeDraftValidationStatus::Failed,
                &message,
                repository_path,
                &draft.manifest_sha256,
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

/// Approve a validated draft for publishing.
pub async fn approve_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    ValidatedJson(body): ValidatedJson<ReviewChallengeDraftRequest>,
) -> Result<Json<ChallengeDraftResponse>> {
    let draft = db::get_challenge_draft(&state.db, &draft_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !matches!(
        draft.status,
        ChallengeDraftStatus::Validated | ChallengeDraftStatus::Approved
    ) {
        return Err(AppError::Conflict);
    }
    db::update_challenge_draft_status(
        &state.db,
        &draft.id,
        ChallengeDraftStatus::Approved,
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
            action: "draft_approved".to_string(),
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

/// Reject a draft with reviewer feedback.
pub async fn reject_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
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

/// Publish an approved draft into immutable challenge/version rows.
pub async fn publish_challenge_draft(
    admin: AdminAuth,
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
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
    validate_draft_repository(&draft, repository_path).await?;
    let proposal_root = std::path::Path::new(repository_path).join(&draft.challenge_path);
    let manifest = challenge_creation::read_challenge_creation_manifest(&proposal_root).await?;
    let Some(version) = manifest.version.as_ref() else {
        return Err(AppError::BadRequest(
            "archive challenge publishing is handled by the archive lifecycle milestone"
                .to_string(),
        ));
    };

    if manifest.request == ChallengeCreationRequestKind::ArchiveChallenge {
        return Err(AppError::BadRequest(
            "archive challenge publishing is handled by the archive lifecycle milestone"
                .to_string(),
        ));
    }

    let bundle_path = proposal_root.join(&version.bundle_path);
    let spec = challenge_bundle::read_challenge_bundle_spec(&bundle_path).await?;
    db::create_or_update_challenge(
        &state.db,
        &manifest.challenge_id,
        &manifest.challenge_id,
        &manifest.title,
        &manifest.summary,
    )
    .await?;
    let statement_path = bundle_path.join("statement.md");
    let published = db::publish_challenge_version(
        &state.db,
        &manifest.challenge_id,
        &bundle_path.to_string_lossy(),
        &statement_path.to_string_lossy(),
        &spec,
        &manifest.title,
        &manifest.summary,
    )
    .await?;
    db::mark_challenge_draft_published(&state.db, &draft.id, &published.version_id).await?;
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
                "challenge_id": &published.challenge_id,
                "version_id": &published.version_id,
                "repository_path": repository_path
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
) -> Result<String> {
    let proposal_root = std::path::Path::new(repository_path).join(&draft.challenge_path);
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
    Ok("challenge draft validation passed".to_string())
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
