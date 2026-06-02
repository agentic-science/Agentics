use std::path::Path;
use std::process::Command;

use agentics_config::Config;
use agentics_contracts::{challenge_bundle, challenge_creation};
use agentics_domain::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeCreationRequestKind, ChallengeReviewRecordResponse,
    ChallengeReviewRecordStatus, ChallengeReviewValidationStatus,
};
use agentics_domain::models::hashes::{GitCommitSha, Sha256Digest};
use agentics_domain::models::ids::{ChallengeReviewAuditEventId, ChallengeReviewPublishClaimId};
use agentics_domain::models::paths::RepositoryCheckoutPath;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{self as persistence, Repositories};
use agentics_storage::Storage;

use super::presentation::review_record_response;
use super::types::ValidateChallengeReviewRecordServiceRequest;
use super::utils::cleanup_runtime_bundle;
use super::{
    CHALLENGE_REVIEW_RECORD_QUOTA_WINDOW_SECONDS, assemble_runtime_bundle,
    temporary_runtime_bundle_path,
};

/// Validate a review_record against a checked-out challenge repository path.
pub async fn validate_challenge_review_record(
    pool: &sqlx::PgPool,
    storage: &dyn Storage,
    config: &Config,
    request: ValidateChallengeReviewRecordServiceRequest,
) -> Result<ChallengeReviewRecordResponse> {
    let ValidateChallengeReviewRecordServiceRequest {
        admin,
        review_record_id,
        body,
    } = request;
    let repos = Repositories::new(pool);
    let review_record = repos
        .challenge_review_records()
        .get(review_record_id.as_str())
        .await?
        .ok_or(ServiceError::NotFound)?;
    let review_record = review_record_response(review_record);
    if !matches!(
        review_record.status,
        ChallengeReviewRecordStatus::PendingReview | ChallengeReviewRecordStatus::Validated
    ) {
        return Err(ServiceError::Conflict);
    }
    let validation_limit = i64::from(config.quotas.challenge_review_record_validations_per_day);
    let repository_path = RepositoryCheckoutPath::from_existing_dir(&body.repository_path)?;
    let validation_record_id =
        agentics_domain::models::ids::ChallengeReviewValidationRecordId::generate();
    repos
        .challenge_review_records()
        .begin_validation(
            &persistence::BeginChallengeReviewRecordValidationInput {
                validation_record_id: validation_record_id.clone(),
                review_record_id: review_record.id.clone(),
                repository_path: repository_path.to_string(),
                manifest_sha256: review_record.manifest_sha256,
            },
            CHALLENGE_REVIEW_RECORD_QUOTA_WINDOW_SECONDS,
            validation_limit,
            config
                .quotas
                .challenge_review_record_validation_timeout_minutes,
        )
        .await?;
    let validation =
        validate_review_record_repository(storage, config, &review_record, &repository_path).await;

    match validation {
        Ok((_, bundle_sha256)) => {
            let message = "challenge review record validation passed".to_string();
            let audit_event = persistence::CreateChallengeReviewRecordAuditEventInput {
                event_id: ChallengeReviewAuditEventId::generate(),
                review_record_id: review_record.id.clone(),
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
                .challenge_review_records()
                .finish_validation(
                    &persistence::FinishChallengeReviewRecordValidationInput {
                        validation_record_id,
                        review_record_id: review_record.id.clone(),
                        status: ChallengeReviewValidationStatus::Passed,
                        message: message.clone(),
                        bundle_sha256: Some(bundle_sha256),
                    },
                    &audit_event,
                )
                .await?;
            let review_record = repos
                .challenge_review_records()
                .get(review_record.id.as_str())
                .await?
                .ok_or(ServiceError::NotFound)?;
            Ok(review_record_response(review_record))
        }
        Err(error) => {
            let message = error.to_string();
            let audit_event = persistence::CreateChallengeReviewRecordAuditEventInput {
                event_id: ChallengeReviewAuditEventId::generate(),
                review_record_id: review_record.id.clone(),
                actor_agent_id: None,
                actor_admin_username: Some(admin.username.clone()),
                action: "draft_validation_failed".to_string(),
                message: message.clone(),
                metadata: serde_json::json!({ "repository_path": repository_path.to_string() }),
            };
            repos
                .challenge_review_records()
                .finish_validation(
                    &persistence::FinishChallengeReviewRecordValidationInput {
                        validation_record_id,
                        review_record_id: review_record.id.clone(),
                        status: ChallengeReviewValidationStatus::Failed,
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

/// Validates the checked-out proposal against the manifest hash recorded at review_record creation.
pub(crate) async fn validate_review_record_repository(
    storage: &dyn Storage,
    config: &Config,
    review_record: &ChallengeReviewRecordResponse,
    repository_path: &RepositoryCheckoutPath,
) -> Result<(ChallengeCreationManifest, Sha256Digest)> {
    ensure_repository_checkout_matches_commit(repository_path, &review_record.commit_sha).await?;
    let proposal_root = repository_path
        .as_path()
        .join(review_record.challenge_path.as_path());
    let manifest =
        challenge_creation::validate_challenge_creation_repository(&proposal_root).await?;
    let manifest_sha256 = challenge_creation::normalized_manifest_sha256(&manifest)?;
    if manifest_sha256 != review_record.manifest_sha256 {
        return Err(ServiceError::Validation(format!(
            "manifest hash mismatch: review_record has {}, repository has {}",
            review_record.manifest_sha256, manifest_sha256
        )));
    }
    if manifest.challenge_name != review_record.challenge_name {
        return Err(ServiceError::Validation(format!(
            "manifest challenge_name mismatch: review_record has {}, repository has {}",
            review_record.challenge_name, manifest.challenge_name
        )));
    }
    let bundle_sha256 = match manifest.request {
        ChallengeCreationRequestKind::ArchiveChallenge => {
            challenge_creation::challenge_review_bundle_sha256(
                &proposal_root,
                &manifest,
                &review_record.private_assets,
            )
            .await?
        }
        ChallengeCreationRequestKind::NewChallenge => {
            validate_and_hash_runtime_bundle(
                storage,
                config,
                review_record,
                &proposal_root,
                &manifest,
            )
            .await?
        }
    };
    Ok((manifest, bundle_sha256))
}

/// Assemble private overlays, validate the runtime bundle, and return its review digest.
async fn validate_and_hash_runtime_bundle(
    storage: &dyn Storage,
    config: &Config,
    review_record: &ChallengeReviewRecordResponse,
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<Sha256Digest> {
    let validation_claim_id = ChallengeReviewPublishClaimId::generate();
    let runtime_bundle_path =
        temporary_runtime_bundle_path(config, review_record, &validation_claim_id)?;
    let validation: Result<Sha256Digest> = async {
        assemble_runtime_bundle(
            storage,
            config,
            review_record,
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
        challenge_creation::challenge_review_runtime_bundle_sha256(&runtime_bundle_path, manifest)
            .await
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
                "repository HEAD commit {} does not match reviewed review_record commit {}",
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
