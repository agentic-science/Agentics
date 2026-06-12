//! Review-record publishing workflow helpers.

use std::path::Path;

use uuid::Uuid;

use agentics_config::Config;
use agentics_contracts::challenge_bundle;
use agentics_domain::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeCreationRequestKind, ChallengeReviewRecordResponse,
    ChallengeReviewRecordStatus,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{ChallengeReviewAuditEventId, ChallengeReviewPublishClaimId};
use agentics_domain::models::paths::RepositoryCheckoutPath;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};
use agentics_storage::{Storage, StorageKey, StorageWriteIntent, pack_directory_to_tar};

use super::presentation::review_record_response;
use super::types::{ChallengeReviewRecordAdmin, PublishChallengeReviewRecordServiceRequest};
use super::utils::{
    challenge_bundle_storage_key, cleanup_file, cleanup_runtime_bundle, cleanup_storage_key,
};
use super::{
    assemble_public_bundle, assemble_runtime_bundle, temporary_public_runtime_bundle_path,
    temporary_runtime_bundle_path, validate_review_record_repository,
};
use crate::storage_errors::storage_error_to_service_error;

/// Publish an approved review_record into an immutable challenge contract.
pub async fn publish_challenge_review_record(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: PublishChallengeReviewRecordServiceRequest,
) -> Result<ChallengeReviewRecordResponse> {
    let PublishChallengeReviewRecordServiceRequest {
        admin,
        review_record_id,
        body,
    } = request;
    let repository_path = RepositoryCheckoutPath::from_existing_dir(&body.repository_path)?;
    let repos = Repositories::new(pool);
    let claim = repos
        .challenge_review_records()
        .claim_for_publish(
            &review_record_id,
            config
                .quotas
                .challenge_review_record_publish_timeout_minutes,
        )
        .await?;
    let review_record = review_record_response(claim.review_record);
    if review_record.status == ChallengeReviewRecordStatus::Published {
        return Ok(review_record);
    }
    let publish_claim_id = claim.publish_claim_id.ok_or_else(|| {
        ServiceError::Internal(
            "publishing review_record claim missing publish claim id".to_string(),
        )
    })?;
    let publish_result = publish_claimed_challenge_review_record(
        pool,
        storage,
        config,
        &admin,
        &review_record,
        &publish_claim_id,
        &repository_path,
    )
    .await;
    if let Err(error) = publish_result {
        repos
            .challenge_review_records()
            .fail_publish(&review_record.id, &publish_claim_id, &error.to_string())
            .await?;
        return Err(error);
    }

    repos
        .challenge_review_records()
        .get(&review_record.id)
        .await?
        .map(review_record_response)
        .ok_or(ServiceError::NotFound)
}

