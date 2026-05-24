//! Rust-native local demo orchestration.
//!
//! This module implements the `agentics-local-demo` binary. It parses dotenv
//! files natively, generates a non-default admin password, manages the local
//! Postgres container through Bollard, runs migrations and demo seeding through
//! `sqlx`, writes demo ZIP artifacts through the `zip` crate, and supervises
//! API/web child processes.
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
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use agentics_config::{
    DEFAULT_ADMIN_USERNAME, DEFAULT_API_HOST, ENV_AGENTICS_ADMIN_PASSWORD,
    ENV_AGENTICS_ADMIN_USERNAME, ENV_AGENTICS_API_BASE_URL, ENV_AGENTICS_API_PORT,
    ENV_AGENTICS_WEB_BASE_URL, default_local_api_base_url, default_local_web_base_url,
};
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
use sqlx::Executor;
use sqlx::postgres::PgPoolOptions;
use tokio::process::Command;
use url::Url;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

mod local_demo_config;
use local_demo_config::{
    AdminPasswordSource, DemoDatabaseName, create_secret_file, demo_allowed_dev_origins,
    demo_cors_allowed_origins, detect_lan_host, env_value, file_env_non_empty, host_is_loopback,
    load_dotenv_file, parse_port, parse_url, repo_root, require_tool, resolve_admin_password,
    resolve_demo_database_url, secure_admin_password_file, validate_demo_database_url,
};

mod process;
use process::{
    DemoProcess, pid_is_running, read_log_tail, read_pid, start_named_process, stop_named_process,
};

use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, env_non_empty, print_reports,
    require_safe_destructive_path, run_command, run_with_ctrl_c,
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
const ENV_DEMO_DATABASE_URL_CONFIRM: &str = "AGENTICS_DEMO_DATABASE_URL_CONFIRM";
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
const POSTGRES_VOLUME: &str = "agentics_local_demo_postgres_data";
const DEMO_DOCKER_LABEL_KEY: &str = "ai.agentics.local-demo";
const DEMO_DOCKER_LABEL_VALUE: &str = "true";
const NON_LOOPBACK_DATABASE_CONFIRMATION: &str = "non-loopback-demo-db";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(180);
const DEMO_ARTIFACT_IDS: &[&str] = &[
    "20000000-0000-4000-8000-000000000001",
    "20000000-0000-4000-8000-000000000002",
    "20000000-0000-4000-8000-000000000003",
    "20000000-0000-4000-8000-000000000101",
    "20000000-0000-4000-8000-000000000102",
    "20000000-0000-4000-8000-000000000103",
];
const DEMO_ARTIFACT_FILES: &[DemoArtifactFile] = &[
    DemoArtifactFile {
        path: "agentics.solution.json",
        executable: false,
        contents: br#"{"protocol":"zip_project","protocol_version":1,"note":"Local demo artifact.","commands":{"setup":"setup.sh","run":"run.sh"}}"#,
    },
    DemoArtifactFile {
        path: "README.md",
        executable: false,
        contents: b"# Local Demo Submission\n\nGenerated by agentics-local-demo.\n",
    },
    DemoArtifactFile {
        path: "setup.sh",
        executable: true,
        contents: b"#!/usr/bin/env sh\nset -eu\necho demo setup\n",
    },
    DemoArtifactFile {
        path: "run.sh",
        executable: true,
        contents: b"#!/usr/bin/env sh\nset -eu\npython main.py\n",
    },
    DemoArtifactFile {
        path: "main.py",
        executable: false,
        contents: b"import json, sys\npayload=json.load(sys.stdin)\nprint(json.dumps({'answer': payload.get('a', 0) + payload.get('b', 0)}))\n",
    },
];

struct DemoArtifactFile {
    path: &'static str,
    executable: bool,
    contents: &'static [u8],
}

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
    prepare_runtime(config).await?;
    let mut reports = install_dependencies(config).await?;
    reports.push(ensure_postgres_ready(config).await?);
    restart_local_services(config).await?;
    prepare_database(config).await?;
    reports.extend(start_application(config).await?);
    reports.extend(status(config).await?);
    reports.push(open_demo_report(config));
    Ok(reports)
}

async fn prepare_runtime(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    tokio::fs::create_dir_all(&config.runtime_root).await?;
    tokio::fs::create_dir_all(&config.pid_dir).await?;
    tokio::fs::create_dir_all(&config.log_dir).await?;
    ensure_admin_password(config)
}

