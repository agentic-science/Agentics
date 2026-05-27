//! Startup and maintenance workflows that cross persistence and storage.

use std::path::{Path, PathBuf};

use agentics_config::Config;
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::storage::StorageKey;
use agentics_error::{Result, ServiceError};
use agentics_persistence::{PublishChallengeInput, Repositories};
use agentics_storage::{
    Storage, StorageError, StorageWriteIntent, pack_directory_to_tar, storage_work_root,
};
use sqlx::PgPool;

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
            seed_bundle_dir(pool, config, storage, &bundle_dir).await?;
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
    bundle_dir: &Path,
) -> Result<()> {
    agentics_contracts::challenge_bundle::validate_challenge_bundle(bundle_dir).await?;
    let spec = agentics_contracts::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
    let bundle_digest =
        agentics_contracts::challenge_bundle::challenge_bundle_tree_sha256(bundle_dir).await?;
    let private_bundle_key = bundle_storage_key(
        "challenge-bundles",
        spec.challenge_name.as_str(),
        &bundle_digest.to_hex(),
    )?;
    let public_bundle_dir =
        seeded_public_bundle_dir(config, bundle_dir, &spec, &bundle_digest.to_hex()).await?;
    let public_digest =
        agentics_contracts::challenge_bundle::challenge_bundle_tree_sha256(&public_bundle_dir)
            .await?;
    let public_bundle_key = bundle_storage_key(
        "challenge-public-bundles",
        spec.challenge_name.as_str(),
        &public_digest.to_hex(),
    )?;
    let statement_key = StorageKey::try_new(format!(
        "challenge-statements/{}/{}.md",
        spec.challenge_name,
        bundle_digest.to_hex()
    ))?;

    put_bundle_archive_if_missing(
        storage,
        config,
        &private_bundle_key,
        bundle_dir,
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

    let target = storage_work_root(config)?
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
    if storage.exists(key).await? {
        return Ok(());
    }
    let archive_path = storage_work_root(config)?
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
    .await?;
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
        Err(error) => Err(error.into()),
    }
}

async fn put_statement_if_missing(
    storage: &dyn Storage,
    config: &Config,
    key: &StorageKey,
    statement_path: &Path,
) -> Result<()> {
    if storage.exists(key).await? {
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
        Err(error) => Err(error.into()),
    }
}

fn bundle_storage_key(prefix: &str, challenge_name: &str, digest: &str) -> Result<StorageKey> {
    StorageKey::try_new(format!("{prefix}/{challenge_name}/{digest}.tar")).map_err(Into::into)
}
