//! Startup and maintenance workflows that cross persistence and storage.

use std::path::{Path, PathBuf};

use agentics_config::Config;
use agentics_contracts::challenge_creation::read_challenge_creation_manifest;
use agentics_contracts::validation::archive::{ArchiveEnvelopePolicy, extract_zip_bytes_to_dir};
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::challenge_creation::ChallengeCreationManifest;
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{PublishChallengeInput, Repositories};
use agentics_storage::{Storage, StorageError, StorageWriteIntent, pack_directory_to_tar};
use sqlx::PgPool;

use crate::storage_errors::storage_error_to_service_error;

const PRIVATE_BUNDLE_BACKUP_PREFIX: &str = "private-bundle-backups";
const LEGACY_PRIVATE_BUNDLE_BACKUP_NAME: &str = "official-runs";
const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;

/// Seed or refresh published challenges by scanning a local bundle root.
///
/// Each immediate child directory may contain one or more bundle directories.
/// Directories without `spec.json` are ignored so local notes or partial bundles
/// do not block startup.
pub async fn ensure_challenges_seeded_from_root(
    pool: &PgPool,
    config: &Config,
    storage: &dyn Storage,
    challenges_root: &str,
) -> Result<usize> {
    tokio::fs::create_dir_all(challenges_root).await?;
    let mut entries = tokio::fs::read_dir(challenges_root).await?;
    let mut challenge_dirs = Vec::new();
    let mut synced = 0usize;

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            challenge_dirs.push(entry.path());
        }
    }
    challenge_dirs.sort();

    for challenge_root in challenge_dirs {
        let manifest =
            if tokio::fs::try_exists(challenge_root.join("agentics.challenge.json")).await? {
                Some(read_challenge_creation_manifest(&challenge_root).await?)
            } else {
                None
            };
        let mut bundles = tokio::fs::read_dir(&challenge_root).await?;
        let mut bundle_dirs: Vec<PathBuf> = Vec::new();

        while let Some(bundle_entry) = bundles.next_entry().await? {
            if !bundle_entry.file_type().await?.is_dir() {
                continue;
            }
            let bundle_dir = bundle_entry.path();
            if tokio::fs::try_exists(bundle_dir.join("spec.json")).await? {
                bundle_dirs.push(bundle_dir);
            }
        }
        bundle_dirs.sort();

        for bundle_dir in bundle_dirs {
            seed_bundle_dir(pool, config, storage, manifest.as_ref(), &bundle_dir).await?;
            synced = synced.checked_add(1).ok_or_else(|| {
                ServiceError::Internal("challenge sync count overflow".to_string())
            })?;
        }
    }

    Ok(synced)
}

async fn seed_bundle_dir(
    pool: &PgPool,
    config: &Config,
    storage: &dyn Storage,
    manifest: Option<&ChallengeCreationManifest>,
    bundle_dir: &Path,
) -> Result<()> {
    let spec = agentics_contracts::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
    let public_digest =
        agentics_contracts::challenge_bundle::challenge_bundle_tree_sha256(bundle_dir).await?;
    let private_bundle_dir = seeded_private_bundle_dir(
        config,
        storage,
        manifest,
        bundle_dir,
        &spec,
        &public_digest.to_hex(),
    )
    .await?;
    agentics_contracts::challenge_bundle::validate_challenge_bundle(&private_bundle_dir).await?;
    let private_digest =
        agentics_contracts::challenge_bundle::challenge_bundle_tree_sha256(&private_bundle_dir)
            .await?;
    let private_bundle_key = bundle_storage_key(
        "challenge-bundles",
        spec.challenge_name.as_str(),
        &private_digest.to_hex(),
    )?;
    let public_bundle_dir =
        seeded_public_bundle_dir(config, bundle_dir, &spec, &public_digest.to_hex()).await?;
    let public_bundle_key = bundle_storage_key(
        "challenge-public-bundles",
        spec.challenge_name.as_str(),
        &public_digest.to_hex(),
    )?;
    let statement_key = StorageKey::try_new(format!(
        "challenge-statements/{}/{}.md",
        spec.challenge_name,
        public_digest.to_hex()
    ))?;

    put_bundle_archive_if_missing(
        storage,
        config,
        &private_bundle_key,
        &private_bundle_dir,
        "seeded-private",
    )
    .await?;
    put_bundle_archive_if_missing(
        storage,
        config,
        &public_bundle_key,
        &public_bundle_dir,
        "seeded-public",
    )
    .await?;
    put_statement_if_missing(
        storage,
        config,
        &statement_key,
        &bundle_dir.join("statement.md"),
    )
    .await?;

    let input = PublishChallengeInput {
        challenge_name: &spec.challenge_name,
        bundle_key: &private_bundle_key,
        public_bundle_key: &public_bundle_key,
        statement_key: &statement_key,
        spec: &spec,
        title: &spec.challenge_title,
        summary: &spec.summary,
    };
    let repos = Repositories::new(pool);
    if repos.challenges().publish(&input).await.is_err() {
        repos.challenges().refresh_seeded(&input).await?;
    }

    Ok(())
}