async fn ensure_postgres_ready(config: &LocalDemoConfig) -> Result<ReportLine, LocalDemoError> {
    let report = start_postgres(config, false).await?;
    wait_for_database(config).await?;
    Ok(report)
}

async fn restart_local_services(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    stop_named_process(config, DemoProcess::Web).await?;
    stop_named_process(config, DemoProcess::Api).await?;
    Ok(())
}

async fn prepare_database(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    reset_database(config).await?;
    run_migrations(config).await
}

async fn start_application(config: &LocalDemoConfig) -> Result<Vec<ReportLine>, LocalDemoError> {
    let mut reports = Vec::new();
    reports.push(start_named_process(config, DemoProcess::Api).await?);
    wait_for_http("API", &config.health_url()?).await?;
    reports.extend(seed_database(config).await?);
    reports.push(start_named_process(config, DemoProcess::Web).await?);
    wait_for_http("web frontend", &config.web_base_url).await?;
    Ok(reports)
}

fn open_demo_report(config: &LocalDemoConfig) -> ReportLine {
    ReportLine::pass(
        "open",
        format!(
            "{} and {}/challenges/sample-sum/leaderboard?target=linux-arm64-cpu",
            config.web_base_url, config.web_base_url
        ),
    )
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
    admin_password_source: AdminPasswordSource,
    api_base_url: Url,
    web_base_url: Url,
    cors_allowed_origins: String,
    web_allowed_dev_origins: String,
    web_session_cookie_secure: Option<String>,
}

fn resolve_env_file(repo_root: &Path) -> PathBuf {
    env_non_empty(ENV_DEMO_ENV_FILE)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("deploy/local/agentics.env.example"))
}

fn resolve_runtime_root(repo_root: &Path, file_env: &HashMap<String, String>) -> PathBuf {
    env_value(ENV_DEMO_RUNTIME_ROOT, file_env)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join(".agentics-demo"))
}

fn resolve_bind_host(
    lan: bool,
    demo_env_name: &str,
    platform_env_name: &str,
    file_env: &HashMap<String, String>,
) -> String {
    if lan {
        return "0.0.0.0".to_string();
    }
    env_non_empty(demo_env_name)
        .or_else(|| env_non_empty(platform_env_name))
        .or_else(|| file_env_non_empty(demo_env_name, file_env))
        .unwrap_or_else(|| DEFAULT_API_HOST.to_string())
}

fn resolve_demo_port(
    demo_env_name: &str,
    platform_env_name: &str,
    default: u16,
    file_env: &HashMap<String, String>,
) -> Result<u16, LocalDemoError> {
    parse_port(
        demo_env_name,
        env_non_empty(demo_env_name)
            .or_else(|| env_non_empty(platform_env_name))
            .or_else(|| file_env_non_empty(demo_env_name, file_env))
            .as_deref(),
        default,
    )
}

fn resolve_database_name(
    file_env: &HashMap<String, String>,
) -> Result<DemoDatabaseName, LocalDemoError> {
    DemoDatabaseName::parse(
        &env_value(ENV_DEMO_DATABASE_NAME, file_env)
            .unwrap_or_else(|| DEFAULT_DEMO_DATABASE_NAME.to_string()),
    )
}

fn resolve_database_url(
    database_name: &DemoDatabaseName,
    file_env: &HashMap<String, String>,
) -> Result<Url, LocalDemoError> {
    let database_url_raw = resolve_demo_database_url(
        env_non_empty(ENV_DEMO_DATABASE_URL),
        file_env_non_empty(ENV_DEMO_DATABASE_URL, file_env),
    )?;
    validate_demo_database_url(
        &database_url_raw,
        database_name,
        env_value(ENV_DEMO_DATABASE_URL_CONFIRM, file_env).as_deref(),
    )
}

fn resolve_configured_url(
    label: &str,
    demo_env_name: &str,
    platform_env_name: &str,
    file_env: &HashMap<String, String>,
    fallback: String,
) -> Result<Url, LocalDemoError> {
    parse_url(
        label,
        &env_non_empty(demo_env_name)
            .or_else(|| env_non_empty(platform_env_name))
            .or_else(|| file_env_non_empty(demo_env_name, file_env))
            .unwrap_or(fallback),
    )
}

fn resolve_public_host(lan: bool, file_env: &HashMap<String, String>) -> Option<String> {
    env_value(ENV_DEMO_PUBLIC_HOST, file_env).or_else(|| lan.then(detect_lan_host).flatten())
}

