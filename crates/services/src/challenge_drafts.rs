//! GitHub-backed challenge draft lifecycle workflows.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

use tracing::warn;
use uuid::Uuid;

use agentics_config::Config;
use agentics_contracts::validation::github::GithubPullRequestRef;
use agentics_contracts::{challenge_bundle, challenge_creation};
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge_creation::{
    AdminChallengePrivateAssetListResponse, ChallengeCreationManifest,
    ChallengeCreationRequestKind, ChallengeDraftCleanupResponse, ChallengeDraftListResponse,
    ChallengeDraftResponse, ChallengeDraftStatus, ChallengeDraftValidationStatus,
    ChallengePrivateAssetKind, ChallengePrivateAssetResponse, CreatorChallengeDraftResponse,
};
use agentics_domain::models::hashes::{GitCommitSha, Sha256Digest};
use agentics_domain::models::ids::{
    AgentId, ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftPublishClaimId,
    ChallengeDraftValidationRecordId, ChallengePrivateAssetId,
};
use agentics_domain::models::names::ChallengeName;
use agentics_domain::models::paths::{RepoRelativePath, RepositoryCheckoutPath};
use agentics_persistence::{self as persistence, Repositories};
use agentics_storage::{Storage, StorageKey, StorageWriteIntent, storage_work_root};

const CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

mod private_assets;
mod publishing;
mod types;
mod utils;

use private_assets::{extract_private_asset_overlay, validate_private_asset_zip_upload};
pub use publishing::publish_challenge_draft;
pub use types::{
    ChallengeDraftAdmin, ChallengeDraftCreator, CreateChallengeDraftServiceRequest,
    PublishChallengeDraftServiceRequest, ReviewChallengeDraftServiceRequest,
    UploadChallengePrivateAssetServiceRequest, ValidateChallengeDraftServiceRequest,
};
use utils::{base64_decode, cleanup_runtime_bundle, cleanup_storage_key, non_empty_message};

