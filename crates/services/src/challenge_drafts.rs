//! GitHub-backed challenge draft lifecycle workflows.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use uuid::Uuid;

use agentics_config::Config;
use agentics_contracts::challenge_bundle;
use agentics_domain::models::challenge_creation::{
    ChallengeCreationManifest, ChallengeDraftResponse, ChallengePrivateAssetKind,
};
use agentics_domain::models::ids::ChallengeDraftPublishClaimId;
use agentics_error::{Result, ServiceError};
use agentics_storage::{Storage, StorageWriteIntent, storage_work_root};

const CHALLENGE_DRAFT_QUOTA_WINDOW_SECONDS: i64 = 24 * 60 * 60;

mod cleanup;
mod create;
mod private_assets;
mod publishing;
mod read;
mod review;
mod types;
mod utils;
mod validation;

pub use cleanup::cleanup_challenge_drafts;
pub use create::create_challenge_draft;
use private_assets::extract_private_asset_overlay;
pub use private_assets::upload_challenge_private_asset;
pub use publishing::publish_challenge_draft;
pub use read::{
    get_challenge_draft, list_admin_challenge_draft_private_assets, list_admin_challenge_drafts,
};
pub use review::{abandon_challenge_draft, approve_challenge_draft, reject_challenge_draft};
pub use types::{
    ChallengeDraftAdmin, ChallengeDraftCreator, CreateChallengeDraftServiceRequest,
    PublishChallengeDraftServiceRequest, ReviewChallengeDraftServiceRequest,
    UploadChallengePrivateAssetServiceRequest, ValidateChallengeDraftServiceRequest,
};
pub use validation::validate_challenge_draft;
pub(super) use validation::validate_draft_repository;

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
