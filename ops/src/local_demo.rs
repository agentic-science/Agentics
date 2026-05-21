//! Rust-native local demo orchestration.
//!
//! This module oxidizes `scripts/dev/local-demo.sh` as the
//! `agentics-local-demo` binary. It parses dotenv files natively, generates a
//! non-default admin password, manages the local Postgres container through
//! Bollard, runs migrations and demo seeding through `sqlx`, writes demo ZIP
//! artifacts through the `zip` crate, and supervises API/web child processes.
//! `cargo` and `bun` remain direct process boundaries because they are the
//! project toolchains being supervised.
//!
//! The command is idempotent: `up` reuses running processes/containers where
//! safe, `down` tolerates absent processes, and `--purge-data` is guarded by
//! demo-owned path checks. Rollback is limited to process/container cleanup
//! because local demo state is disposable and explicitly owned by the demo
//! runtime roots.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use bollard::Docker;
use bollard::models::{
    ContainerCreateBody, HostConfig, Mount, MountType, PortBinding, VolumeCreateRequest,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
    RemoveVolumeOptionsBuilder, StartContainerOptions,
};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use secrecy::{ExposeSecret, SecretString};
use shared::config::{
    DEFAULT_ADMIN_USERNAME, DEFAULT_API_HOST, ENV_AGENTICS_ADMIN_PASSWORD,
    ENV_AGENTICS_ADMIN_USERNAME, ENV_AGENTICS_API_BASE_URL, ENV_AGENTICS_API_PORT,
    ENV_AGENTICS_WEB_BASE_URL, default_local_api_base_url, default_local_web_base_url,
};
use sqlx::Executor;
use sqlx::postgres::PgPoolOptions;
use tokio::process::Command;
use url::Url;
use uuid::Uuid;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::support::{
    ReportLine, SupportError, env_non_empty, print_reports, require_safe_destructive_path,
    run_with_ctrl_c,
};

const PREFIX: &str = "agentics-demo";
const ENV_DEMO_ENV_FILE: &str = "AGENTICS_DEMO_ENV_FILE";
const ENV_DEMO_RUNTIME_ROOT: &str = "AGENTICS_DEMO_RUNTIME_ROOT";
const ENV_DEMO_API_HOST: &str = "AGENTICS_DEMO_API_HOST";
const ENV_DEMO_WEB_HOST: &str = "AGENTICS_DEMO_WEB_HOST";
const ENV_DEMO_API_PORT: &str = "AGENTICS_DEMO_API_PORT";
const ENV_DEMO_WEB_PORT: &str = "AGENTICS_DEMO_WEB_PORT";
const ENV_DEMO_API_BASE_URL: &str = "AGENTICS_DEMO_API_BASE_URL";
const ENV_DEMO_WEB_BASE_URL: &str = "AGENTICS_DEMO_WEB_BASE_URL";
const ENV_DEMO_DATABASE_NAME: &str = "AGENTICS_DEMO_DATABASE_NAME";
const ENV_DEMO_DATABASE_URL: &str = "AGENTICS_DEMO_DATABASE_URL";
const ENV_DEMO_CORS_ALLOWED_ORIGINS: &str = "AGENTICS_DEMO_CORS_ALLOWED_ORIGINS";
const ENV_DEMO_WEB_ALLOWED_DEV_ORIGINS: &str = "AGENTICS_DEMO_WEB_ALLOWED_DEV_ORIGINS";
const ENV_DEMO_PUBLIC_HOST: &str = "AGENTICS_DEMO_PUBLIC_HOST";
const ENV_POSTGRES_PORT: &str = "AGENTICS_POSTGRES_PORT";
const ENV_STORAGE_ROOT: &str = "AGENTICS_STORAGE_ROOT";
const ENV_CHALLENGES_ROOT: &str = "AGENTICS_CHALLENGES_ROOT";
const ENV_API_HOST: &str = "AGENTICS_API_HOST";
const ENV_WEB_HOST: &str = "AGENTICS_WEB_HOST";
const ENV_WEB_PORT: &str = "AGENTICS_WEB_PORT";
const ENV_CORS_ALLOWED_ORIGINS: &str = "AGENTICS_CORS_ALLOWED_ORIGINS";
const ENV_WEB_ALLOWED_DEV_ORIGINS: &str = "AGENTICS_WEB_ALLOWED_DEV_ORIGINS";
const ENV_WEB_SESSION_COOKIE_SECURE: &str = "AGENTICS_WEB_SESSION_COOKIE_SECURE";
const ENV_DATABASE_URL: &str = "AGENTICS_DATABASE_URL";