/// Create a challenge draft bound to a public GitHub PR and manifest.
pub async fn create_challenge_draft(
    pool: &sqlx::PgPool,
    config: &Config,
    request: CreateChallengeDraftServiceRequest,
) -> Result<CreatorChallengeDraftResponse> {
    let CreateChallengeDraftServiceRequest { creator, body } = request;
    challenge_creation::validate_challenge_creation_manifest(&body.manifest)?;
    validate_challenge_draft_path(&body.challenge_path, &body.manifest.challenge_name)?;
    GithubPullRequestRef::try_new(
        body.repo_url.clone(),
        body.pr_url.clone(),
        body.pr_number.clone(),
    )?;

    if creator.github_user_id != body.pr_author_github_user_id {
        return Err(ServiceError::BadRequest(format!(
            "PR author GitHub user id {} does not match authenticated creator GitHub user id {}",
            body.pr_author_github_user_id, creator.github_user_id
        )));
    }
    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&body.manifest)?;
    let draft_id = ChallengeDraftId::generate();
    let repo_url = body.repo_url.clone();
    let pr_number = body.pr_number.clone();
    let commit_sha = body.commit_sha;
    let draft = Repositories::new(pool)
        .challenge_drafts()
        .create(
            &persistence::CreateChallengeDraftInput {
                draft_id: draft_id.clone(),
                creator_agent_id: creator.agent_id.clone(),
                max_active_drafts: i64::from(config.quotas.max_active_challenge_drafts_per_agent),
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
            &persistence::CreateChallengeDraftAuditEventInput {
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
        .map_err(ServiceError::unique_violation_as_conflict)?;

    Ok(draft.into())
}

/// Fetch a challenge draft owned by the authenticated agent.
pub async fn get_challenge_draft(
    pool: &sqlx::PgPool,
    creator_agent_id: &AgentId,
    draft_id: &ChallengeDraftId,
) -> Result<CreatorChallengeDraftResponse> {
    let draft = Repositories::new(pool)
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    if draft.creator_agent_id != *creator_agent_id {
        return Err(ServiceError::NotFound);
    }
    Ok(draft.into())
}

/// Upload a private benchmark asset for a draft owned by the authenticated agent.
pub async fn upload_challenge_private_asset(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: UploadChallengePrivateAssetServiceRequest,
) -> Result<ChallengePrivateAssetResponse> {
    let UploadChallengePrivateAssetServiceRequest {
        creator_agent_id,
        draft_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let draft = repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    if draft.creator_agent_id != creator_agent_id {
        return Err(ServiceError::NotFound);
    }
    if matches!(
        draft.status,
        ChallengeDraftStatus::Rejected
            | ChallengeDraftStatus::Approved
            | ChallengeDraftStatus::Publishing
            | ChallengeDraftStatus::Published
            | ChallengeDraftStatus::Abandoned
    ) {
        return Err(ServiceError::Conflict);
    }

    let requirement = draft
        .manifest
        .private_assets
        .iter()
        .find(|asset| asset.asset_name == body.asset_name)
        .ok_or_else(|| {
            ServiceError::BadRequest(format!(
                "private asset `{}` is not declared in the challenge manifest",
                body.asset_name
            ))
        })?;
    if requirement.kind != body.kind {
        return Err(ServiceError::BadRequest(format!(
            "private asset `{}` kind mismatch",
            body.asset_name
        )));
    }

    let asset_bytes = base64_decode(&body.asset_base64).ok_or(ServiceError::Base64)?;
    let asset_size_bytes = u64::try_from(asset_bytes.len()).map_err(|_| {
        ServiceError::BadRequest("private asset size exceeds supported range".to_string())
    })?;
    if asset_size_bytes > config.quotas.challenge_private_asset_bytes_per_draft {
        return Err(ServiceError::BadRequest(format!(
            "private asset must be at most {} bytes",
            config.quotas.challenge_private_asset_bytes_per_draft
        )));
    }
    let asset_size_bytes_i64 = i64::try_from(asset_size_bytes).map_err(|_| {
        ServiceError::BadRequest("private asset size exceeds supported database range".to_string())
    })?;
    validate_private_asset_zip_upload(
        &asset_bytes,
        body.asset_name.as_str(),
        config.quotas.challenge_private_asset_bytes_per_draft,
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
    repos
        .challenge_drafts()
        .reserve_private_asset(
            &persistence::CreateChallengePrivateAssetInput {
                asset_row_id: asset_row_id.clone(),
                draft_id: draft.id.clone(),
                asset_name: body.asset_name.clone(),
                kind: body.kind,
                required: requirement.required,
                size_bytes: asset_size_bytes_i64,
                sha256,
                storage_key: storage_key.clone(),
                temporary_storage_key: temporary_asset_key.clone(),
                uploader_agent_id: creator_agent_id.clone(),
            },
            config.quotas.challenge_private_asset_bytes_per_draft,
            config.quotas.challenge_draft_validation_timeout_minutes,
            config
                .quotas
                .challenge_private_asset_pending_timeout_minutes,
        )
        .await
        .map_err(ServiceError::unique_violation_as_conflict)?;

    let temporary_storage_key = match storage
        .put(
            &temporary_asset_key,
            &asset_bytes,
            StorageWriteIntent::new(
                "challenge private asset ZIP",
                config.quotas.challenge_private_asset_bytes_per_draft,
            ),
        )
        .await
    {
        Ok(key) => key,
        Err(error) => {
            fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
            cleanup_storage_key(storage, &temporary_asset_key).await;
            return Err(error.into());
        }
    };

    if let Err(error) = cleanup_unreferenced_private_asset_object(pool, storage, &storage_key).await
    {
        fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
        cleanup_storage_key(storage, &temporary_storage_key).await;
        return Err(error);
    }

    if let Err(error) = storage.promote(&temporary_storage_key, &storage_key).await {
        fail_challenge_private_asset_record(pool, &asset_row_id, &error.to_string()).await;
        cleanup_storage_key(storage, &temporary_storage_key).await;
        return Err(error.into());
    }
    let asset = match Repositories::new(pool)
        .challenge_drafts()
        .activate_private_asset_with_audit(
            &asset_row_id,
            ChallengeDraftAuditEventId::generate(),
            &creator_agent_id,
        )
        .await
    {
        Ok(asset) => asset,
        Err(error) => {
            cleanup_storage_key(storage, &storage_key).await;
            return Err(error);
        }
    };

    Ok(asset)
}

/// Marks the pending private asset failed when storage writes cannot complete.
async fn fail_challenge_private_asset_record(
    pool: &sqlx::PgPool,
    asset_row_id: &ChallengePrivateAssetId,
    message: &str,
) {
    if let Err(error) = Repositories::new(pool)
        .challenge_drafts()
        .fail_private_asset(asset_row_id, message)
        .await
    {
        warn!(
            asset_row_id = %asset_row_id,
            error = %error,
            "failed to mark private asset upload failed after storage error"
        );
    }
}

/// Remove an unreferenced durable object left by a stale pending private asset.
async fn cleanup_unreferenced_private_asset_object(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    storage_key: &StorageKey,
) -> Result<()> {
    if !storage.exists(storage_key).await? {
        return Ok(());
    }
    if Repositories::new(pool)
        .challenge_drafts()
        .private_asset_storage_key_has_active_reference(storage_key)
        .await?
    {
        return Err(ServiceError::Conflict);
    }
    cleanup_storage_key(storage, storage_key).await;
    Ok(())
}

/// List GitHub-backed challenge drafts for admin review.
pub async fn list_admin_challenge_drafts(
    pool: &sqlx::PgPool,
) -> Result<ChallengeDraftListResponse> {
    let items = Repositories::new(pool).challenge_drafts().list(100).await?;
    Ok(ChallengeDraftListResponse { items })
}

/// List every private asset lifecycle record for one draft for admin review.
pub async fn list_admin_challenge_draft_private_assets(
    pool: &sqlx::PgPool,
    draft_id: &ChallengeDraftId,
) -> Result<AdminChallengePrivateAssetListResponse> {
    let repos = Repositories::new(pool);
    repos
        .challenge_drafts()
        .get(draft_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    let items = repos
        .challenge_drafts()
        .list_private_asset_states(draft_id.as_str())
        .await?;
    Ok(AdminChallengePrivateAssetListResponse { items })
}

/// Validate a draft against a checked-out challenge repository path.
pub async fn validate_challenge_draft(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: ValidateChallengeDraftServiceRequest,
) -> Result<ChallengeDraftResponse> {
    let ValidateChallengeDraftServiceRequest {
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
    if !matches!(
        draft.status,
        ChallengeDraftStatus::Draft | ChallengeDraftStatus::Validated
    ) {
        return Err(ServiceError::Conflict);
    }
    let validation_limit = i64::from(config.quotas.challenge_draft_validations_per_day);
    let repository_path = RepositoryCheckoutPath::from_existing_dir(&body.repository_path)?;
    let validation_record_id = ChallengeDraftValidationRecordId::generate();
    repos
        .challenge_drafts()
        .begin_validation(
            &persistence::BeginChallengeDraftValidationInput {
                validation_record_id: validation_record_id.clone(),
                draft_id: draft.id.clone(),
                repository_path: repository_path.to_string(),
                manifest_sha256: draft.manifest_sha256,
            },
            CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS,
            validation_limit,
            config.quotas.challenge_draft_validation_timeout_minutes,
        )
        .await?;
    let validation = validate_draft_repository(storage, config, &draft, &repository_path).await;

    match validation {
        Ok((_, bundle_sha256)) => {
            let message = "challenge draft validation passed".to_string();
            let audit_event = persistence::CreateChallengeDraftAuditEventInput {
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
            repos
                .challenge_drafts()
                .finish_validation(
                    &persistence::FinishChallengeDraftValidationInput {
                        validation_record_id,
                        draft_id: draft.id.clone(),
                        status: ChallengeDraftValidationStatus::Passed,
                        message: message.clone(),
                        bundle_sha256: Some(bundle_sha256),
                    },
                    &audit_event,
                )
                .await?;
            let draft = repos
                .challenge_drafts()
                .get(draft.id.as_str())
                .await?
                .ok_or(ServiceError::NotFound)?;
            Ok(draft)
        }
        Err(error) => {
            let message = error.to_string();
            let audit_event = persistence::CreateChallengeDraftAuditEventInput {
                event_id: ChallengeDraftAuditEventId::generate(),
                draft_id: draft.id.clone(),
                actor_agent_id: None,
                actor_admin_username: Some(admin.username.clone()),
                action: "draft_validation_failed".to_string(),
                message: message.clone(),
                metadata: serde_json::json!({ "repository_path": repository_path.to_string() }),
            };
            repos
                .challenge_drafts()
                .finish_validation(
                    &persistence::FinishChallengeDraftValidationInput {
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

/// Expire stale drafts and purge private assets for rejected or abandoned
/// unpublished drafts after the configured grace period.
pub async fn cleanup_challenge_drafts(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
) -> Result<ChallengeDraftCleanupResponse> {
    let repos = Repositories::new(pool);
    let abandoned = repos
        .challenge_drafts()
        .abandon_stale(config.quotas.challenge_draft_ttl_days)
        .await?;
    let purge_candidates = repos
        .challenge_drafts()
        .list_unpublished_private_assets_for_purge(
            config.quotas.unpublished_challenge_asset_grace_days,
        )
        .await?;

    let mut purged = 0_i64;
    for asset in purge_candidates {
        let Some(asset) = repos
            .challenge_drafts()
            .mark_private_asset_purging(&asset.id)
            .await?
        else {
            continue;
        };
        storage.delete(&asset.storage_key).await?;
        if let Some(temporary_storage_key) = &asset.temporary_storage_key {
            storage.delete(temporary_storage_key).await?;
        }
        repos
            .challenge_drafts()
            .delete_private_asset(asset.id.as_str())
            .await?;
        purged = purged.checked_add(1).ok_or_else(|| {
            ServiceError::Internal("private asset purge count overflow".to_string())
        })?;
    }
    let tmp_cutoff = temporary_storage_cleanup_cutoff(config)?;
    let purged_temporary_storage_objects = storage
        .delete_prefix_older_than(&StorageKey::try_new("_tmp")?, tmp_cutoff)
        .await?;
    let purged_temporary_storage_objects = i64::try_from(purged_temporary_storage_objects)
        .map_err(|_| {
            ServiceError::Internal(
                "temporary storage cleanup count exceeds supported range".to_string(),
            )
        })?;

    Ok(ChallengeDraftCleanupResponse {
        abandoned_drafts: abandoned,
        purged_private_assets: purged,
        purged_temporary_storage_objects,
    })
}

fn temporary_storage_cleanup_cutoff(config: &Config) -> Result<SystemTime> {
    let seconds = config
        .storage
        .tmp_object_grace_hours
        .checked_mul(60 * 60)
        .ok_or_else(|| {
            ServiceError::Internal("temporary storage grace window overflow".to_string())
        })?;
    SystemTime::now()
        .checked_sub(Duration::from_secs(seconds))
        .ok_or_else(|| {
            ServiceError::Internal("temporary storage cleanup cutoff underflow".to_string())
        })
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

/// Ensures a draft path follows the canonical `challenges/{challenge_name}` repository layout.
fn validate_challenge_draft_path(
    path: &RepoRelativePath,
    challenge_name: &ChallengeName,
) -> Result<()> {
    let expected = format!("challenges/{challenge_name}");
    if path.as_str() != expected {
        return Err(ServiceError::BadRequest(format!(
            "challenge_path must be `{expected}`"
        )));
    }
    Ok(())
}

/// Validates the checked-out proposal against the manifest hash recorded at draft creation.
pub(super) async fn validate_draft_repository(
    storage: &dyn Storage,
    config: &Config,
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
        return Err(ServiceError::Validation(format!(
            "manifest hash mismatch: draft has {}, repository has {}",
            draft.manifest_sha256, manifest_sha256
        )));
    }
    if manifest.challenge_name != draft.challenge_name {
        return Err(ServiceError::Validation(format!(
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
            validate_and_hash_runtime_bundle(storage, config, draft, &proposal_root, &manifest)
                .await?
        }
    };
    Ok((manifest, bundle_sha256))
}

/// Assemble private overlays, validate the runtime bundle, and return its review digest.
async fn validate_and_hash_runtime_bundle(
    storage: &dyn Storage,
    config: &Config,
    draft: &ChallengeDraftResponse,
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<Sha256Digest> {
    let validation_claim_id = ChallengeDraftPublishClaimId::generate();
    let runtime_bundle_path = temporary_runtime_bundle_path(config, draft, &validation_claim_id)?;
    let validation: Result<Sha256Digest> = async {
        assemble_runtime_bundle(
            storage,
            config,
            draft,
            proposal_root,
            manifest,
            &runtime_bundle_path,
        )
        .await?;
        challenge_bundle::validate_challenge_bundle(&runtime_bundle_path).await?;
        let spec = challenge_bundle::read_challenge_bundle_spec(&runtime_bundle_path).await?;
        if config.requires_digest_pinned_images() {
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
            ServiceError::Validation(format!("repository HEAD is not a valid Git commit: {e}"))
        })?;
        if head != expected_commit {
            return Err(ServiceError::Validation(format!(
                "repository HEAD commit {} does not match reviewed draft commit {}",
                head, expected_commit
            )));
        }

        let status = run_git(&repository_path, &["status", "--porcelain=v1"])?;
        if !status.trim().is_empty() {
            return Err(ServiceError::Validation(
                "repository checkout has uncommitted changes; validate and publish from a clean checkout at the reviewed commit"
                    .to_string(),
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("repository Git inspection task failed: {e}")))?
}

/// Run one Git command inside the reviewed repository checkout and return stdout as UTF-8.
fn run_git(repository_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository_path)
        .args(args)
        .output()
        .map_err(|e| {
            ServiceError::Validation(format!("failed to inspect repository with git: {e}"))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ServiceError::Validation(format!(
            "failed to inspect repository with git: {}",
            stderr.trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| ServiceError::Validation(format!("git output was not UTF-8: {e}")))
}

/// Builds the managed runtime bundle by combining public bundle files and private overlays.
pub(super) async fn assemble_runtime_bundle(
    storage: &dyn Storage,
    config: &Config,
    draft: &ChallengeDraftResponse,
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    runtime_bundle_path: &Path,
) -> Result<()> {
    let bundle_path = manifest.bundle_path.as_ref().ok_or_else(|| {
        ServiceError::BadRequest("bundle_path is required for publishable drafts".to_string())
    })?;
    let public_bundle_path = proposal_root.join(bundle_path.as_path());
    let public_spec = challenge_bundle::read_challenge_bundle_spec(&public_bundle_path).await?;
    validate_private_assets_for_publish(draft, manifest, &public_spec)?;

    challenge_bundle::copy_challenge_bundle_dir(&public_bundle_path, runtime_bundle_path, true)
        .await?;

    for asset in &draft.private_assets {
        let bytes = storage
            .get(
                &asset.storage_key,
                StorageWriteIntent::new(
                    "challenge private asset ZIP",
                    config.quotas.challenge_private_asset_bytes_per_draft,
                ),
            )
            .await?;
        extract_private_asset_overlay(
            &bytes,
            runtime_bundle_path,
            &asset.asset_name,
            config.quotas.challenge_private_asset_bytes_per_draft,
        )
        .await?;
    }
    validate_private_asset_required_paths(draft, manifest, runtime_bundle_path).await?;

    Ok(())
}

/// Builds the managed public-only bundle from the reviewed public challenge checkout.
pub(super) async fn assemble_public_bundle(
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    public_runtime_bundle_path: &Path,
) -> Result<()> {
    let bundle_path = manifest.bundle_path.as_ref().ok_or_else(|| {
        ServiceError::BadRequest("bundle_path is required for publishable drafts".to_string())
    })?;
    let public_bundle_path = proposal_root.join(bundle_path.as_path());
    challenge_bundle::copy_challenge_bundle_dir(
        &public_bundle_path,
        public_runtime_bundle_path,
        true,
    )
    .await
}

/// Attempt-scoped temporary runtime-bundle path under local storage work root.
pub(super) fn temporary_runtime_bundle_path(
    config: &Config,
    draft: &ChallengeDraftResponse,
    publish_claim_id: &ChallengeDraftPublishClaimId,
) -> Result<PathBuf> {
    Ok(storage_work_root(config)?
        .join("_tmp")
        .join("challenge-bundles")
        .join(format!(
            "{}-{}-{}",
            draft.id,
            publish_claim_id,
            Uuid::new_v4()
        )))
}

/// Attempt-scoped temporary public-only bundle path under local storage work root.
pub(super) fn temporary_public_runtime_bundle_path(
    config: &Config,
    draft: &ChallengeDraftResponse,
    publish_claim_id: &ChallengeDraftPublishClaimId,
) -> Result<PathBuf> {
    Ok(storage_work_root(config)?
        .join("_tmp")
        .join("challenge-public-bundles")
        .join(format!(
            "{}-{}-{}",
            draft.id,
            publish_claim_id,
            Uuid::new_v4()
        )))
}

/// Verifies every private asset required by the manifest and bundle shape is present.
fn validate_private_assets_for_publish(
    draft: &ChallengeDraftResponse,
    manifest: &ChallengeCreationManifest,
    spec: &agentics_domain::models::challenge::ChallengeBundleSpec,
) -> Result<()> {
    let uploaded: HashSet<&str> = draft
        .private_assets
        .iter()
        .map(|asset| asset.asset_name.as_str())
        .collect();
    for requirement in &manifest.private_assets {
        if requirement.required && !uploaded.contains(requirement.asset_name.as_str()) {
            return Err(ServiceError::BadRequest(format!(
                "required private asset `{}` has not been uploaded",
                requirement.asset_name
            )));
        }
    }

    let uses_static_private_benchmark = spec.datasets.private_benchmark_enabled
        && match &spec.execution {
            agentics_domain::models::challenge::ChallengeExecutionSpec::SeparatedEvaluator(
                execution,
            ) => execution.official_runs.is_some() && execution.official_evaluation_setup.is_none(),
            agentics_domain::models::challenge::ChallengeExecutionSpec::PipedStdio(execution) => {
                execution.official_session.is_some()
                    && execution.official_evaluation_setup.is_none()
            }
            agentics_domain::models::challenge::ChallengeExecutionSpec::CoexecutedBenchmark(_) => {
                false
            }
        };
    let private_benchmark_uploaded = draft
        .private_assets
        .iter()
        .any(|asset| asset.kind == ChallengePrivateAssetKind::PrivateBenchmarkData);
    if uses_static_private_benchmark && !private_benchmark_uploaded {
        return Err(ServiceError::BadRequest(
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
            return Err(ServiceError::BadRequest(format!(
                "private asset `{}` did not provide required runtime path `{required_path}`",
                requirement.asset_name
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