/// Publish a review_record that has already been claimed with `publishing` status.
async fn publish_claimed_challenge_review_record(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    admin: &ChallengeReviewRecordAdmin,
    review_record: &ChallengeReviewRecordResponse,
    publish_claim_id: &ChallengeReviewPublishClaimId,
    repository_path: &RepositoryCheckoutPath,
) -> Result<()> {
    let (manifest, bundle_sha256) =
        validate_review_record_repository(storage, config, review_record, repository_path).await?;
    let approved_bundle_sha256 = review_record
        .approved_bundle_sha256
        .as_ref()
        .ok_or_else(|| ServiceError::Conflict)?;
    if *approved_bundle_sha256 != bundle_sha256 {
        return Err(ServiceError::Validation(
            "challenge review record content changed after approval; validate and approve the review_record again before publishing"
                .to_string(),
        ));
    }
    let proposal_root = repository_path
        .as_path()
        .join(review_record.challenge_path.as_path());
    match manifest.request {
        ChallengeCreationRequestKind::ArchiveChallenge => {
            Repositories::new(pool)
                .challenge_review_records()
                .publish_archive(&persistence::PublishArchiveChallengeReviewRecordInput {
                    review_record_id: review_record.id.clone(),
                    publish_claim_id: publish_claim_id.clone(),
                    challenge_name: manifest.challenge_name.clone(),
                    owner_human_id: review_record.creator_human_id.clone(),
                    audit_event_id: ChallengeReviewAuditEventId::generate(),
                    actor_human_id: admin.human_id.clone(),
                    actor_admin_service_token_id: admin.admin_service_token_id.clone(),
                    actor_display: admin.display.clone(),
                    repository_path: repository_path.to_string(),
                    bundle_sha256,
                })
                .await?;
        }
        ChallengeCreationRequestKind::NewChallenge => {
            let temporary_bundle_path =
                temporary_runtime_bundle_path(config, review_record, publish_claim_id)?;
            let temporary_public_bundle_path =
                temporary_public_runtime_bundle_path(config, review_record, publish_claim_id)?;

            let publish_new_result = prepare_and_publish_new_challenge_review_record(
                pool,
                storage,
                config,
                PublishNewChallengeReviewRecordContext {
                    admin,
                    review_record,
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
struct PublishNewChallengeReviewRecordContext<'a> {
    admin: &'a ChallengeReviewRecordAdmin,
    review_record: &'a ChallengeReviewRecordResponse,
    publish_claim_id: &'a ChallengeReviewPublishClaimId,
    repository_path: &'a RepositoryCheckoutPath,
    proposal_root: &'a Path,
    manifest: &'a ChallengeCreationManifest,
    bundle_sha256: Sha256Digest,
    temporary_bundle_path: &'a Path,
    temporary_public_bundle_path: &'a Path,
}

/// Assemble, validate, promote, and commit a new challenge publish attempt.
async fn prepare_and_publish_new_challenge_review_record(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    ctx: PublishNewChallengeReviewRecordContext<'_>,
) -> Result<()> {
    assemble_runtime_bundle(
        storage,
        config,
        ctx.review_record,
        ctx.proposal_root,
        ctx.manifest,
        ctx.temporary_bundle_path,
    )
    .await?;
    assemble_public_bundle(
        config,
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
        ctx.review_record.id.as_str(),
        ctx.publish_claim_id.as_str(),
    )?;
    let public_bundle_key = challenge_bundle_storage_key(
        "challenge-public-bundles",
        ctx.manifest.challenge_name.as_str(),
        ctx.review_record.id.as_str(),
        ctx.publish_claim_id.as_str(),
    )?;
    let statement_key = StorageKey::try_new(format!(
        "challenge-statements/{}/{}-{}.md",
        ctx.manifest.challenge_name, ctx.review_record.id, ctx.publish_claim_id
    ))?;
    let private_archive_path = config
        .storage_work_root()
        .map_err(storage_error_to_service_error)?
        .join("_tmp")
        .join(format!("bundle-{}.tar", Uuid::new_v4()));
    let public_archive_path = config
        .storage_work_root()
        .map_err(storage_error_to_service_error)?
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
        .await
        .map_err(storage_error_to_service_error)?;
        pack_directory_to_tar(
            ctx.temporary_public_bundle_path,
            &public_archive_path,
            bundle_archive_intent,
        )
        .await
        .map_err(storage_error_to_service_error)?;
        storage
            .put_file(
                &bundle_key,
                &private_archive_path,
                StorageWriteIntent::new(
                    "challenge bundle archive",
                    config.storage.max_bundle_archive_bytes,
                ),
            )
            .await
            .map_err(storage_error_to_service_error)?;
        storage
            .put_file(
                &public_bundle_key,
                &public_archive_path,
                StorageWriteIntent::new(
                    "challenge bundle archive",
                    config.storage.max_bundle_archive_bytes,
                ),
            )
            .await
            .map_err(storage_error_to_service_error)?;
        let statement_bytes =
            tokio::fs::read(ctx.temporary_bundle_path.join("statement.md")).await?;
        storage
            .put(
                &statement_key,
                &statement_bytes,
                StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
            )
            .await
            .map_err(storage_error_to_service_error)?;
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
        .challenge_review_records()
        .publish_new(&persistence::PublishNewChallengeReviewRecordInput {
            review_record_id: ctx.review_record.id.clone(),
            publish_claim_id: ctx.publish_claim_id.clone(),
            challenge_name: ctx.manifest.challenge_name.clone(),
            bundle_key: bundle_key.clone(),
            public_bundle_key: public_bundle_key.clone(),
            statement_key: statement_key.clone(),
            spec,
            title: ctx.manifest.title.clone(),
            summary: ctx.manifest.summary.clone(),
            owner_human_id: ctx.review_record.creator_human_id.clone(),
            audit_event_id: ChallengeReviewAuditEventId::generate(),
            actor_human_id: ctx.admin.human_id.clone(),
            actor_admin_service_token_id: ctx.admin.admin_service_token_id.clone(),
            actor_display: ctx.admin.display.clone(),
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
