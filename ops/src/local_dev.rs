//! Compose-local database preparation and dev data seeding.
//!
//! This module implements the `agentics-local-dev` binary used by the
//! containerized development stack. Compose owns service lifetimes, networking,
//! Postgres storage, and log collection. This binary applies database
//! migrations, prepares the curated local development challenge root, and
//! stages baseline dev test submissions once the configured API and database
//! are reachable.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use agentics_config::{
    Config, DEFAULT_API_HOST, DEFAULT_API_PORT, ENV_AGENTICS_API_BASE_URL,
    ENV_AGENTICS_S3_ENDPOINT_URL, EnvPolicyReport, EnvServiceRole, local_api_base_url,
};
use clap::{Parser, Subcommand};
use secrecy::{ExposeSecret, SecretString};
use sqlx::postgres::PgPoolOptions;
use url::Url;

mod local_dev_config;
use local_dev_config::{
    LocalDevDatabaseName, RawLocalDevEnv, env_value, load_dotenv_file, parse_url, repo_root,
    resolve_local_dev_database_url, validate_local_dev_database_url,
};

mod local_dev_seed;

use crate::support::{ReportLine, SupportError, print_reports, run_with_ctrl_c};

const PREFIX: &str = "agentics-local-dev";
const ENV_LOCAL_DEV_DATABASE_NAME: &str = "AGENTICS_LOCAL_DEV_DATABASE_NAME";
const ENV_LOCAL_DEV_DATABASE_URL: &str = "AGENTICS_LOCAL_DEV_DATABASE_URL";
const ENV_LOCAL_DEV_DATABASE_URL_CONFIRM: &str = "AGENTICS_LOCAL_DEV_DATABASE_URL_CONFIRM";
const ENV_DATABASE_URL: &str = "AGENTICS_DATABASE_URL";

const DEFAULT_LOCAL_DEV_DATABASE_NAME: &str = "agentics_dev";
const NON_LOOPBACK_DATABASE_CONFIRMATION: &str = "non-loopback-local-dev-db";

/// CLI for Compose-local database preparation.
#[derive(Debug, Parser)]
#[command(
    about = "Prepares the Agentics Compose dev database.",
    long_about = "Applies migrations, prepares the local development challenge root, or seeds baseline test-solution submissions for the containerized development stack. It does not start Postgres, API, worker, web, or Docker runner services; Compose owns those lifetimes."
)]
pub struct Cli {
    #[command(subcommand)]
    command: LocalDevCommand,
}

/// Compose-local database command.
#[derive(Debug, Subcommand)]
pub enum LocalDevCommand {
    /// Prepare the configured local development challenge root for API startup seeding.
    Prepare,
    /// Run migrations against the configured dev database.
    Migrate,
    /// Re-run dev-data seeding against an existing database and API.
    Seed,
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> Result<Vec<ReportLine>, LocalDevError> {
    let config = LocalDevConfig::from_env()?;
    match cli.command {
        LocalDevCommand::Prepare => prepare_only(&config).await,
        LocalDevCommand::Migrate => migrate_only(&config).await,
        LocalDevCommand::Seed => seed_only(&config).await,
    }
}

async fn prepare_only(config: &LocalDevConfig) -> Result<Vec<ReportLine>, LocalDevError> {
    let report = local_dev_seed::prepare_challenge_root(config).await?;
    Ok(vec![
        ReportLine::pass(
            "challenge root",
            format!("prepared {} local dev challenge(s)", report.prepared),
        ),
        ReportLine::pass(
            "challenge discovery",
            format!(
                "skipped {} GPU challenge(s) and {} challenge(s) requiring private assets",
                report.skipped_gpu, report.skipped_private_assets
            ),
        ),
    ])
}

async fn migrate_only(config: &LocalDevConfig) -> Result<Vec<ReportLine>, LocalDevError> {
    wait_for_database(config).await?;
    run_migrations(config).await?;
    Ok(vec![ReportLine::pass(
        "migrate",
        "applied database migrations",
    )])
}

async fn seed_only(config: &LocalDevConfig) -> Result<Vec<ReportLine>, LocalDevError> {
    wait_for_database(config).await?;
    wait_for_http("API", &config.health_url()?).await?;
    local_dev_seed::seed_database(config).await
}

#[derive(Debug, Clone)]
pub struct LocalDevConfig {
    repo_root: PathBuf,
    storage_config: Config,
    database_url: SecretString,
    api_base_url: Url,
    challenge_source_root: PathBuf,
    test_solutions_root: PathBuf,
}

fn resolve_env_file(repo_root: &Path, process_env: &RawLocalDevEnv) -> PathBuf {
    env_value(process_env.local_dev_env_file.as_ref(), None)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("deploy/compose/env/dev.env.example"))
}

