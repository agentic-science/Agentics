//! Frontier-CS dev-data seeding for the Compose development stack.

use std::fs;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use agentics_config::Config;
use agentics_contracts::challenge_bundle::{read_challenge_bundle_spec, validate_challenge_bundle};
use agentics_contracts::challenge_creation::read_challenge_creation_manifest;
use agentics_contracts::validation::archive::{ArchiveEnvelopePolicy, extract_zip_bytes_to_dir};
use agentics_domain::models::challenge::{ChallengeBundleSpec, TargetAccelerator};
use agentics_domain::models::challenge_creation::ChallengeCreationManifest;
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{AgentId, EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_domain::storage::StorageKey;
use agentics_persistence::{
    CreateSolutionSubmissionInput, Repositories, SolutionSubmissionQuotaAdmission,
};
use agentics_storage::{Storage, StorageWriteIntent};
use secrecy::ExposeSecret;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::{LocalDemoConfig, LocalDemoError};
use crate::support::ReportLine;

const CHALLENGE_REPOSITORY_ROOT: &str = "challenge-repos/agentics-challenges";
const CHALLENGES_DIR: &str = "challenges";
const TEST_SOLUTIONS_DIR: &str = "test-solutions";
const PRIVATE_BACKUP_PREFIX: &str = "private-bundle-backups";
const LEGACY_BACKUP_BATCH: &str = "frontier-cs-migrations-20260525";
const DEV_SEED_AGENT_ID: &str = "10000000-0000-4000-8000-00000000fc00";
const DEV_SEED_AGENT_DISPLAY_NAME: &str = "Frontier-CS Dev Baseline";
const DEV_SEED_CREDIT_TEXT: &str = "Seeded by agentics-local-demo from test-solutions";
const DEV_SEED_NOTE: &str = "Frontier-CS dev test solution";
const DEV_QUOTA_WINDOW_SECONDS: i64 = 86_400;
const DEV_PER_AGENT_CHALLENGE_LIMIT: i64 = 10_000;
const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;
const TEST_SOLUTION_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
struct FrontierChallenge {
    name: ChallengeName,
    challenge_root: PathBuf,
    manifest: ChallengeCreationManifest,
    spec: ChallengeBundleSpec,
}

#[derive(Debug, Clone, Copy, Default)]
struct SeedStats {
    private_bundles: usize,
    test_solution_submissions: usize,
    skipped_test_solutions: usize,
}

/// Prepare the API startup seed root with only migrated non-GPU Frontier-CS challenges.
pub(super) async fn prepare_challenge_root(
    config: &LocalDemoConfig,
) -> Result<usize, LocalDemoError> {
    let source_root = challenge_repository_root(config.repo_root()).join(CHALLENGES_DIR);
    let destination_root = PathBuf::from(&config.storage_config().storage.challenges_root);
    let challenges = discover_frontier_challenges(&source_root).await?;
    let storage = agentics_storage::build_storage(
        config
            .storage_config()
            .storage_factory_options()
            .map_err(|error| LocalDemoError::StorageInit(error.to_string()))?,
    )
    .await
    .map_err(|error| LocalDemoError::StorageInit(error.to_string()))?;
    replace_dir(&destination_root)?;
    for challenge in &challenges {
        let destination = destination_root.join(challenge.name.as_str());
        copy_dir_rejecting_symlinks(&challenge.challenge_root, &destination)?;
        let Some(bundle_path) = &challenge.manifest.bundle_path else {
            continue;
        };
        let destination_bundle = destination.join(bundle_path.as_path());
        for asset in &challenge.manifest.private_assets {
            if !asset.required {
                continue;
            }
            let asset_key = find_private_asset_key(
                storage.as_ref(),
                &challenge.name,
                asset.asset_name.as_str(),
            )
            .await?;
            let bytes = storage
                .get(
                    &asset_key,
                    StorageWriteIntent::new(
                        "Frontier-CS private asset ZIP",
                        config
                            .storage_config()
                            .quotas
                            .challenge_private_asset_bytes_per_draft,
                    ),
                )
                .await?;
            extract_private_asset_overlay(
                &bytes,
                &destination_bundle,
                asset.asset_name.as_str(),
                config
                    .storage_config()
                    .quotas
                    .challenge_private_asset_bytes_per_draft,
            )
            .await?;
        }
        validate_challenge_bundle(&destination_bundle).await?;
    }
    Ok(challenges.len())
}

/// Seed private runtime bundles and real Frontier-CS test solution submissions.
pub(super) async fn seed_database(
    config: &LocalDemoConfig,
) -> Result<Vec<ReportLine>, LocalDemoError> {
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(config.database_url_secret().expose_secret())
        .await?;
    let storage = agentics_storage::build_storage(
        config
            .storage_config()
            .storage_factory_options()
            .map_err(|error| LocalDemoError::StorageInit(error.to_string()))?,
    )
    .await
    .map_err(|error| LocalDemoError::StorageInit(error.to_string()))?;
    let challenges = discover_frontier_challenges(&PathBuf::from(
        &config.storage_config().storage.challenges_root,
    ))
    .await?;

    ensure_dev_seed_agent(&pool).await?;

    let mut stats = SeedStats::default();
    for challenge in &challenges {
        verify_published_runtime_bundle(&pool, &challenge.name).await?;
        stats.private_bundles = stats.private_bundles.checked_add(1).ok_or_else(|| {
            LocalDemoError::InvalidConfig("private bundle count overflow".to_string())
        })?;

        if seed_test_solution_submission(&pool, storage.as_ref(), config, challenge).await? {
            stats.test_solution_submissions = stats
                .test_solution_submissions
                .checked_add(1)
                .ok_or_else(|| {
                    LocalDemoError::InvalidConfig(
                        "test solution submission count overflow".to_string(),
                    )
                })?;
        } else {
            stats.skipped_test_solutions =
                stats.skipped_test_solutions.checked_add(1).ok_or_else(|| {
                    LocalDemoError::InvalidConfig(
                        "skipped test solution count overflow".to_string(),
                    )
                })?;
        }
    }
    pool.close().await;

    Ok(vec![
        ReportLine::pass(
            "private bundles",
            format!(
                "verified {} prepared Frontier-CS runtime bundles",
                stats.private_bundles
            ),
        ),
        ReportLine::pass(
            "test submissions",
            format!(
                "staged {} official test solution(s); skipped {} without test solution or existing submission",
                stats.test_solution_submissions, stats.skipped_test_solutions
            ),
        ),
    ])
}

#[cfg(test)]
pub(super) async fn upload_test_solution_artifact_for_test(
    storage: &dyn Storage,
    config: &Config,
    solution_root: &Path,
    challenge_name: &str,
) -> Result<StorageKey, LocalDemoError> {
    upload_test_solution_artifact(storage, config, solution_root, challenge_name).await
}

async fn discover_frontier_challenges(
    challenges_root: &Path,
) -> Result<Vec<FrontierChallenge>, LocalDemoError> {
    let mut entries = fs::read_dir(challenges_root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());

    let mut challenges = Vec::new();
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let challenge_root = entry.path();
        let manifest_path = challenge_root.join("agentics.challenge.json");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest = read_challenge_creation_manifest(&challenge_root).await?;
        if !manifest.challenge_name.as_str().contains("frontier-cs") {
            continue;
        }
        let Some(bundle_path) = &manifest.bundle_path else {
            continue;
        };
        let bundle_dir = challenge_root.join(bundle_path.as_path());
        let spec = read_challenge_bundle_spec(&bundle_dir).await?;
        if spec
            .targets
            .iter()
            .any(|target| target.accelerator == TargetAccelerator::Gpu)
        {
            continue;
        }
        challenges.push(FrontierChallenge {
            name: manifest.challenge_name.clone(),
            challenge_root,
            manifest,
            spec,
        });
    }
    Ok(challenges)
}