const DEFAULT_DEMO_API_PORT: u16 = 13_100;
const DEFAULT_DEMO_WEB_PORT: u16 = 13_001;
const DEFAULT_DEMO_DATABASE_NAME: &str = "agentics_demo";
const POSTGRES_IMAGE: &str = "postgres:16-alpine";
const POSTGRES_CONTAINER: &str = "agentics-local-demo-platform-db";
const POSTGRES_VOLUME: &str = "agentics_platform_db_data";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(180);

/// CLI for local demo orchestration.
#[derive(Debug, Parser)]
#[command(
    about = "Runs the Agentics local demo profile.",
    long_about = "Starts/stops the local Postgres container, API server, and web frontend for visual inspection. Uses native dotenv parsing, Bollard-managed Postgres, sqlx migrations/seeding, generated demo credentials, and Rust-created demo artifacts."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<LocalDemoCommand>,
}

/// Local demo lifecycle command.
#[derive(Debug, Subcommand)]
pub enum LocalDemoCommand {
    /// Start Postgres, migrate/seed the database, and launch API plus web.
    Up {
        /// Bind API and web to 0.0.0.0 for same-network inspection.
        #[arg(long)]
        lan: bool,
    },
    /// Stop API/web and optionally Postgres.
    Down {
        /// Stop and remove the Postgres container.
        #[arg(long)]
        db: bool,
        /// Remove generated demo logs, pid files, artifacts, and DB volume.
        #[arg(long)]
        purge_data: bool,
    },
    /// Re-run demo seeding against an existing database.
    Seed,
    /// Print API/web/process state.
    Status,
    /// Print the current tail of API and web logs.
    Logs,
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
    let command = cli.command.unwrap_or(LocalDemoCommand::Up { lan: false });
    let config = LocalDemoConfig::from_env(matches!(command, LocalDemoCommand::Up { lan: true }))?;
    match command {
        LocalDemoCommand::Up { .. } => up(&config).await,
        LocalDemoCommand::Down { db, purge_data } => {
            down(&config, db || purge_data, purge_data).await
        }
        LocalDemoCommand::Seed => seed_only(&config).await,
        LocalDemoCommand::Status => status(&config).await,
        LocalDemoCommand::Logs => logs(&config).await,
    }
}

async fn up(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    tokio::fs::create_dir_all(&config.runtime_root).await?;
    tokio::fs::create_dir_all(&config.pid_dir).await?;
    tokio::fs::create_dir_all(&config.log_dir).await?;
    ensure_admin_password(config).await?;

    let mut reports = Vec::new();
    reports.extend(install_dependencies(config).await?);
    reports.push(start_postgres(config, false).await?);
    wait_for_database(config).await?;
    stop_named_process(config, DemoProcess::Web).await?;
    stop_named_process(config, DemoProcess::Api).await?;
    reset_database(config).await?;
    run_migrations(config).await?;
    reports.push(start_named_process(config, DemoProcess::Api).await?);
    wait_for_http("API", &config.health_url()?).await?;
    reports.extend(seed_database(config).await?);
    reports.push(start_named_process(config, DemoProcess::Web).await?);
    wait_for_http("web frontend", &config.web_base_url).await?;
    reports.extend(status(config).await?);
    reports.push(ReportLine::pass(
        "open",
        format!(
            "{} and {}/challenges/sample-sum/leaderboard?target=linux-arm64-cpu",
            config.web_base_url, config.web_base_url
        ),
    ));
    Ok(reports)
}

async fn down(
    config: &LocalDemoConfig,
    stop_db: bool,
    purge_data: bool,
) -> Result<Vec<ReportLine>, LocalDemoError> {
    let mut reports = Vec::new();
    reports.push(stop_named_process(config, DemoProcess::Web).await?);
    reports.push(stop_named_process(config, DemoProcess::Api).await?);
    if stop_db {
        reports.push(stop_postgres(purge_data).await?);
    }
    if purge_data {
        purge_demo_files(config).await?;
        reports.push(ReportLine::pass(
            "purge",
            "removed demo-owned runtime and storage paths",
        ));
    }
    Ok(reports)
}

async fn seed_only(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    wait_for_database(config).await?;
    seed_database(config).await
}

