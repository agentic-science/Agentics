//! Compose-local database preparation and Frontier-CS dev data seeding.
//!
//! This module implements the `agentics-local-demo` binary used by the
//! containerized development stack. Compose owns service lifetimes, networking,
//! Postgres storage, and log collection. This binary applies database
//! migrations, prepares the curated local challenge root, and stages real
//! migrated Frontier-CS test submissions once the configured API and database
//! are reachable.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use agentics_config::{
    Config, DEFAULT_API_HOST, DEFAULT_API_PORT, ENV_AGENTICS_API_BASE_URL,
    ENV_AGENTICS_S3_ENDPOINT_URL, local_api_base_url,
};
use clap::{Parser, Subcommand};
use secrecy::{ExposeSecret, SecretString};
use sqlx::postgres::PgPoolOptions;
use url::Url;

mod local_demo_config;
use local_demo_config::{
    DemoDatabaseName, RawLocalDemoEnv, env_value, load_dotenv_file, parse_url, repo_root,
    resolve_demo_database_url, validate_demo_database_url,
};

mod local_demo_seed;

use crate::support::{ReportLine, SupportError, print_reports, run_with_ctrl_c};

const PREFIX: &str = "agentics-demo";
const ENV_DEMO_DATABASE_NAME: &str = "AGENTICS_DEMO_DATABASE_NAME";
const ENV_DEMO_DATABASE_URL: &str = "AGENTICS_DEMO_DATABASE_URL";
const ENV_DEMO_DATABASE_URL_CONFIRM: &str = "AGENTICS_DEMO_DATABASE_URL_CONFIRM";
const ENV_DATABASE_URL: &str = "AGENTICS_DATABASE_URL";

const DEFAULT_DEMO_DATABASE_NAME: &str = "agentics_demo";
const NON_LOOPBACK_DATABASE_CONFIRMATION: &str = "non-loopback-demo-db";

/// CLI for Compose-local database preparation.
#[derive(Debug, Parser)]
#[command(
    about = "Prepares the Agentics Compose dev database.",
    long_about = "Applies migrations, prepares the migrated Frontier-CS challenge root, or seeds real test-solution submissions for the containerized development stack. It does not start Postgres, API, worker, web, or Docker runner services; Compose owns those lifetimes."
)]
pub struct Cli {
    #[command(subcommand)]
    command: LocalDemoCommand,
}

/// Compose-local database command.
#[derive(Debug, Subcommand)]
pub enum LocalDemoCommand {
    /// Prepare the filtered migrated Frontier-CS challenge root for API startup seeding.
    Prepare,
    /// Run migrations against the configured demo database.
    Migrate,
    /// Re-run Frontier-CS dev-data seeding against an existing database and API.
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

async fn run(cli: Cli) -> Result<Vec<ReportLine>, LocalDemoError> {
    let config = LocalDemoConfig::from_env()?;
    match cli.command {
        LocalDemoCommand::Prepare => prepare_only(&config).await,
        LocalDemoCommand::Migrate => migrate_only(&config).await,
        LocalDemoCommand::Seed => seed_only(&config).await,
    }
}

async fn prepare_only(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    let prepared = local_demo_seed::prepare_challenge_root(config).await?;
    Ok(vec![ReportLine::pass(
        "challenge root",
        format!("prepared {prepared} migrated non-GPU Frontier-CS challenges"),
    )])
}

async fn migrate_only(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    wait_for_database(config).await?;
    run_migrations(config).await?;
    Ok(vec![ReportLine::pass(
        "migrate",
        "applied database migrations",
    )])
}

async fn seed_only(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    wait_for_database(config).await?;
    wait_for_http("API", &config.health_url()?).await?;
    local_demo_seed::seed_database(config).await
}

#[derive(Debug, Clone)]
pub struct LocalDemoConfig {
    repo_root: PathBuf,
    storage_config: Config,
    database_url: SecretString,
    api_base_url: Url,
}

fn resolve_env_file(repo_root: &Path, process_env: &RawLocalDemoEnv) -> PathBuf {
    env_value(process_env.demo_env_file.as_ref(), None)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("deploy/compose/env/dev.env.example"))
}