async fn find_private_asset_key(
    storage: &dyn Storage,
    challenge_name: &ChallengeName,
    asset_name: &str,
) -> Result<StorageKey, LocalDemoError> {
    for candidate in private_asset_key_candidates(challenge_name.as_str(), asset_name)? {
        if storage.exists(&candidate).await? {
            return Ok(candidate);
        }
    }
    Err(LocalDemoError::InvalidConfig(format!(
        "missing private asset backup for `{challenge_name}` asset `{asset_name}`; run the private-bundle restore step first"
    )))
}

fn private_asset_key_candidates(
    challenge_name: &str,
    asset_name: &str,
) -> Result<Vec<StorageKey>, LocalDemoError> {
    [
        format!("{PRIVATE_BACKUP_PREFIX}/{challenge_name}/{asset_name}.zip"),
        format!("{PRIVATE_BACKUP_PREFIX}/{challenge_name}/{challenge_name}-{asset_name}.zip"),
        format!("{PRIVATE_BACKUP_PREFIX}/{challenge_name}/official-runs.zip"),
        format!("{PRIVATE_BACKUP_PREFIX}/{challenge_name}/official-session.zip"),
        format!(
            "{PRIVATE_BACKUP_PREFIX}/{LEGACY_BACKUP_BATCH}/uploaded-assets/{challenge_name}-{asset_name}.zip"
        ),
        format!(
            "{PRIVATE_BACKUP_PREFIX}/{LEGACY_BACKUP_BATCH}/tmp-zips/{challenge_name}-private.zip"
        ),
    ]
    .into_iter()
    .map(|value| {
        StorageKey::try_new(value).map_err(|error| LocalDemoError::InvalidConfig(error.to_string()))
    })
    .collect()
}

