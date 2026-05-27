//! Draft publishing workflow helpers.

use std::path::Path;

use uuid::Uuid;

use agentics_config::Config;
use agentics_contracts::challenge_bundle;
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeCreationRequestKind, ChallengeDraftResponse,
    ChallengeDraftStatus,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{ChallengeDraftAuditEventId, ChallengeDraftPublishClaimId};
use agentics_domain::models::paths::RepositoryCheckoutPath;
use agentics_persistence::{self as persistence, Repositories};
use agentics_storage::{
    Storage, StorageKey, StorageWriteIntent, pack_directory_to_tar, storage_work_root,
};

use super::types::PublishChallengeDraftServiceRequest;
use super::utils::{
    challenge_bundle_storage_key, cleanup_file, cleanup_runtime_bundle, cleanup_storage_key,
};
use super::{
    assemble_public_bundle, assemble_runtime_bundle, temporary_public_runtime_bundle_path,
    temporary_runtime_bundle_path, validate_draft_repository,
};

/// Publish an approved draft into an immutable challenge contract.
pub async fn publish_challenge_draft(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: PublishChallengeDraftServiceRequest,
) -> Result<ChallengeDraftResponse> {
    let PublishChallengeDraftServiceRequest {
        admin,
        draft_id,
        body,
    } = request;
    let repository_path = RepositoryCheckoutPath::from_existing_dir(&body.repository_path)?;
    let repos = Repositories::new(pool);
    let claim = repos
        .challenge_drafts()
        .claim_for_publish(
            draft_id.as_str(),
            config.quotas.challenge_draft_publish_timeout_minutes,
        )
        .await?;
    let draft = claim.draft;
    if draft.status == ChallengeDraftStatus::Published {
        return Ok(draft);
    }
    let publish_claim_id = claim.publish_claim_id.ok_or_else(|| {
        ServiceError::Internal("publishing draft claim missing publish claim id".to_string())
    })?;
    let publish_result = publish_claimed_challenge_draft(
        pool,
        storage,
        config,
        &admin.username,
        &draft,
        &publish_claim_id,
        &repository_path,
    )
    .await;
    if let Err(error) = publish_result {
        repos
            .challenge_drafts()
            .fail_publish(draft.id.as_str(), &publish_claim_id, &error.to_string())
            .await?;
        return Err(error);
    }

    repos
        .challenge_drafts()
        .get(draft.id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)
}