fn resolve_database_name(
    process_env: &RawLocalDevEnv,
    file_env: &RawLocalDevEnv,
) -> Result<LocalDevDatabaseName, LocalDevError> {
    LocalDevDatabaseName::parse(
        &env_value(
            process_env.local_dev_database_name.as_ref(),
            file_env.local_dev_database_name.as_ref(),
        )
        .unwrap_or_else(|| DEFAULT_LOCAL_DEV_DATABASE_NAME.to_string()),
    )
}

fn resolve_database_url(
    database_name: &LocalDevDatabaseName,
    process_env: &RawLocalDevEnv,
    file_env: &RawLocalDevEnv,
) -> Result<Url, LocalDevError> {
    let database_url_raw = resolve_local_dev_database_url(
        env_value(process_env.local_dev_database_url.as_ref(), None),
        env_value(file_env.local_dev_database_url.as_ref(), None),
    )?;
    validate_local_dev_database_url(
        &database_url_raw,
        database_name,
        env_value(
            process_env.local_dev_database_url_confirm.as_ref(),
            file_env.local_dev_database_url_confirm.as_ref(),
        )
        .as_deref(),
    )
}

fn resolve_api_base_url(
    process_env: &RawLocalDevEnv,
    file_env: &RawLocalDevEnv,
) -> Result<Url, LocalDevError> {
    parse_url(
        ENV_AGENTICS_API_BASE_URL,
        &env_value(
            process_env.api_base_url.as_ref(),
            file_env.api_base_url.as_ref(),
        )
        .unwrap_or_else(|| local_api_base_url(DEFAULT_API_HOST, DEFAULT_API_PORT)),
    )
}

fn resolve_repo_path(
    repo_root: &Path,
    process_value: Option<&String>,
    file_value: Option<&String>,
    default_relative: &str,
) -> PathBuf {
    let path = env_value(process_value, file_value)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_relative));
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn resolve_storage_root(
    repo_root: &Path,
    process_env: &RawLocalDevEnv,
    file_env: &RawLocalDevEnv,
) -> PathBuf {
    let storage_root = env_value(
        process_env.storage_root.as_ref(),
        file_env.storage_root.as_ref(),
    )
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from(agentics_config::DEFAULT_STORAGE_ROOT));
    if storage_root.is_absolute() {
        storage_root
    } else {
        repo_root.join(storage_root)
    }
}

fn resolve_storage_work_root(
    repo_root: &Path,
    process_env: &RawLocalDevEnv,
    file_env: &RawLocalDevEnv,
) -> Option<String> {
    env_value(
        process_env.storage_work_root.as_ref(),
        file_env.storage_work_root.as_ref(),
    )
    .map(|value| {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        }
        .to_string_lossy()
        .to_string()
    })
}

fn resolve_storage_config(
    repo_root: &Path,
    process_env: &RawLocalDevEnv,
    file_env: &RawLocalDevEnv,
) -> Result<Config, LocalDevError> {
    let mut config =
        Config::from_env().map_err(|error| LocalDevError::InvalidConfig(error.to_string()))?;
    config.storage.root = resolve_storage_root(repo_root, process_env, file_env)
        .to_string_lossy()
        .to_string();
    config.storage.backend = process_env
        .storage_backend
        .or(file_env.storage_backend)
        .unwrap_or(config.storage.backend);
    config.storage.work_root =
        resolve_storage_work_root(repo_root, process_env, file_env).or(config.storage.work_root);
    config.storage.s3_bucket =
        env_value(process_env.s3_bucket.as_ref(), file_env.s3_bucket.as_ref())
            .or(config.storage.s3_bucket);
    config.storage.s3_prefix =
        env_value(process_env.s3_prefix.as_ref(), file_env.s3_prefix.as_ref())
            .or(config.storage.s3_prefix);
    config.storage.s3_region =
        env_value(process_env.s3_region.as_ref(), file_env.s3_region.as_ref())
            .unwrap_or(config.storage.s3_region);
    config.storage.s3_endpoint_url = env_value(
        process_env.s3_endpoint_url.as_ref(),
        file_env.s3_endpoint_url.as_ref(),
    )
    .map(|value| parse_url(ENV_AGENTICS_S3_ENDPOINT_URL, &value))
    .transpose()?
    .or(config.storage.s3_endpoint_url);
    config.storage.s3_force_path_style = process_env
        .s3_force_path_style
        .or(file_env.s3_force_path_style)
        .unwrap_or(config.storage.s3_force_path_style);
    Ok(config)
}