async fn status(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    let client = reqwest::Client::new();
    let api = match client.get(config.health_url()?).send().await {
        Ok(response) if response.status().is_success() => {
            ReportLine::pass("API", format!("up at {}", config.api_base_url))
        }
        Ok(response) => ReportLine::fail("API", format!("HTTP {}", response.status())),
        Err(_) => ReportLine::skip("API", "down"),
    };
    let web = match client.get(config.web_base_url.clone()).send().await {
        Ok(response) if response.status().is_success() => {
            ReportLine::pass("web", format!("up at {}", config.web_base_url))
        }
        Ok(response) => ReportLine::fail("web", format!("HTTP {}", response.status())),
        Err(_) => ReportLine::skip("web", "down"),
    };
    let pids = [DemoProcess::Api, DemoProcess::Web]
        .into_iter()
        .filter_map(|process| {
            let pid = read_pid(&process.pid_path(config)).ok()?;
            pid_is_running(pid).then(|| format!("{}={pid}", process.as_str()))
        })
        .collect::<Vec<_>>();
    Ok(vec![
        api,
        web,
        ReportLine::pass(
            "PIDs",
            if pids.is_empty() {
                "none".to_string()
            } else {
                pids.join(", ")
            },
        ),
    ])
}

async fn logs(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    tokio::fs::create_dir_all(&config.log_dir).await?;
    let mut reports = Vec::new();
    for process in [DemoProcess::Api, DemoProcess::Web] {
        let path = process.log_path(config);
        let tail = read_log_tail(&path, 12 * 1024)?;
        reports.push(ReportLine::pass(
            format!("{} log", process.as_str()),
            if tail.trim().is_empty() {
                format!("{} is empty", path.display())
            } else {
                tail
            },
        ));
    }
    Ok(reports)
}

#[derive(Debug, Clone)]
pub struct LocalDemoConfig {
    repo_root: PathBuf,
    runtime_root: PathBuf,
    pid_dir: PathBuf,
    log_dir: PathBuf,
    admin_password_file: PathBuf,
    storage_root: PathBuf,
    api_host: String,
    api_port: u16,
    web_host: String,
    web_port: u16,
    database_name: DemoDatabaseName,
    database_url: SecretString,
    admin_password: SecretString,
    api_base_url: Url,
    web_base_url: Url,
    cors_allowed_origins: String,
    web_allowed_dev_origins: String,
    web_session_cookie_secure: Option<String>,
}