/// Publish a draft that has already been claimed with `publishing` status.
async fn publish_claimed_challenge_draft(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    admin_username: &str,
    draft: &ChallengeDraftResponse,
    publish_claim_id: &ChallengeDraftPublishClaimId,
    repository_path: &RepositoryCheckoutPath,
) -> Result<()> {
    let (manifest, bundle_sha256) =
        validate_draft_repository(storage, config, draft, repository_path).await?;
    let approved_bundle_sha256 = draft
        .approved_bundle_sha256
        .as_ref()
        .ok_or_else(|| ServiceError::Conflict)?;
    if *approved_bundle_sha256 != bundle_sha256 {
        return Err(ServiceError::Validation(
            "challenge draft content changed after approval; validate and approve the draft again before publishing"
                .to_string(),
        ));
    }
    let proposal_root = repository_path
        .as_path()
        .join(draft.challenge_path.as_path());
    match manifest.request {
        ChallengeCreationRequestKind::ArchiveChallenge => {
            Repositories::new(pool)
                .challenge_drafts()
                .publish_archive(&persistence::PublishArchiveChallengeDraftInput {
                    draft_id: draft.id.clone(),
                    publish_claim_id: publish_claim_id.clone(),
                    challenge_name: manifest.challenge_name.clone(),
                    owner_agent_id: draft.creator_agent_id.clone(),
                    audit_event_id: ChallengeDraftAuditEventId::generate(),
                    admin_username: admin_username.to_string(),
                    repository_path: repository_path.to_string(),
                    bundle_sha256,
                })
                .await?;
        }
        ChallengeCreationRequestKind::NewChallenge => {
            let temporary_bundle_path =
                temporary_runtime_bundle_path(config, draft, publish_claim_id)?;
            let temporary_public_bundle_path =
                temporary_public_runtime_bundle_path(config, draft, publish_claim_id)?;

            let publish_new_result = prepare_and_publish_new_challenge_draft(
                pool,
                storage,
                config,
                PublishNewChallengeDraftContext {
                    admin_username,
                    draft,
                    publish_claim_id,
                    repository_path,
                    proposal_root: &proposal_root,
                    manifest: &manifest,
                    bundle_sha256,
                    temporary_bundle_path: &temporary_bundle_path,
                    temporary_public_bundle_path: &temporary_public_bundle_path,
                },
            )
            .await;
            cleanup_runtime_bundle(&temporary_bundle_path).await;
            cleanup_runtime_bundle(&temporary_public_bundle_path).await;
            publish_new_result?;
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
    temporary_public_bundle_path: &'a Path,
}

/// Assemble, validate, promote, and commit a new challenge publish attempt.
async fn prepare_and_publish_new_challenge_draft(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    ctx: PublishNewChallengeDraftContext<'_>,
) -> Result<()> {
    assemble_runtime_bundle(
        storage,
        config,
        ctx.draft,
        ctx.proposal_root,
        ctx.manifest,
        ctx.temporary_bundle_path,
    )
    .await?;
    assemble_public_bundle(
        ctx.proposal_root,
        ctx.manifest,
        ctx.temporary_public_bundle_path,
    )
    .await?;
    challenge_bundle::validate_challenge_bundle(ctx.temporary_bundle_path).await?;
    let spec = challenge_bundle::read_challenge_bundle_spec(ctx.temporary_bundle_path).await?;
    if config.requires_digest_pinned_images() {
        challenge_bundle::validate_digest_pinned_images(&spec)?;
    }
    let bundle_key = challenge_bundle_storage_key(
        "challenge-bundles",
        ctx.manifest.challenge_name.as_str(),
        ctx.draft.id.as_str(),
        ctx.publish_claim_id.as_str(),
    )?;
    let public_bundle_key = challenge_bundle_storage_key(
        "challenge-public-bundles",
        ctx.manifest.challenge_name.as_str(),
        ctx.draft.id.as_str(),
        ctx.publish_claim_id.as_str(),
    )?;
    let statement_key = StorageKey::try_new(format!(
        "challenge-statements/{}/{}-{}.md",
        ctx.manifest.challenge_name, ctx.draft.id, ctx.publish_claim_id
    ))?;
    let private_archive_path = storage_work_root(config)?
        .join("_tmp")
        .join(format!("bundle-{}.tar", Uuid::new_v4()));
    let public_archive_path = storage_work_root(config)?
        .join("_tmp")
        .join(format!("public-bundle-{}.tar", Uuid::new_v4()));
    let storage_result = async {
        let bundle_archive_intent = StorageWriteIntent::new(
            "challenge bundle archive",
            config.storage.max_bundle_archive_bytes,
        );
        pack_directory_to_tar(
            ctx.temporary_bundle_path,
            &private_archive_path,
            bundle_archive_intent,
        )
        .await?;
        pack_directory_to_tar(
            ctx.temporary_public_bundle_path,
            &public_archive_path,
            bundle_archive_intent,
        )
        .await?;
        storage
            .put_file(
                &bundle_key,
                &private_archive_path,
                StorageWriteIntent::new(
                    "challenge bundle archive",
                    config.storage.max_bundle_archive_bytes,
                ),
            )
            .await?;
        storage
            .put_file(
                &public_bundle_key,
                &public_archive_path,
                StorageWriteIntent::new(
                    "challenge bundle archive",
                    config.storage.max_bundle_archive_bytes,
                ),
            )
            .await?;
        let statement_bytes =
            tokio::fs::read(ctx.temporary_bundle_path.join("statement.md")).await?;
        storage
            .put(
                &statement_key,
                &statement_bytes,
                StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
            )
            .await?;
        Ok::<(), ServiceError>(())
    }
    .await;
    cleanup_file(&private_archive_path).await;
    cleanup_file(&public_archive_path).await;
    if let Err(error) = storage_result {
        cleanup_storage_key(storage, &statement_key).await;
        cleanup_storage_key(storage, &public_bundle_key).await;
        cleanup_storage_key(storage, &bundle_key).await;
        return Err(error);
    }
    let publish_result = Repositories::new(pool)
        .challenge_drafts()
        .publish_new(&persistence::PublishNewChallengeDraftInput {
            draft_id: ctx.draft.id.clone(),
            publish_claim_id: ctx.publish_claim_id.clone(),
            challenge_name: ctx.manifest.challenge_name.clone(),
            bundle_key: bundle_key.clone(),
            public_bundle_key: public_bundle_key.clone(),
            statement_key: statement_key.clone(),
            spec,
            title: ctx.manifest.title.clone(),
            summary: ctx.manifest.summary.clone(),
            owner_agent_id: ctx.draft.creator_agent_id.clone(),
            audit_event_id: ChallengeDraftAuditEventId::generate(),
            admin_username: ctx.admin_username.to_string(),
            repository_path: ctx.repository_path.to_string(),
            bundle_sha256: ctx.bundle_sha256,
        })
        .await;
    if let Err(error) = publish_result {
        cleanup_storage_key(storage, &statement_key).await;
        cleanup_storage_key(storage, &public_bundle_key).await;
        cleanup_storage_key(storage, &bundle_key).await;
        return Err(error);
    }
    Ok(())
}