impl LocalDevConfig {
    fn from_env() -> Result<Self, LocalDevError> {
        let repo_root = repo_root()?;
        let process_env = RawLocalDevEnv::from_process()?;
        let env_file = resolve_env_file(&repo_root, &process_env);
        let file_env_values = load_dotenv_file(&env_file)?;
        let mut policy_env = file_env_values.clone();
        policy_env.extend(std::env::vars());
        let env_report =
            agentics_config::validate_env_policy(&policy_env, EnvServiceRole::LocalDev)
                .map_err(|error| LocalDevError::InvalidConfig(error.to_string()))?;
        print_env_policy_warnings(&env_report);
        let file_env = RawLocalDevEnv::from_map(&file_env_values)?;
        let database_name = resolve_database_name(&process_env, &file_env)?;
        let database_url = resolve_database_url(&database_name, &process_env, &file_env)?;
        let api_base_url = resolve_api_base_url(&process_env, &file_env)?;
        let storage_config = resolve_storage_config(&repo_root, &process_env, &file_env)?;
        let challenge_source_root = resolve_repo_path(
            &repo_root,
            process_env.local_dev_challenge_source_root.as_ref(),
            file_env.local_dev_challenge_source_root.as_ref(),
            "challenge-repos/agentics-challenges/dev/challenges",
        );
        let test_solutions_root = resolve_repo_path(
            &repo_root,
            process_env.local_dev_test_solutions_root.as_ref(),
            file_env.local_dev_test_solutions_root.as_ref(),
            "challenge-repos/agentics-challenges/dev/test-solutions",
        );
        Ok(Self {
            repo_root,
            storage_config,
            database_url: SecretString::from(database_url.to_string()),
            api_base_url,
            challenge_source_root,
            test_solutions_root,
        })
    }

    fn health_url(&self) -> Result<Url, LocalDevError> {
        self.api_base_url
            .join("healthz")
            .map_err(|error| LocalDevError::InvalidConfig(format!("invalid health URL: {error}")))
    }

    fn storage_config(&self) -> &Config {
        &self.storage_config
    }

    fn database_url_secret(&self) -> &SecretString {
        &self.database_url
    }

    fn challenge_source_root(&self) -> &Path {
        &self.challenge_source_root
    }

    fn test_solutions_root(&self) -> &Path {
        &self.test_solutions_root
    }
}

fn print_env_policy_warnings(report: &EnvPolicyReport) {
    for warning in &report.warnings {
        eprintln!("[{PREFIX}] WARN env {}: {}", warning.name, warning.message);
    }
}