impl LocalDemoConfig {
    fn from_env(lan: bool) -> Result<Self, LocalDemoError> {
        let repo_root = repo_root()?;
        let env_file = env_non_empty(ENV_DEMO_ENV_FILE)
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root.join("deploy/local/agentics.env.example"));
        let file_env = load_dotenv_file(&env_file)?;
        let runtime_root = env_value(ENV_DEMO_RUNTIME_ROOT, &file_env)
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root.join(".agentics-demo"));
        let pid_dir = runtime_root.join("pids");
        let log_dir = runtime_root.join("logs");
        let admin_password_file = runtime_root.join("admin-password");
        let api_host = if lan {
            "0.0.0.0".to_string()
        } else {
            env_non_empty(ENV_DEMO_API_HOST)
                .or_else(|| env_non_empty(ENV_API_HOST))
                .or_else(|| file_env_non_empty(ENV_DEMO_API_HOST, &file_env))
                .unwrap_or_else(|| DEFAULT_API_HOST.to_string())
        };
        let web_host = if lan {
            "0.0.0.0".to_string()
        } else {
            env_non_empty(ENV_DEMO_WEB_HOST)
                .or_else(|| env_non_empty(ENV_WEB_HOST))
                .or_else(|| file_env_non_empty(ENV_DEMO_WEB_HOST, &file_env))
                .unwrap_or_else(|| DEFAULT_API_HOST.to_string())
        };
        let api_port = parse_port(
            ENV_DEMO_API_PORT,
            env_non_empty(ENV_DEMO_API_PORT)
                .or_else(|| env_non_empty(ENV_AGENTICS_API_PORT))
                .or_else(|| file_env_non_empty(ENV_DEMO_API_PORT, &file_env))
                .as_deref(),
            DEFAULT_DEMO_API_PORT,
        )?;
        let web_port = parse_port(
            ENV_DEMO_WEB_PORT,
            env_non_empty(ENV_DEMO_WEB_PORT)
                .or_else(|| env_non_empty(ENV_WEB_PORT))
                .or_else(|| file_env_non_empty(ENV_DEMO_WEB_PORT, &file_env))
                .as_deref(),
            DEFAULT_DEMO_WEB_PORT,
        )?;
        let database_name = DemoDatabaseName::parse(
            &env_value(ENV_DEMO_DATABASE_NAME, &file_env)
                .unwrap_or_else(|| DEFAULT_DEMO_DATABASE_NAME.to_string()),
        )?;
        let postgres_port = parse_port(
            ENV_POSTGRES_PORT,
            env_value(ENV_POSTGRES_PORT, &file_env).as_deref(),
            5432,
        )?;
        let database_url_raw = env_non_empty(ENV_DEMO_DATABASE_URL)
            .or_else(|| file_env_non_empty(ENV_DEMO_DATABASE_URL, &file_env))
            .or_else(|| env_non_empty(ENV_DATABASE_URL))
            .unwrap_or_else(|| {
                format!(
                    "postgres://agentics:agentics@127.0.0.1:{postgres_port}/{}",
                    database_name.as_str()
                )
            });
        let api_base_url = parse_url(
            ENV_AGENTICS_API_BASE_URL,
            &env_non_empty(ENV_DEMO_API_BASE_URL)
                .or_else(|| env_non_empty(ENV_AGENTICS_API_BASE_URL))
                .or_else(|| file_env_non_empty(ENV_DEMO_API_BASE_URL, &file_env))
                .unwrap_or_else(|| default_local_api_base_url("127.0.0.1", api_port)),
        )?;
        let web_base_url = parse_url(
            ENV_AGENTICS_WEB_BASE_URL,
            &env_non_empty(ENV_DEMO_WEB_BASE_URL)
                .or_else(|| env_non_empty(ENV_AGENTICS_WEB_BASE_URL))
                .or_else(|| file_env_non_empty(ENV_DEMO_WEB_BASE_URL, &file_env))
                .unwrap_or_else(|| default_local_web_base_url("127.0.0.1", web_port)),
        )?;
        let public_host = env_value(ENV_DEMO_PUBLIC_HOST, &file_env);
        let cors_allowed_origins = env_value(ENV_DEMO_CORS_ALLOWED_ORIGINS, &file_env)
            .or_else(|| env_value(ENV_CORS_ALLOWED_ORIGINS, &file_env))
            .unwrap_or_else(|| {
                demo_cors_allowed_origins(web_port, public_host.as_deref(), &web_host)
            });
        let web_allowed_dev_origins = env_value(ENV_DEMO_WEB_ALLOWED_DEV_ORIGINS, &file_env)
            .or_else(|| env_value(ENV_WEB_ALLOWED_DEV_ORIGINS, &file_env))
            .unwrap_or_else(|| demo_allowed_dev_origins(public_host.as_deref(), &web_host));
        let storage_root = env_value(ENV_STORAGE_ROOT, &file_env)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("storage"));
        let storage_root = if storage_root.is_absolute() {
            storage_root
        } else {
            repo_root.join(storage_root)
        };
        let admin_password = env_non_empty(ENV_AGENTICS_ADMIN_PASSWORD)
            .filter(|value| !matches!(value.as_str(), "agentics-admin" | "change-me"))
            .map(SecretString::from)
            .unwrap_or_else(|| SecretString::from(generate_demo_admin_password()));
        Ok(Self {
            repo_root,
            runtime_root,
            pid_dir,
            log_dir,
            admin_password_file,
            storage_root,
            api_host,
            api_port,
            web_host: web_host.clone(),
            web_port,
            database_name,
            database_url: SecretString::from(database_url_raw),
            admin_password,
            api_base_url,
            web_base_url,
            cors_allowed_origins,
            web_allowed_dev_origins,
            web_session_cookie_secure: (!host_is_loopback(&web_host)).then(|| "true".to_string()),
        })
    }

    fn health_url(&self) -> Result<Url, LocalDemoError> {
        self.api_base_url
            .join("healthz")
            .map_err(|error| LocalDemoError::InvalidConfig(format!("invalid health URL: {error}")))
    }

    fn admin_database_url(&self) -> Result<SecretString, LocalDemoError> {
        let mut url = Url::parse(self.database_url.expose_secret()).map_err(|error| {
            LocalDemoError::InvalidConfig(format!("invalid database URL: {error}"))
        })?;
        url.set_path("postgres");
        Ok(SecretString::from(url.to_string()))
    }

    fn child_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::from([
            (
                ENV_DATABASE_URL.to_string(),
                self.database_url.expose_secret().to_string(),
            ),
            (ENV_API_HOST.to_string(), self.api_host.clone()),
            (ENV_AGENTICS_API_PORT.to_string(), self.api_port.to_string()),
            (ENV_WEB_HOST.to_string(), self.web_host.clone()),
            (ENV_WEB_PORT.to_string(), self.web_port.to_string()),
            (
                ENV_AGENTICS_API_BASE_URL.to_string(),
                self.api_base_url.to_string(),
            ),
            (
                ENV_AGENTICS_WEB_BASE_URL.to_string(),
                self.web_base_url.to_string(),
            ),
            (
                ENV_CORS_ALLOWED_ORIGINS.to_string(),
                self.cors_allowed_origins.clone(),
            ),
            (
                ENV_WEB_ALLOWED_DEV_ORIGINS.to_string(),
                self.web_allowed_dev_origins.clone(),
            ),
            (
                ENV_STORAGE_ROOT.to_string(),
                self.storage_root.to_string_lossy().to_string(),
            ),
            (
                ENV_CHALLENGES_ROOT.to_string(),
                self.repo_root
                    .join("examples/challenges")
                    .to_string_lossy()
                    .to_string(),
            ),
            (
                ENV_AGENTICS_ADMIN_USERNAME.to_string(),
                DEFAULT_ADMIN_USERNAME.to_string(),
            ),
            (
                ENV_AGENTICS_ADMIN_PASSWORD.to_string(),
                self.admin_password.expose_secret().to_string(),
            ),
        ]);
        if let Some(value) = &self.web_session_cookie_secure {
            env.insert(ENV_WEB_SESSION_COOKIE_SECURE.to_string(), value.clone());
        }
        env
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoDatabaseName(String);