/// Return a private runtime bundle directory, applying restored private overlays when needed.
async fn seeded_private_bundle_dir(
    config: &Config,
    storage: &dyn Storage,
    manifest: Option<&ChallengeCreationManifest>,
    bundle_dir: &Path,
    spec: &ChallengeBundleSpec,
    public_digest: &str,
) -> Result<PathBuf> {
    let Some(manifest) = manifest else {
        if spec.datasets.private_benchmark_enabled {
            return Err(ServiceError::Internal(format!(
                "seeded private-benchmark challenge `{}` needs agentics.challenge.json to locate restored private assets",
                spec.challenge_name
            )));
        }
        agentics_contracts::challenge_bundle::validate_challenge_bundle(bundle_dir).await?;
        return Ok(bundle_dir.to_path_buf());
    };
    let required_assets = manifest
        .private_assets
        .iter()
        .filter(|asset| asset.required)
        .collect::<Vec<_>>();
    if required_assets.is_empty() {
        agentics_contracts::challenge_bundle::validate_challenge_bundle(bundle_dir).await?;
        return Ok(bundle_dir.to_path_buf());
    }

    let target = config
        .storage_work_root()
        .map_err(storage_error_to_service_error)?
        .join("seeded-private-bundles")
        .join(spec.challenge_name.as_str())
        .join(public_digest);
    agentics_contracts::challenge_bundle::copy_challenge_bundle_dir(bundle_dir, &target, true)
        .await?;
    for requirement in required_assets {
        let bytes = read_restored_private_asset(
            storage,
            spec.challenge_name.as_str(),
            requirement.asset_name.as_str(),
            config
                .quotas
                .challenge_private_asset_bytes_per_review_record,
        )
        .await?;
        extract_seed_private_asset_overlay(
            &bytes,
            &target,
            requirement.asset_name.as_str(),
            config
                .quotas
                .challenge_private_asset_bytes_per_review_record,
        )
        .await?;
    }
    validate_seed_private_asset_required_paths(manifest, &target).await?;
    Ok(target)
}

async fn read_restored_private_asset(
    storage: &dyn Storage,
    challenge_name: &str,
    asset_name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let candidate_keys = private_asset_backup_keys(challenge_name, asset_name)?;
    let mut missing = Vec::with_capacity(candidate_keys.len());
    for key in candidate_keys {
        match storage
            .get(
                &key,
                StorageWriteIntent::new("seeded private asset ZIP", max_bytes),
            )
            .await
        {
            Ok(bytes) => return Ok(bytes),
            Err(StorageError::ObjectNotFound(_)) => missing.push(key.to_string()),
            Err(error) => return Err(storage_error_to_service_error(error)),
        }
    }
    Err(ServiceError::Internal(format!(
        "missing restored private asset backup for `{challenge_name}` asset `{asset_name}`; tried {}",
        missing.join(", ")
    )))
}

fn private_asset_backup_keys(challenge_name: &str, asset_name: &str) -> Result<Vec<StorageKey>> {
    let mut keys = vec![private_asset_backup_key(challenge_name, asset_name)?];
    keys.push(private_asset_backup_named_key(
        challenge_name,
        challenge_name,
        asset_name,
    )?);
    if asset_name != LEGACY_PRIVATE_BUNDLE_BACKUP_NAME {
        keys.push(private_asset_backup_key(
            challenge_name,
            LEGACY_PRIVATE_BUNDLE_BACKUP_NAME,
        )?);
        keys.push(private_asset_backup_named_key(
            challenge_name,
            challenge_name,
            LEGACY_PRIVATE_BUNDLE_BACKUP_NAME,
        )?);
    }
    Ok(keys)
}

fn private_asset_backup_key(challenge_name: &str, asset_name: &str) -> Result<StorageKey> {
    StorageKey::try_new(format!(
        "{PRIVATE_BUNDLE_BACKUP_PREFIX}/{challenge_name}/{asset_name}.zip"
    ))
    .map_err(Into::into)
}

fn private_asset_backup_named_key(
    challenge_name: &str,
    file_prefix: &str,
    asset_name: &str,
) -> Result<StorageKey> {
    StorageKey::try_new(format!(
        "{PRIVATE_BUNDLE_BACKUP_PREFIX}/{challenge_name}/{file_prefix}-{asset_name}.zip"
    ))
    .map_err(Into::into)
}

async fn extract_seed_private_asset_overlay(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        let policy = ArchiveEnvelopePolicy::new(
            format!("seeded private asset `{asset_name}`"),
            max_uncompressed_bytes,
            MAX_PRIVATE_ASSET_FILE_COUNT,
            max_uncompressed_bytes,
        );
        extract_zip_bytes_to_dir(&bytes, &target_dir, &policy)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("seeded private asset extraction failed: {e}")))?
}