async fn wait_for_database(config: &LocalDevConfig) -> Result<(), LocalDevError> {
    let deadline = deadline_after(Duration::from_secs(60));
    loop {
        if PgPoolOptions::new()
            .max_connections(1)
            .connect(config.database_url.expose_secret())
            .await
            .is_ok()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() > deadline {
            return Err(LocalDevError::Timeout("Postgres readiness".to_string()));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn run_migrations(config: &LocalDevConfig) -> Result<(), LocalDevError> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(config.database_url.expose_secret())
        .await?;
    let migrations = config.repo_root.join("backend/migrations");
    let migrator = sqlx::migrate::Migrator::new(migrations)
        .await
        .map_err(|error| LocalDevError::Migrate(error.to_string()))?;
    migrator
        .run(&pool)
        .await
        .map_err(|error| LocalDevError::Migrate(error.to_string()))?;
    Ok(())
}

async fn wait_for_http(label: &str, url: &Url) -> Result<(), LocalDevError> {
    let client = reqwest::Client::new();
    let deadline = deadline_after(Duration::from_secs(180));
    loop {
        if let Ok(response) = client.get(url.clone()).send().await
            && response.status().is_success()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() > deadline {
            return Err(LocalDevError::Timeout(format!("{label} at {url}")));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn deadline_after(duration: Duration) -> tokio::time::Instant {
    tokio::time::Instant::now()
        .checked_add(duration)
        .unwrap_or_else(tokio::time::Instant::now)
}

/// Local dev orchestration error.
#[derive(Debug, thiserror::Error)]
pub enum LocalDevError {
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Dotenv(#[from] dotenvy::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Service(#[from] agentics_error::ServiceError),
    #[error(transparent)]
    Storage(#[from] agentics_storage::StorageError),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error("storage initialization failed: {0}")]
    StorageInit(String),
    #[error("invalid local dev config: {0}")]
    InvalidConfig(String),
    #[error("{0} timed out")]
    Timeout(String),
    #[error("migration failed: {0}")]
    Migrate(String),
}

#[cfg(test)]
mod tests {
    use agentics_config::{Config, StorageBackend};
    use secrecy::SecretString;

    use super::{
        ENV_LOCAL_DEV_DATABASE_URL, LocalDevDatabaseName, NON_LOOPBACK_DATABASE_CONFIRMATION,
        RawLocalDevEnv, resolve_local_dev_database_url, resolve_repo_path,
        validate_local_dev_database_url,
    };

    /// Verifies database names are constrained before SQL identifier use.
    #[test]
    fn database_name_rejects_unsafe_identifier() {
        assert!(LocalDevDatabaseName::parse("agentics_dev").is_ok());
        assert!(LocalDevDatabaseName::parse("agentics-local-dev").is_err());
        assert!(LocalDevDatabaseName::parse("dev;drop").is_err());
    }

    /// Verifies local dev refuses implicit or generic platform database URLs.
    #[test]
    fn local_dev_database_url_must_be_explicit() {
        let error =
            resolve_local_dev_database_url(None, None).expect_err("missing URL should fail");
        assert!(error.to_string().contains(ENV_LOCAL_DEV_DATABASE_URL));
    }

    /// Verifies dev database URLs must target the dev database on loopback by default.
    #[test]
    fn local_dev_database_url_is_validated() {
        let name = LocalDevDatabaseName::parse("agentics_dev").unwrap();
        assert!(
            validate_local_dev_database_url(
                "postgres://agentics:agentics@127.0.0.1:5432/agentics_dev",
                &name,
                None,
            )
            .is_ok()
        );
        assert!(
            validate_local_dev_database_url(
                "postgres://agentics:agentics@127.0.0.1:5432/agentics",
                &name,
                None,
            )
            .is_err()
        );
        assert!(
            validate_local_dev_database_url(
                "postgres://agentics:agentics@db.internal:5432/agentics_dev",
                &name,
                None,
            )
            .is_err()
        );
        assert!(
            validate_local_dev_database_url(
                "postgres://agentics:agentics@db.internal:5432/agentics_dev",
                &name,
                Some(NON_LOOPBACK_DATABASE_CONFIRMATION),
            )
            .is_ok()
        );
    }

    /// Verifies default local-dev source roots point at the dev challenge catalog.
    #[test]
    fn local_dev_source_roots_default_to_dev_catalog() {
        let repo = std::path::Path::new("/repo");
        let empty = RawLocalDevEnv::default();

        assert_eq!(
            resolve_repo_path(
                repo,
                empty.local_dev_challenge_source_root.as_ref(),
                None,
                "challenge-repos/agentics-challenges/dev/challenges",
            ),
            repo.join("challenge-repos/agentics-challenges/dev/challenges")
        );
        assert_eq!(
            resolve_repo_path(
                repo,
                empty.local_dev_test_solutions_root.as_ref(),
                None,
                "challenge-repos/agentics-challenges/dev/test-solutions",
            ),
            repo.join("challenge-repos/agentics-challenges/dev/test-solutions")
        );
    }

    /// Verifies dev test solution seeding writes through the configured storage backend.
    #[tokio::test]
    async fn test_solution_artifacts_are_uploaded_through_storage_backend() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let mut storage_config = Config::from_env().expect("default config should load");
        storage_config.storage.backend = StorageBackend::Local;
        storage_config.storage.root = tempdir.path().join("storage").display().to_string();
        storage_config.storage.work_root = Some(tempdir.path().join("work").display().to_string());
        storage_config.storage.challenges_root =
            tempdir.path().join("challenges").display().to_string();
        let config = super::LocalDevConfig {
            repo_root: tempdir.path().to_path_buf(),
            storage_config,
            database_url: SecretString::from("postgres://agentics:agentics@localhost/dev"),
            api_base_url: "http://127.0.0.1:3100".parse().expect("valid API URL"),
            challenge_source_root: tempdir.path().join("dev/challenges"),
            test_solutions_root: tempdir.path().join("dev/test-solutions"),
        };

        let solution_root = tempdir
            .path()
            .join("dev/test-solutions/example-dev-challenge");
        std::fs::create_dir_all(&solution_root).expect("solution dir");
        std::fs::write(
            solution_root.join("agentics.solution.json"),
            r#"{"protocol":"zip_project","protocol_version":1,"commands":{"run":"run.sh"}}"#,
        )
        .expect("manifest");
        std::fs::write(solution_root.join("run.sh"), "#!/usr/bin/env sh\n").expect("run");

        let storage = agentics_storage::build_storage(
            config
                .storage_config()
                .storage_factory_options()
                .expect("valid storage options"),
        )
        .await
        .expect("storage");
        let key = super::local_dev_seed::upload_test_solution_artifact_for_test(
            storage.as_ref(),
            config.storage_config(),
            &solution_root,
            "example-dev-challenge",
        )
        .await
        .expect("test solution artifact should upload");

        assert!(
            tempdir.path().join("storage").join(key.as_path()).exists(),
            "test solution artifact key should exist: {key}"
        );
    }
}