impl DemoDatabaseName {
    fn parse(value: &str) -> Result<Self, LocalDemoError> {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err(LocalDemoError::InvalidConfig(format!(
                "{ENV_DEMO_DATABASE_NAME} must contain only letters, digits, and underscores"
            )));
        }
        Ok(Self(trimmed.to_string()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

async fn install_dependencies(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    require_tool("docker")?;
    require_tool("cargo")?;
    require_tool("bun")?;
    checked_process_in("bun", ["install"], &config.repo_root, HashMap::new()).await?;
    Ok(vec![ReportLine::pass(
        "dependencies",
        "bun install completed",
    )])
}

async fn start_postgres(
    config: &LocalDemoConfig,
    purge_existing: bool,
) -> Result<ReportLine, LocalDemoError> {
    let docker = Docker::connect_with_defaults()?;
    ensure_image(&docker, POSTGRES_IMAGE).await?;
    let _ignored = docker
        .create_volume(VolumeCreateRequest {
            name: Some(POSTGRES_VOLUME.to_string()),
            ..Default::default()
        })
        .await;
    if purge_existing {
        let _ignored = docker
            .remove_container(
                POSTGRES_CONTAINER,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await;
    }
    if docker
        .inspect_container(POSTGRES_CONTAINER, None)
        .await
        .is_err()
    {
        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            "5432/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some(config.postgres_port().to_string()),
            }]),
        );
        let body = ContainerCreateBody {
            image: Some(POSTGRES_IMAGE.to_string()),
            env: Some(vec![
                "POSTGRES_USER=agentics".to_string(),
                "POSTGRES_PASSWORD=agentics".to_string(),
                "POSTGRES_DB=agentics".to_string(),
            ]),
            exposed_ports: Some(vec!["5432/tcp".to_string()]),
            host_config: Some(HostConfig {
                port_bindings: Some(port_bindings),
                mounts: Some(vec![Mount {
                    target: Some("/var/lib/postgresql/data".to_string()),
                    source: Some(POSTGRES_VOLUME.to_string()),
                    typ: Some(MountType::VOLUME),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opts = CreateContainerOptionsBuilder::default()
            .name(POSTGRES_CONTAINER)
            .build();
        docker.create_container(Some(opts), body).await?;
    }
    let _ignored = docker
        .start_container(POSTGRES_CONTAINER, None::<StartContainerOptions>)
        .await;
    Ok(ReportLine::pass("Postgres", POSTGRES_CONTAINER))
}

async fn stop_postgres(purge_data: bool) -> Result<ReportLine, LocalDemoError> {
    let docker = Docker::connect_with_defaults()?;
    let _ignored = docker
        .remove_container(
            POSTGRES_CONTAINER,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    if purge_data {
        let _ignored = docker
            .remove_volume(
                POSTGRES_VOLUME,
                Some(RemoveVolumeOptionsBuilder::default().force(true).build()),
            )
            .await;
        Ok(ReportLine::pass("Postgres", "removed container and volume"))
    } else {
        Ok(ReportLine::pass("Postgres", "removed container"))
    }
}

async fn ensure_image(docker: &Docker, image: &str) -> Result<(), LocalDemoError> {
    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }
    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .build();
    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        item?;
    }
    Ok(())
}

impl LocalDemoConfig {
    fn postgres_port(&self) -> u16 {
        Url::parse(self.database_url.expose_secret())
            .ok()
            .and_then(|url| url.port())
            .unwrap_or(5432)
    }
}

async fn wait_for_database(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    let deadline = deadline_after(Duration::from_secs(60));
    let admin_url = config.admin_database_url()?;
    loop {
        if PgPoolOptions::new()
            .max_connections(1)
            .connect(admin_url.expose_secret())
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

async fn reset_database(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    let admin_url = config.admin_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(admin_url.expose_secret())
        .await?;
    let db = config.database_name.as_str();
    pool.execute(
        format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db}' AND pid <> pg_backend_pid()"
        )
        .as_str(),
    )
    .await?;
    pool.execute(format!("DROP DATABASE IF EXISTS {db}").as_str())
        .await?;
    pool.execute(format!("CREATE DATABASE {db}").as_str())
        .await?;
    Ok(())
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

async fn seed_database(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    write_demo_artifacts(config)?;
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(config.database_url.expose_secret())
        .await?;
    sqlx::query(DEMO_SEED_SQL).execute(&pool).await?;
    Ok(vec![
        ReportLine::pass("demo artifacts", "wrote sample solution ZIPs"),
        ReportLine::pass(
            "demo seed",
            "inserted local-demo service heartbeat evidence",
        ),
    ])
}

fn write_demo_artifacts(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    let artifact_dir = config.storage_root.join("solution-submissions");
    std::fs::create_dir_all(&artifact_dir)?;
    for id in [
        "20000000-0000-4000-8000-000000000001",
        "20000000-0000-4000-8000-000000000002",
        "20000000-0000-4000-8000-000000000003",
        "20000000-0000-4000-8000-000000000101",
        "20000000-0000-4000-8000-000000000102",
        "20000000-0000-4000-8000-000000000103",
    ] {
        write_demo_artifact(&artifact_dir.join(format!("{id}.zip")))?;
    }
    Ok(())
}

fn write_demo_artifact(path: &Path) -> Result<(), LocalDemoError> {
    let file = File::create(path)?;
    let mut archive = zip::ZipWriter::new(file);
    let executable = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755);
    let regular = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    archive.start_file("agentics.solution.json", regular)?;
    archive.write_all(
        br#"{"protocol":"zip_project","protocol_version":1,"note":"Local demo artifact.","commands":{"setup":"setup.sh","run":"run.sh"}}"#,
    )?;
    archive.start_file("README.md", regular)?;
    archive.write_all(b"# Local Demo Submission\n\nGenerated by agentics-local-demo.\n")?;
    archive.start_file("setup.sh", executable)?;
    archive.write_all(b"#!/usr/bin/env sh\nset -eu\necho demo setup\n")?;
    archive.start_file("run.sh", executable)?;
    archive.write_all(b"#!/usr/bin/env sh\nset -eu\npython main.py\n")?;
    archive.start_file("main.py", regular)?;
    archive.write_all(
        b"import json, sys\npayload=json.load(sys.stdin)\nprint(json.dumps({'answer': payload.get('a', 0) + payload.get('b', 0)}))\n",
    )?;
    archive.finish()?;
    Ok(())
}

async fn start_named_process(
    config: &LocalDemoConfig,
    process: DemoProcess,
) -> Result<ReportLine, LocalDemoError> {
    let pid_path = process.pid_path(config);
    if let Ok(pid) = read_pid(&pid_path)
        && pid_is_running(pid)
    {
        return Ok(ReportLine::pass(
            process.as_str(),
            format!("already running with pid {pid}"),
        ));
    }
    tokio::fs::create_dir_all(&config.pid_dir).await?;
    tokio::fs::create_dir_all(&config.log_dir).await?;
    let log_path = process.log_path(config);
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_err = log.try_clone()?;
    let mut command = match process {
        DemoProcess::Api => {
            let mut cmd = Command::new("cargo");
            cmd.args(["run", "-p", "api-server", "--bin", "api"])
                .current_dir(&config.repo_root);
            cmd
        }
        DemoProcess::Web => {
            let mut cmd = Command::new("bun");
            let web_port = config.web_port.to_string();
            cmd.args(["run", "dev", "--", "-H", &config.web_host, "-p", &web_port])
                .current_dir(config.repo_root.join("frontends/web"));
            cmd
        }
    };
    command
        .envs(config.child_env())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    let child = command.spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| LocalDemoError::Process("spawned child has no pid".to_string()))?;
    tokio::fs::write(&pid_path, pid.to_string()).await?;
    Ok(ReportLine::pass(
        process.as_str(),
        format!("started pid {pid}; log {}", log_path.display()),
    ))
}

async fn stop_named_process(
    config: &LocalDemoConfig,
    process: DemoProcess,
) -> Result<ReportLine, LocalDemoError> {
    let pid_path = process.pid_path(config);
    let Ok(pid) = read_pid(&pid_path) else {
        let _ignored = tokio::fs::remove_file(&pid_path).await;
        return Ok(ReportLine::skip(process.as_str(), "not running"));
    };
    if !pid_is_running(pid) {
        let _ignored = tokio::fs::remove_file(&pid_path).await;
        return Ok(ReportLine::skip(process.as_str(), "stale pid removed"));
    }
    terminate_pid(pid).await?;
    let _ignored = tokio::fs::remove_file(&pid_path).await;
    Ok(ReportLine::pass(
        process.as_str(),
        format!("stopped pid {pid}"),
    ))
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

async fn purge_demo_files(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    let allowed = [
        config.repo_root.join(".agentics-demo"),
        config.repo_root.join("storage"),
    ];
    for (label, path) in [
        ("runtime root", &config.runtime_root),
        ("storage root", &config.storage_root),
    ] {
        require_safe_destructive_path(path, label, &allowed)?;
        if path.exists() {
            tokio::fs::remove_dir_all(path).await?;
        }
    }
    Ok(())
}

async fn ensure_admin_password(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    if config.admin_password_file.exists() {
        return Ok(());
    }
    tokio::fs::write(
        &config.admin_password_file,
        config.admin_password.expose_secret(),
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum DemoProcess {
    Api,
    Web,
}

impl DemoProcess {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Web => "web",
        }
    }

    fn pid_path(self, config: &LocalDemoConfig) -> PathBuf {
        config.pid_dir.join(format!("{}.pid", self.as_str()))
    }

    fn log_path(self, config: &LocalDemoConfig) -> PathBuf {
        config.log_dir.join(format!("{}.log", self.as_str()))
    }
}

fn read_pid(path: &Path) -> Result<u32, LocalDemoError> {
    let text = std::fs::read_to_string(path)?;
    text.trim().parse::<u32>().map_err(|error| {
        LocalDemoError::Process(format!("invalid pid file {}: {error}", path.display()))
    })
}

fn pid_is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid.cast_signed()), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _pid = pid;
        false
    }
}

async fn terminate_pid(pid: u32) -> Result<(), LocalDemoError> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let process = Pid::from_raw(pid.cast_signed());
        let _ignored = kill(process, Signal::SIGTERM);
        for _ in 0..20 {
            if !pid_is_running(pid) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let _ignored = kill(process, Signal::SIGKILL);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _pid = pid;
        Err(LocalDemoError::Process(
            "process termination is unsupported on this platform".to_string(),
        ))
    }
}

async fn checked_process_in<const N: usize>(
    program: &str,
    args: [&str; N],
    cwd: &Path,
    env: HashMap<String, String>,
) -> Result<(), LocalDemoError> {
    let output = tokio::time::timeout(PROCESS_TIMEOUT, async {
        Command::new(program)
            .args(args)
            .envs(env)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .output()
            .await
    })
    .await
    .map_err(|_| LocalDemoError::Timeout(program.to_string()))??;
    if output.status.success() {
        Ok(())
    } else {
        Err(LocalDemoError::Process(format!(
            "{program} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

fn read_log_tail(path: &Path, max_bytes: u64) -> Result<String, LocalDemoError> {
    if !path.exists() {
        return Ok(format!("{} is absent", path.display()));
    }
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(std::io::SeekFrom::Start(start))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn deadline_after(duration: Duration) -> tokio::time::Instant {
    tokio::time::Instant::now()
        .checked_add(duration)
        .unwrap_or_else(tokio::time::Instant::now)
}

fn load_dotenv_file(path: &Path) -> Result<HashMap<String, String>, LocalDemoError> {
    if !path.exists() {
        return Err(LocalDemoError::InvalidConfig(format!(
            "missing env file {}",
            path.display()
        )));
    }
    let mut values = HashMap::new();
    for item in dotenvy::from_path_iter(path)? {
        let (key, value) = item?;
        values.insert(key, value);
    }
    Ok(values)
}

fn env_value(name: &str, file_env: &HashMap<String, String>) -> Option<String> {
    env_non_empty(name).or_else(|| file_env_non_empty(name, file_env))
}

fn file_env_non_empty(name: &str, file_env: &HashMap<String, String>) -> Option<String> {
    file_env
        .get(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_port(name: &str, value: Option<&str>, default: u16) -> Result<u16, LocalDemoError> {
    match value {
        Some(value) => value.parse::<u16>().map_err(|error| {
            LocalDemoError::InvalidConfig(format!("invalid {name} value {value:?}: {error}"))
        }),
        None => Ok(default),
    }
}

fn parse_url(name: &str, value: &str) -> Result<Url, LocalDemoError> {
    Url::parse(value)
        .map_err(|error| LocalDemoError::InvalidConfig(format!("invalid {name}: {error}")))
}

fn demo_cors_allowed_origins(web_port: u16, public_host: Option<&str>, web_host: &str) -> String {
    let mut origins = vec![
        format!("http://127.0.0.1:{web_port}"),
        format!("http://localhost:{web_port}"),
    ];
    if !host_is_loopback(web_host)
        && let Some(host) = public_host
        && !host_is_loopback(host)
    {
        origins.push(format!("http://{host}:{web_port}"));
    }
    origins.join(",")
}

fn demo_allowed_dev_origins(public_host: Option<&str>, web_host: &str) -> String {
    let mut origins = vec!["127.0.0.1".to_string(), "localhost".to_string()];
    if !host_is_loopback(web_host)
        && let Some(host) = public_host
        && !host_is_loopback(host)
    {
        origins.push(host.to_string());
    }
    origins.join(",")
}

fn host_is_loopback(host: &str) -> bool {
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

fn generate_demo_admin_password() -> String {
    format!(
        "local-demo-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn repo_root() -> Result<PathBuf, LocalDemoError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| LocalDemoError::InvalidConfig("cannot determine repo root".to_string()))
}

fn require_tool(tool: &str) -> Result<(), LocalDemoError> {
    let Some(path) = std::env::var_os("PATH") else {
        return Err(LocalDemoError::MissingTool(tool.to_string()));
    };
    if std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()) {
        Ok(())
    } else {
        Err(LocalDemoError::MissingTool(tool.to_string()))
    }
}

const DEMO_SEED_SQL: &str = include_str!("local_demo_seed.sql");

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
    Docker(#[from] bollard::errors::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid local demo config: {0}")]
    InvalidConfig(String),
    #[error("missing required tool: {0}")]
    MissingTool(String),
    #[error("{0} timed out")]
    Timeout(String),
    #[error("{0}")]
    Process(String),
    #[error("migration failed: {0}")]
    Migrate(String),
}

#[cfg(test)]
mod tests {
    use super::{DemoDatabaseName, demo_allowed_dev_origins, demo_cors_allowed_origins};

    /// Verifies database names are constrained before SQL identifier use.
    #[test]
    fn database_name_rejects_unsafe_identifier() {
        assert!(DemoDatabaseName::parse("agentics_demo").is_ok());
        assert!(DemoDatabaseName::parse("agentics-demo").is_err());
        assert!(DemoDatabaseName::parse("demo;drop").is_err());
    }

    /// Verifies LAN origins are added only when a public non-loopback host exists.
    #[test]
    fn lan_origin_helpers_are_deterministic() {
        assert_eq!(
            demo_cors_allowed_origins(13001, Some("192.168.1.20"), "0.0.0.0"),
            "http://127.0.0.1:13001,http://localhost:13001,http://192.168.1.20:13001"
        );
        assert_eq!(
            demo_allowed_dev_origins(Some("127.0.0.1"), "0.0.0.0"),
            "127.0.0.1,localhost"
        );
    }
}