async fn extract_private_asset_overlay(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<(), LocalDemoError> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        let policy = ArchiveEnvelopePolicy::new(
            format!("private asset `{asset_name}`"),
            max_uncompressed_bytes,
            MAX_PRIVATE_ASSET_FILE_COUNT,
            max_uncompressed_bytes,
        );
        extract_zip_bytes_to_dir(&bytes, &target_dir, &policy)
    })
    .await
    .map_err(|error| {
        LocalDemoError::InvalidConfig(format!("private asset task failed: {error}"))
    })??;
    Ok(())
}

async fn verify_published_runtime_bundle(
    pool: &PgPool,
    challenge_name: &ChallengeName,
) -> Result<(), LocalDemoError> {
    let bundle_key = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT bundle_key
        FROM challenges
        WHERE challenge_name = $1
        "#,
    )
    .bind(challenge_name.as_str())
    .fetch_optional(pool)
    .await?;
    let Some(Some(bundle_key)) = bundle_key else {
        return Err(LocalDemoError::InvalidConfig(format!(
            "challenge `{challenge_name}` was not published by API startup seeding"
        )));
    };
    if bundle_key.trim().is_empty() {
        return Err(LocalDemoError::InvalidConfig(format!(
            "challenge `{challenge_name}` has an empty runtime bundle key"
        )));
    }
    Ok(())
}

async fn ensure_dev_seed_agent(pool: &PgPool) -> Result<AgentId, LocalDemoError> {
    let agent_id = agent_id(DEV_SEED_AGENT_ID)?;
    sqlx::query(
        r#"
        INSERT INTO agents (
            id, display_name, agent_description, owner, model_info, status, created_at
        )
        VALUES ($1::uuid, $2, $3, $4, $5, 'active', NOW())
        ON CONFLICT (id) DO UPDATE
        SET display_name = EXCLUDED.display_name,
            agent_description = EXCLUDED.agent_description,
            owner = EXCLUDED.owner,
            model_info = EXCLUDED.model_info,
            status = 'active'
        "#,
    )
    .bind(agent_id.as_str())
    .bind(DEV_SEED_AGENT_DISPLAY_NAME)
    .bind("Real baseline submissions loaded from agentics-challenges/test-solutions.")
    .bind("Agentics Dev")
    .bind(serde_json::json!({"profile": "frontier-cs-dev", "source": "test-solutions"}))
    .execute(pool)
    .await?;
    Ok(agent_id)
}

async fn seed_test_solution_submission(
    pool: &PgPool,
    storage: &dyn Storage,
    config: &LocalDemoConfig,
    challenge: &FrontierChallenge,
) -> Result<bool, LocalDemoError> {
    let solution_root = challenge_repository_root(config.repo_root())
        .join(TEST_SOLUTIONS_DIR)
        .join(challenge.name.as_str());
    if !solution_root.is_dir() {
        return Ok(false);
    }
    let Some(target) = challenge.spec.sole_target() else {
        return Ok(false);
    };
    let agent_id = agent_id(DEV_SEED_AGENT_ID)?;
    if existing_test_submission(pool, &agent_id, &challenge.name, target).await? {
        return Ok(false);
    }

    let artifact_key = upload_test_solution_artifact(
        storage,
        config.storage_config(),
        &solution_root,
        challenge.name.as_str(),
    )
    .await?;
    Repositories::new(pool)
        .solution_submissions()
        .create_with_job(&CreateSolutionSubmissionInput {
            solution_submission_id: SolutionSubmissionId::generate(),
            job_id: EvaluationJobId::generate(),
            agent_id,
            challenge_name: challenge.name.clone(),
            target: target.clone(),
            artifact_key,
            note: DEV_SEED_NOTE.to_string(),
            eval_type: ScoringMode::Official,
            explanation: format!(
                "Development smoke solution from test-solutions/{}.",
                challenge.name
            ),
            parent_solution_submission_id: None,
            credit_text: DEV_SEED_CREDIT_TEXT.to_string(),
            quota_admission: SolutionSubmissionQuotaAdmission {
                window_seconds: DEV_QUOTA_WINDOW_SECONDS,
                per_agent_challenge_limit: DEV_PER_AGENT_CHALLENGE_LIMIT,
                challenge_lifetime_limit: None,
                max_active_official_jobs: None,
            },
        })
        .await?;
    Ok(true)
}