fn resolve_database_name(
    process_env: &RawLocalDemoEnv,
    file_env: &RawLocalDemoEnv,
) -> Result<DemoDatabaseName, LocalDemoError> {
    DemoDatabaseName::parse(
        &env_value(
            process_env.demo_database_name.as_ref(),
            file_env.demo_database_name.as_ref(),
        )
        .unwrap_or_else(|| DEFAULT_DEMO_DATABASE_NAME.to_string()),
    )
}

fn resolve_database_url(
    database_name: &DemoDatabaseName,
    process_env: &RawLocalDemoEnv,
    file_env: &RawLocalDemoEnv,
) -> Result<Url, LocalDemoError> {
    let database_url_raw = resolve_demo_database_url(
        env_value(process_env.demo_database_url.as_ref(), None),
        env_value(file_env.demo_database_url.as_ref(), None),
    )?;
    validate_demo_database_url(
        &database_url_raw,
        database_name,
        env_value(
            process_env.demo_database_url_confirm.as_ref(),
            file_env.demo_database_url_confirm.as_ref(),
        )
        .as_deref(),
    )
}

fn resolve_api_base_url(
    process_env: &RawLocalDemoEnv,
    file_env: &RawLocalDemoEnv,
) -> Result<Url, LocalDemoError> {
    parse_url(
        ENV_AGENTICS_API_BASE_URL,
        &env_value(
            process_env.api_base_url.as_ref(),
            file_env.api_base_url.as_ref(),
        )
        .unwrap_or_else(|| local_api_base_url(DEFAULT_API_HOST, DEFAULT_API_PORT)),
    )
}