fn resolve_cors_allowed_origins(
    web_port: u16,
    public_host: Option<&str>,
    web_host: &str,
    file_env: &HashMap<String, String>,
) -> String {
    env_value(ENV_DEMO_CORS_ALLOWED_ORIGINS, file_env)
        .or_else(|| env_value(ENV_CORS_ALLOWED_ORIGINS, file_env))
        .unwrap_or_else(|| demo_cors_allowed_origins(web_port, public_host, web_host))
}

fn resolve_web_allowed_dev_origins(
    public_host: Option<&str>,
    web_host: &str,
    file_env: &HashMap<String, String>,
) -> String {
    env_value(ENV_DEMO_WEB_ALLOWED_DEV_ORIGINS, file_env)
        .or_else(|| env_value(ENV_WEB_ALLOWED_DEV_ORIGINS, file_env))
        .unwrap_or_else(|| demo_allowed_dev_origins(public_host, web_host))
}

fn resolve_storage_root(repo_root: &Path, file_env: &HashMap<String, String>) -> PathBuf {
    let storage_root = env_value(ENV_STORAGE_ROOT, file_env)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("storage"));
    if storage_root.is_absolute() {
        storage_root
    } else {
        repo_root.join(storage_root)
    }
}

impl LocalDemoConfig {
    fn from_env(lan: bool) -> Result<Self, LocalDemoError> {
        let repo_root = repo_root()?;
        let env_file = resolve_env_file(&repo_root);
        let file_env = load_dotenv_file(&env_file)?;
        let runtime_root = resolve_runtime_root(&repo_root, &file_env);
        let pid_dir = runtime_root.join("pids");
        let log_dir = runtime_root.join("logs");
        let admin_password_file = runtime_root.join("admin-password");
        let api_host = resolve_bind_host(lan, ENV_DEMO_API_HOST, ENV_API_HOST, &file_env);
        let web_host = resolve_bind_host(lan, ENV_DEMO_WEB_HOST, ENV_WEB_HOST, &file_env);
        let api_port = resolve_demo_port(
            ENV_DEMO_API_PORT,
            ENV_AGENTICS_API_PORT,
            DEFAULT_DEMO_API_PORT,
            &file_env,
        )?;
        let web_port = resolve_demo_port(
            ENV_DEMO_WEB_PORT,
            ENV_WEB_PORT,
            DEFAULT_DEMO_WEB_PORT,
            &file_env,
        )?;
        let database_name = resolve_database_name(&file_env)?;
        let database_url = resolve_database_url(&database_name, &file_env)?;
        let api_base_url = resolve_configured_url(
            ENV_AGENTICS_API_BASE_URL,
            ENV_DEMO_API_BASE_URL,
            ENV_AGENTICS_API_BASE_URL,
            &file_env,
            default_local_api_base_url("127.0.0.1", api_port),
        )?;
        let web_base_url = resolve_configured_url(
            ENV_AGENTICS_WEB_BASE_URL,
            ENV_DEMO_WEB_BASE_URL,
            ENV_AGENTICS_WEB_BASE_URL,
            &file_env,
            default_local_web_base_url("127.0.0.1", web_port),
        )?;
        let public_host = resolve_public_host(lan, &file_env);
        let cors_allowed_origins =
            resolve_cors_allowed_origins(web_port, public_host.as_deref(), &web_host, &file_env);
        let web_allowed_dev_origins =
            resolve_web_allowed_dev_origins(public_host.as_deref(), &web_host, &file_env);
        let storage_root = resolve_storage_root(&repo_root, &file_env);
        let (admin_password, admin_password_source) = resolve_admin_password(&admin_password_file)?;
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
            database_url: SecretString::from(database_url.to_string()),
            admin_password,
            admin_password_source,
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
    prepare_postgres_container(&docker, config, purge_existing).await?;
    start_postgres_container(&docker).await?;
    Ok(ReportLine::pass("Postgres", POSTGRES_CONTAINER))
}

async fn prepare_postgres_container(
    docker: &Docker,
    config: &LocalDemoConfig,
    purge_existing: bool,
) -> Result<(), LocalDemoError> {
    ensure_image(docker, POSTGRES_IMAGE).await?;
    ensure_demo_volume(docker).await?;
    remove_existing_postgres_if_requested(docker, purge_existing).await;
    match docker.inspect_container(POSTGRES_CONTAINER, None).await {
        Ok(container) => require_demo_container(&container),
        Err(_) => create_postgres_container(docker, config).await,
    }
}

async fn remove_existing_postgres_if_requested(docker: &Docker, purge_existing: bool) {
    if purge_existing {
        let _ignored = docker
            .remove_container(
                POSTGRES_CONTAINER,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await;
    }
}

async fn create_postgres_container(
    docker: &Docker,
    config: &LocalDemoConfig,
) -> Result<(), LocalDemoError> {
    let opts = CreateContainerOptionsBuilder::default()
        .name(POSTGRES_CONTAINER)
        .build();
    docker
        .create_container(Some(opts), postgres_container_body(config))
        .await?;
    Ok(())
}

fn postgres_container_body(config: &LocalDemoConfig) -> ContainerCreateBody {
    ContainerCreateBody {
        image: Some(POSTGRES_IMAGE.to_string()),
        env: Some(vec![
            "POSTGRES_USER=agentics".to_string(),
            "POSTGRES_PASSWORD=agentics".to_string(),
            "POSTGRES_DB=agentics".to_string(),
        ]),
        exposed_ports: Some(vec!["5432/tcp".to_string()]),
        host_config: Some(HostConfig {
            port_bindings: Some(postgres_port_bindings(config)),
            mounts: Some(vec![Mount {
                target: Some("/var/lib/postgresql/data".to_string()),
                source: Some(POSTGRES_VOLUME.to_string()),
                typ: Some(MountType::VOLUME),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        labels: Some(demo_docker_labels()),
        ..Default::default()
    }
}

fn postgres_port_bindings(config: &LocalDemoConfig) -> HashMap<String, Option<Vec<PortBinding>>> {
    HashMap::from([(
        "5432/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("127.0.0.1".to_string()),
            host_port: Some(config.postgres_port().to_string()),
        }]),
    )])
}

async fn start_postgres_container(docker: &Docker) -> Result<(), LocalDemoError> {
    if let Err(error) = docker
        .start_container(POSTGRES_CONTAINER, None::<StartContainerOptions>)
        .await
    {
        let inspect = docker.inspect_container(POSTGRES_CONTAINER, None).await?;
        let running = inspect
            .state
            .as_ref()
            .and_then(|state| state.running)
            .unwrap_or(false);
        if !running {
            return Err(LocalDemoError::Docker(error));
        }
    }
    Ok(())
}

async fn stop_postgres(purge_data: bool) -> Result<ReportLine, LocalDemoError> {
    let docker = Docker::connect_with_defaults()?;
    if let Ok(container) = docker.inspect_container(POSTGRES_CONTAINER, None).await {
        require_demo_container(&container)?;
        docker
            .remove_container(
                POSTGRES_CONTAINER,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await?;
    }
    if purge_data {
        require_demo_volume_owned(&docker).await?;
        docker
            .remove_volume(
                POSTGRES_VOLUME,
                Some(RemoveVolumeOptionsBuilder::default().force(true).build()),
            )
            .await?;
        Ok(ReportLine::pass("Postgres", "removed container and volume"))
    } else {
        Ok(ReportLine::pass("Postgres", "removed container"))
    }
}

async fn ensure_demo_volume(docker: &Docker) -> Result<(), LocalDemoError> {
    match docker.inspect_volume(POSTGRES_VOLUME).await {
        Ok(volume) => {
            if has_demo_label(&volume.labels) {
                Ok(())
            } else {
                Err(LocalDemoError::InvalidConfig(format!(
                    "refusing to use Docker volume {POSTGRES_VOLUME} without {DEMO_DOCKER_LABEL_KEY}={DEMO_DOCKER_LABEL_VALUE}"
                )))
            }
        }
        Err(_) => {
            docker
                .create_volume(VolumeCreateRequest {
                    name: Some(POSTGRES_VOLUME.to_string()),
                    labels: Some(demo_docker_labels()),
                    ..Default::default()
                })
                .await?;
            Ok(())
        }
    }
}

fn require_demo_volume_record(
    docker_volume: &bollard::models::Volume,
) -> Result<(), LocalDemoError> {
    if has_demo_label(&docker_volume.labels) {
        Ok(())
    } else {
        Err(LocalDemoError::InvalidConfig(format!(
            "refusing to remove Docker volume {POSTGRES_VOLUME} without {DEMO_DOCKER_LABEL_KEY}={DEMO_DOCKER_LABEL_VALUE}"
        )))
    }
}

async fn require_demo_volume_owned(docker: &Docker) -> Result<(), LocalDemoError> {
    let volume = docker.inspect_volume(POSTGRES_VOLUME).await?;
    require_demo_volume_record(&volume)
}

fn require_demo_container(
    container: &bollard::models::ContainerInspectResponse,
) -> Result<(), LocalDemoError> {
    let labels = container
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref());
    if labels.is_some_and(has_demo_label) {
        Ok(())
    } else {
        Err(LocalDemoError::InvalidConfig(format!(
            "refusing to use Docker container {POSTGRES_CONTAINER} without {DEMO_DOCKER_LABEL_KEY}={DEMO_DOCKER_LABEL_VALUE}"
        )))
    }
}

fn demo_docker_labels() -> HashMap<String, String> {
    HashMap::from([(
        DEMO_DOCKER_LABEL_KEY.to_string(),
        DEMO_DOCKER_LABEL_VALUE.to_string(),
    )])
}

fn has_demo_label(labels: &HashMap<String, String>) -> bool {
    labels.get(DEMO_DOCKER_LABEL_KEY).map(String::as_str) == Some(DEMO_DOCKER_LABEL_VALUE)
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
    sqlx::raw_sql(DEMO_SEED_SQL).execute(&pool).await?;
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
    for id in DEMO_ARTIFACT_IDS {
        write_demo_artifact(&artifact_dir.join(format!("{id}.zip")))?;
    }
    Ok(())
}

fn write_demo_artifact(path: &Path) -> Result<(), LocalDemoError> {
    let file = File::create(path)?;
    let mut archive = zip::ZipWriter::new(file);
    for entry in DEMO_ARTIFACT_FILES {
        archive.start_file(entry.path, demo_artifact_file_options(entry.executable))?;
        archive.write_all(entry.contents)?;
    }
    archive.finish()?;
    Ok(())
}

fn demo_artifact_file_options(executable: bool) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(if executable { 0o755 } else { 0o644 })
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

fn ensure_admin_password(config: &LocalDemoConfig) -> Result<(), LocalDemoError> {
    match config.admin_password_source {
        AdminPasswordSource::ExistingFile => {
            secure_admin_password_file(&config.admin_password_file)?
        }
        AdminPasswordSource::Environment => {}
        AdminPasswordSource::Generated => {
            if let Some(parent) = config.admin_password_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            create_secret_file(
                &config.admin_password_file,
                config.admin_password.expose_secret(),
            )?;
        }
    }
    Ok(())
}

async fn checked_process_in<const N: usize>(
    program: &str,
    args: [&str; N],
    cwd: &Path,
    env: HashMap<String, String>,
) -> Result<(), LocalDemoError> {
    let output = tokio::time::timeout(PROCESS_TIMEOUT, async {
        let mut command = Command::new(program);
        command
            .args(args)
            .envs(env)
            .current_dir(cwd)
            .stdin(Stdio::null());
        run_command(command, program, None, DEFAULT_OUTPUT_LIMIT_BYTES).await
    })
    .await
    .map_err(|_| LocalDemoError::Timeout(program.to_string()))??;
    if output.success() {
        Ok(())
    } else {
        Err(LocalDemoError::Process(format!(
            "{program} failed with {:?}: {}",
            output.status,
            output.combined()
        )))
    }
}

fn deadline_after(duration: Duration) -> tokio::time::Instant {
    tokio::time::Instant::now()
        .checked_add(duration)
        .unwrap_or_else(tokio::time::Instant::now)
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
    use std::fs::File;
    use std::io::Read;

    use super::{
        DemoDatabaseName, ENV_DEMO_DATABASE_URL, NON_LOOPBACK_DATABASE_CONFIRMATION,
        demo_allowed_dev_origins, demo_cors_allowed_origins, resolve_demo_database_url,
        validate_demo_database_url, write_demo_artifact,
    };

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

    /// Verifies generated demo ZIPs contain the runnable solution contract.
    #[test]
    fn demo_artifact_zip_contains_solution_files() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("artifact.zip");
        write_demo_artifact(&path).expect("write demo artifact");
        let file = File::open(path).expect("open demo artifact");
        let mut archive = zip::ZipArchive::new(file).expect("read demo artifact");

        let mut manifest = String::new();
        archive
            .by_name("agentics.solution.json")
            .expect("manifest exists")
            .read_to_string(&mut manifest)
            .expect("read manifest");
        assert!(manifest.contains("\"protocol\":\"zip_project\""));
        assert!(archive.by_name("setup.sh").is_ok());
        assert!(archive.by_name("run.sh").is_ok());
        assert!(archive.by_name("main.py").is_ok());
    }
}