async fn existing_test_submission(
    pool: &PgPool,
    agent_id: &AgentId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<bool, LocalDemoError> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM solution_submissions
            WHERE agent_id = $1::uuid
              AND challenge_name = $2
              AND target = $3
              AND note = $4
        )
        "#,
    )
    .bind(agent_id.as_str())
    .bind(challenge_name.as_str())
    .bind(target.as_str())
    .bind(DEV_SEED_NOTE)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

async fn upload_test_solution_artifact(
    storage: &dyn Storage,
    config: &Config,
    solution_root: &Path,
    challenge_name: &str,
) -> Result<StorageKey, LocalDemoError> {
    let artifact_key = StorageKey::try_new(format!(
        "solution-submissions/frontier-dev-seed/{challenge_name}.zip"
    ))
    .map_err(|error| LocalDemoError::InvalidConfig(error.to_string()))?;
    if storage.exists(&artifact_key).await? {
        storage.delete(&artifact_key).await?;
    }
    let archive_path = config.storage_work_root()?.join("_tmp").join(format!(
        "frontier-dev-solution-{challenge_name}-{}.zip",
        uuid::Uuid::new_v4()
    ));
    zip_dir(solution_root, &archive_path)?;
    storage
        .put_file(
            &artifact_key,
            &archive_path,
            StorageWriteIntent::new(
                "Frontier-CS test solution ZIP",
                TEST_SOLUTION_ARTIFACT_BYTES,
            ),
        )
        .await?;
    remove_file_if_exists(&archive_path)?;
    Ok(artifact_key)
}

fn zip_dir(source_dir: &Path, destination: &Path) -> Result<(), LocalDemoError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(destination)?;
    let mut archive = zip::ZipWriter::new(file);
    zip_dir_entries(source_dir, source_dir, &mut archive)?;
    archive.finish()?;
    Ok(())
}

fn zip_dir_entries<W: Write + Seek>(
    root: &Path,
    current: &Path,
    archive: &mut zip::ZipWriter<W>,
) -> Result<(), LocalDemoError> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(LocalDemoError::InvalidConfig(format!(
                "test solution contains symlink {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            zip_dir_entries(root, &path, archive)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|error| {
            LocalDemoError::InvalidConfig(format!("invalid test solution path: {error}"))
        })?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        archive.start_file(relative, zip_file_options(&metadata))?;
        let bytes = fs::read(&path)?;
        archive.write_all(&bytes)?;
    }
    Ok(())
}

#[cfg(unix)]
fn zip_file_options(metadata: &fs::Metadata) -> SimpleFileOptions {
    use std::os::unix::fs::PermissionsExt;

    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn zip_file_options(_metadata: &fs::Metadata) -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

fn challenge_repository_root(repo_root: &Path) -> PathBuf {
    repo_root.join(CHALLENGE_REPOSITORY_ROOT)
}

fn replace_dir(path: &Path) -> Result<(), LocalDemoError> {
    remove_dir_if_exists(path)?;
    fs::create_dir_all(path)?;
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), LocalDemoError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), LocalDemoError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn copy_dir_rejecting_symlinks(source: &Path, destination: &Path) -> Result<(), LocalDemoError> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(LocalDemoError::InvalidConfig(format!(
                "challenge source contains symlink {}",
                source_path.display()
            )));
        }
        if metadata.is_dir() {
            copy_dir_rejecting_symlinks(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn agent_id(value: &str) -> Result<AgentId, LocalDemoError> {
    AgentId::try_new(value).map_err(|error| {
        LocalDemoError::InvalidConfig(format!("invalid Frontier-CS dev seed agent id: {error}"))
    })
}