fn resolve_storage_root(
    repo_root: &Path,
    process_env: &RawLocalDemoEnv,
    file_env: &RawLocalDemoEnv,
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
    process_env: &RawLocalDemoEnv,
    file_env: &RawLocalDemoEnv,
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
    process_env: &RawLocalDemoEnv,
    file_env: &RawLocalDemoEnv,
) -> Result<Config, LocalDemoError> {
    let mut config =
        Config::from_env().map_err(|error| LocalDemoError::InvalidConfig(error.to_string()))?;
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

impl LocalDemoConfig {
    fn from_env() -> Result<Self, LocalDemoError> {
        let repo_root = repo_root()?;
        let process_env = RawLocalDemoEnv::from_process()?;
        let env_file = resolve_env_file(&repo_root, &process_env);
        let file_env_values = load_dotenv_file(&env_file)?;
        let file_env = RawLocalDemoEnv::from_map(&file_env_values)?;
        let database_name = resolve_database_name(&process_env, &file_env)?;
        let database_url = resolve_database_url(&database_name, &process_env, &file_env)?;
        let api_base_url = resolve_api_base_url(&process_env, &file_env)?;
        let storage_config = resolve_storage_config(&repo_root, &process_env, &file_env)?;
        Ok(Self {
            repo_root,
            storage_config,
            database_url: SecretString::from(database_url.to_string()),
            api_base_url,
        })
    }

    fn health_url(&self) -> Result<Url, LocalDemoError> {
        self.api_base_url
            .join("healthz")
            .map_err(|error| LocalDemoError::InvalidConfig(format!("invalid health URL: {error}")))
    }

    fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    fn storage_config(&self) -> &Config {
        &self.storage_config
    }

    fn database_url_secret(&self) -> &SecretString {
        &self.database_url
    }
}

async fn wait_for_database(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
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
            return Err(LocalDemoError::Timeout("Postgres readiness".to_string()));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn run_migrations(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(config.database_url.expose_secret())
        .await?;
    let migrations = config.repo_root.join("backend/migrations");
    let migrator = sqlx::migrate::Migrator::new(migrations)
        .await
        .map_err(|error| LocalDemoError::Migrate(error.to_string()))?;
    migrator
        .run(&pool)
        .await
        .map_err(|error| LocalDemoError::Migrate(error.to_string()))?;
    Ok(())
}

async fn wait_for_http(label: &str, url: &Url) -> Result<(), LocalDemoError> {
    let client = reqwest::Client::new();
    let deadline = deadline_after(Duration::from_secs(180));
    loop {
        if let Ok(response) = client.get(url.clone()).send().await
            && response.status().is_success()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() > deadline {
            return Err(LocalDemoError::Timeout(format!("{label} at {url}")));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn deadline_after(duration: Duration) -> tokio::time::Instant {
    tokio::time::Instant::now()
        .checked_add(duration)
        .unwrap_or_else(tokio::time::Instant::now)
}

/// Local demo orchestration error.
#[derive(Debug, thiserror::Error)]
pub enum LocalDemoError {
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
    #[error("invalid local demo config: {0}")]
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
        DemoDatabaseName, ENV_DEMO_DATABASE_URL, NON_LOOPBACK_DATABASE_CONFIRMATION,
        resolve_demo_database_url, validate_demo_database_url,
    };

    /// Verifies database names are constrained before SQL identifier use.
    #[test]
    fn database_name_rejects_unsafe_identifier() {
        assert!(DemoDatabaseName::parse("agentics_demo").is_ok());
        assert!(DemoDatabaseName::parse("agentics-demo").is_err());
        assert!(DemoDatabaseName::parse("demo;drop").is_err());
    }

    /// Verifies local demo refuses implicit or generic platform database URLs.
    #[test]
    fn demo_database_url_must_be_explicit() {
        let error = resolve_demo_database_url(None, None).expect_err("missing URL should fail");
        assert!(error.to_string().contains(ENV_DEMO_DATABASE_URL));
    }

    /// Verifies demo database URLs must target the demo database on loopback by default.
    #[test]
    fn demo_database_url_is_validated() {
        let name = DemoDatabaseName::parse("agentics_demo").unwrap();
        assert!(
            validate_demo_database_url(
                "postgres://agentics:agentics@127.0.0.1:5432/agentics_demo",
                &name,
                None,
            )
            .is_ok()
        );
        assert!(
            validate_demo_database_url(
                "postgres://agentics:agentics@127.0.0.1:5432/agentics",
                &name,
                None,
            )
            .is_err()
        );
        assert!(
            validate_demo_database_url(
                "postgres://agentics:agentics@db.internal:5432/agentics_demo",
                &name,
                None,
            )
            .is_err()
        );
        assert!(
            validate_demo_database_url(
                "postgres://agentics:agentics@db.internal:5432/agentics_demo",
                &name,
                Some(NON_LOOPBACK_DATABASE_CONFIRMATION),
            )
            .is_ok()
        );
    }

    /// Verifies Frontier-CS test solution seeding writes through the configured storage backend.
    #[tokio::test]
    async fn test_solution_artifacts_are_uploaded_through_storage_backend() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let mut storage_config = Config::from_env().expect("default config should load");
        storage_config.storage.backend = StorageBackend::Local;
        storage_config.storage.root = tempdir.path().join("storage").display().to_string();
        storage_config.storage.work_root = Some(tempdir.path().join("work").display().to_string());
        storage_config.storage.challenges_root =
            tempdir.path().join("challenges").display().to_string();
        let config = super::LocalDemoConfig {
            repo_root: tempdir.path().to_path_buf(),
            storage_config,
            database_url: SecretString::from("postgres://agentics:agentics@localhost/demo"),
            api_base_url: "http://127.0.0.1:3100".parse().expect("valid API URL"),
        };

        let solution_root = tempdir
            .path()
            .join("challenge-repos/agentics-challenges/test-solutions/example-frontier-cs");
        std::fs::create_dir_all(&solution_root).expect("solution dir");
        std::fs::write(
            solution_root.join("agentics.solution.json"),
            r#"{"protocol":"zip_project","protocol_version":1,"commands":{"run":"run.sh"}}"#,
        )
        .expect("manifest");
        std::fs::write(solution_root.join("run.sh"), "#!/usr/bin/env sh\n").expect("run");

        let storage = agentics_storage::build_storage(config.storage_config())
            .await
            .expect("storage");
        let key = super::local_demo_seed::upload_test_solution_artifact_for_test(
            storage.as_ref(),
            config.storage_config(),
            &solution_root,
            "example-frontier-cs",
        )
        .await
        .expect("test solution artifact should upload");

        assert!(
            tempdir.path().join("storage").join(key.as_path()).exists(),
            "test solution artifact key should exist: {key}"
        );
    }
}