async fn validate_seed_private_asset_required_paths(
    manifest: &ChallengeCreationManifest,
    runtime_bundle_path: &Path,
) -> Result<()> {
    for requirement in &manifest.private_assets {
        if !requirement.required {
            continue;
        }
        for required_path in &requirement.required_paths {
            let path = runtime_bundle_path.join(required_path.as_path());
            if tokio::fs::try_exists(&path).await? {
                continue;
            }
            return Err(ServiceError::Validation(format!(
                "seeded private asset `{}` did not provide required path `{}`",
                requirement.asset_name, required_path
            )));
        }
    }
    Ok(())
}

/// Return a public-only bundle directory for a seeded challenge.
async fn seeded_public_bundle_dir(
    config: &Config,
    bundle_dir: &Path,
    spec: &ChallengeBundleSpec,
    digest: &str,
) -> Result<PathBuf> {
    if !spec.datasets.private_benchmark_enabled {
        return Ok(bundle_dir.to_path_buf());
    }

    let target = config
        .storage_work_root()
        .map_err(storage_error_to_service_error)?
        .join("seeded-public-bundles")
        .join(spec.challenge_name.as_str())
        .join(digest);
    if let Some(private_benchmark_dir) = &spec.datasets.private_benchmark_dir {
        agentics_contracts::challenge_bundle::copy_challenge_bundle_dir_excluding(
            bundle_dir,
            &target,
            private_benchmark_dir.as_path(),
            true,
        )
        .await?;
    } else {
        agentics_contracts::challenge_bundle::copy_challenge_bundle_dir(bundle_dir, &target, true)
            .await?;
    }

    Ok(target)
}

async fn put_bundle_archive_if_missing(
    storage: &dyn Storage,
    config: &Config,
    key: &StorageKey,
    bundle_dir: &Path,
    label: &str,
) -> Result<()> {
    if storage
        .exists(key)
        .await
        .map_err(storage_error_to_service_error)?
    {
        return Ok(());
    }
    let archive_path = config
        .storage_work_root()
        .map_err(storage_error_to_service_error)?
        .join("_tmp")
        .join(format!("{label}-{}.tar", uuid::Uuid::new_v4()));
    pack_directory_to_tar(
        bundle_dir,
        &archive_path,
        StorageWriteIntent::new(
            "challenge bundle archive",
            config.storage.max_bundle_archive_bytes,
        ),
    )
    .await
    .map_err(storage_error_to_service_error)?;
    let result = storage
        .put_file(
            key,
            &archive_path,
            StorageWriteIntent::new(
                "challenge bundle archive",
                config.storage.max_bundle_archive_bytes,
            ),
        )
        .await;
    let cleanup = tokio::fs::remove_file(&archive_path).await;
    if let Err(error) = cleanup
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    match result {
        Ok(_) => Ok(()),
        Err(StorageError::ObjectConflict(_)) => Ok(()),
        Err(error) => Err(storage_error_to_service_error(error)),
    }
}

async fn put_statement_if_missing(
    storage: &dyn Storage,
    config: &Config,
    key: &StorageKey,
    statement_path: &Path,
) -> Result<()> {
    if storage
        .exists(key)
        .await
        .map_err(storage_error_to_service_error)?
    {
        return Ok(());
    }
    let bytes = tokio::fs::read(statement_path).await?;
    match storage
        .put(
            key,
            &bytes,
            StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
        )
        .await
    {
        Ok(_) | Err(StorageError::ObjectConflict(_)) => Ok(()),
        Err(error) => Err(storage_error_to_service_error(error)),
    }
}

fn bundle_storage_key(prefix: &str, challenge_name: &str, digest: &str) -> Result<StorageKey> {
    StorageKey::try_new(format!("{prefix}/{challenge_name}/{digest}.tar")).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::private_asset_backup_keys;

    /// Verifies seeded private asset lookup supports current and legacy backup names.
    #[test]
    fn private_asset_backup_keys_include_legacy_fallback() {
        let keys =
            private_asset_backup_keys("sample-challenge", "official-session").expect("backup keys");
        let key_strings = keys.iter().map(ToString::to_string).collect::<Vec<_>>();

        assert_eq!(
            key_strings,
            vec![
                "private-bundle-backups/sample-challenge/official-session.zip",
                "private-bundle-backups/sample-challenge/sample-challenge-official-session.zip",
                "private-bundle-backups/sample-challenge/official-runs.zip",
                "private-bundle-backups/sample-challenge/sample-challenge-official-runs.zip",
            ]
        );
    }

    /// Verifies the ordinary official-runs asset is not looked up twice.
    #[test]
    fn private_asset_backup_keys_do_not_duplicate_legacy_name() {
        let keys =
            private_asset_backup_keys("sample-challenge", "official-runs").expect("backup keys");

        assert_eq!(keys.len(), 2);
        assert_eq!(
            keys[0].as_str(),
            "private-bundle-backups/sample-challenge/official-runs.zip"
        );
        assert_eq!(
            keys[1].as_str(),
            "private-bundle-backups/sample-challenge/sample-challenge-official-runs.zip"
        );
    }
}
