//! Production Docker Compose wrapper.
//!
//! Compose owns long-lived platform services, while the worker creates runner
//! containers as host-level siblings through the Docker socket. This command
//! keeps those boundaries explicit: ordinary service operations are delegated to
//! `docker compose` with typed arguments, and runner cleanup uses the Docker API
//! with exact Agentics runner labels and namespace filtering.
//!
//! Dry-run modes are non-mutating. `down --runner keep --dry-run` only reports
//! Compose services that would be stopped. `down --runner clean --dry-run`
//! reports both the Compose services and matching runner containers, without
//! stopping services or removing runners.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use agentics_config::RunnerNamespace;
use agentics_runner::RUNNER_SCOPE_HOSTED_WORKER;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use tokio::process::Command;

use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, print_reports, run_command,
    run_with_ctrl_c,
};

mod runner_cleanup;
mod runner_docker;

use runner_cleanup::clean_runners;
use runner_docker::{runner_docker_down, runner_docker_up};

const PREFIX: &str = "agentics-compose-prod";
const ENV_PREFIX: &str = "AGENTICS_";
const DEFAULT_PROJECT: &str = "agentics-prod";
const DEFAULT_ENV_FILE: &str = "deploy/compose/env/prod.env";
const DEFAULT_PRIVATE_BUNDLE_BACKUP_CONTAINER: &str = "agentics-rustfs-private-backup";
const WORKER_SERVICES: &[&str] = &["worker-cpu", "worker-gpu"];
const PROD_SERVICES: &[&str] = &[
    "postgres",
    "rustfs",
    "migrate",
    "api",
    "worker-cpu",
    "worker-gpu",
    "web",
    "check",
];

/// CLI for production Compose operations.
#[derive(Debug, Parser)]
#[command(
    about = "Operate the Agentics production Docker Compose stack.",
    long_about = "Builds, starts, stops, checks, and inspects the production Compose stack. Service operations call docker compose directly. Runner cleanup uses Docker labels and requires an explicit --runner clean choice on down."
)]
pub struct Cli {
    /// Production env file passed to Docker Compose.
    #[arg(long)]
    env_file: Option<PathBuf>,
    /// Compose project name.
    #[arg(long)]
    project: Option<String>,
    #[command(subcommand)]
    command: ProdCommand,
}

/// Production Compose command.
#[derive(Debug, Subcommand)]
pub enum ProdCommand {
    /// Build production images.
    Build,
    /// Start production services in detached mode.
    Up,
    /// Stop production services with an explicit runner policy.
    Down {
        /// Whether to keep or clean matching production runner containers.
        #[arg(long)]
        runner: Option<RunnerDownPolicy>,
        /// Show intended changes without stopping services or removing runners.
        #[arg(long)]
        dry_run: bool,
    },
    /// Follow production service logs.
    Logs,
    /// Show production service status.
    Ps,
    /// Run the production check service.
    Check,
    /// Copy backed-up migrated challenge private bundles into production storage.
    RestorePrivateBundles,
    /// Start the dedicated production runner Docker daemon.
    RunnerDockerUp {
        /// Show the resolved daemon config without starting Docker.
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop the dedicated production runner Docker daemon.
    RunnerDockerDown {
        /// Show the resolved daemon config without stopping Docker.
        #[arg(long)]
        dry_run: bool,
    },
    /// Clean matching production runner containers.
    CleanRunners {
        /// Override the runner namespace. Defaults to AGENTICS_RUNNER_NAMESPACE or the Compose project.
        #[arg(long)]
        namespace: Option<RunnerNamespace>,
        /// Runner scope to clean.
        #[arg(long, value_enum, default_value_t = RunnerCleanupScope::HostedWorker)]
        scope: RunnerCleanupScope,
        /// List matching runners without removing them.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Runner handling policy for production shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RunnerDownPolicy {
    /// Stop Compose services and keep runner containers.
    Keep,
    /// Stop workers, remove matching runner containers, then stop remaining Compose services.
    Clean,
}

/// Runner scope supported by production cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RunnerCleanupScope {
    /// Hosted worker runner containers.
    HostedWorker,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawComposeProdEnv {
    compose_prod_project: Option<String>,
    compose_prod_env_file: Option<String>,
    runner_namespace: Option<String>,
    database_url: Option<String>,
    docker_host: Option<String>,
    docker_socket_path: Option<String>,
    docker_socket_gid: Option<u32>,
    dgx_docker_data_root: Option<String>,
    dgx_runner_docker_exec_root: Option<String>,
    dgx_runner_docker_pidfile: Option<String>,
    dgx_runner_docker_log: Option<String>,
    dgx_runner_docker_bridge: Option<String>,
    dgx_runner_docker_bridge_cidr: Option<String>,
    rustfs_backup_container: Option<String>,
}

impl RawComposeProdEnv {
    fn from_process() -> Result<Self, ComposeProdError> {
        envy::prefixed(ENV_PREFIX)
            .from_env::<Self>()
            .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))
    }

    fn from_map(values: &HashMap<String, String>) -> Result<Self, ComposeProdError> {
        envy::prefixed(ENV_PREFIX)
            .from_iter(values.clone())
            .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))
    }
}

impl RunnerCleanupScope {
    fn as_label(self) -> &'static str {
        match self {
            Self::HostedWorker => RUNNER_SCOPE_HOSTED_WORKER,
        }
    }
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok(code) => code,
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> Result<ExitCode, ComposeProdError> {
    if matches!(&cli.command, ProdCommand::Down { runner: None, .. }) {
        return Err(ComposeProdError::MissingRunnerPolicy);
    }
    let context = ComposeContext::from_cli(&cli)?;
    match cli.command {
        ProdCommand::Build => context.run_compose_passthrough(["build"]).await,
        ProdCommand::Up => {
            context
                .run_compose_passthrough(["up", "-d", "--remove-orphans"])
                .await
        }
        ProdCommand::Down { runner, dry_run } => down(&context, runner, dry_run).await,
        ProdCommand::Logs => context.run_compose_passthrough(["logs", "-f"]).await,
        ProdCommand::Ps => context.run_compose_passthrough(["ps"]).await,
        ProdCommand::Check => {
            context
                .run_compose_passthrough(["run", "--rm", "check"])
                .await
        }
        ProdCommand::RestorePrivateBundles => restore_private_bundles(&context).await,
        ProdCommand::RunnerDockerUp { dry_run } => runner_docker_up(&context, dry_run).await,
        ProdCommand::RunnerDockerDown { dry_run } => runner_docker_down(&context, dry_run).await,
        ProdCommand::CleanRunners {
            namespace,
            scope,
            dry_run,
        } => {
            let namespace = context.resolve_namespace(namespace)?;
            let reports = clean_runners(&context, &namespace, scope, dry_run).await?;
            Ok(print_reports(PREFIX, &reports))
        }
    }
}

async fn restore_private_bundles(context: &ComposeContext) -> Result<ExitCode, ComposeProdError> {
    let network_name = context.default_network_name();
    let backup_container = context.private_bundle_backup_container();
    let joined_network =
        ensure_container_network(context, &backup_container, &network_name).await?;
    let restore_result = context
        .run_compose_passthrough(["run", "--rm", "private-bundle-restore"])
        .await;
    if joined_network
        && let Err(error) =
            disconnect_container_network(context, &backup_container, &network_name).await
    {
        eprintln!(
            "[{PREFIX}] WARN: failed to disconnect backup container `{backup_container}` from network `{network_name}`: {error}"
        );
    }
    restore_result
}

async fn ensure_container_network(
    context: &ComposeContext,
    container: &str,
    network: &str,
) -> Result<bool, ComposeProdError> {
    if container_is_on_network(context, container, network).await? {
        return Ok(false);
    }
    let output = docker_output(
        context,
        [
            "network", "connect", "--alias", container, network, container,
        ],
        Duration::from_secs(30),
    )
    .await?;
    if output.success() {
        return Ok(true);
    }
    let combined = output.combined();
    if combined.contains("already exists") || combined.contains("already connected") {
        return Ok(false);
    }
    Err(ComposeProdError::Process(format!(
        "failed to connect backup container `{container}` to `{network}`: {combined}"
    )))
}

async fn disconnect_container_network(
    context: &ComposeContext,
    container: &str,
    network: &str,
) -> Result<(), ComposeProdError> {
    let output = docker_output(
        context,
        ["network", "disconnect", network, container],
        Duration::from_secs(30),
    )
    .await?;
    if output.success() {
        return Ok(());
    }
    Err(ComposeProdError::Process(format!(
        "docker network disconnect failed: {}",
        output.combined()
    )))
}

async fn container_is_on_network(
    context: &ComposeContext,
    container: &str,
    network: &str,
) -> Result<bool, ComposeProdError> {
    let output = docker_output(
        context,
        [
            "inspect",
            container,
            "--format",
            "{{json .NetworkSettings.Networks}}",
        ],
        Duration::from_secs(30),
    )
    .await?;
    if !output.success() {
        return Err(ComposeProdError::Process(format!(
            "failed to inspect backup container `{container}`: {}",
            output.combined()
        )));
    }
    let networks = serde_json::from_str::<serde_json::Value>(output.stdout.trim())
        .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))?;
    Ok(networks.get(network).is_some())
}

async fn docker_output<I, S>(
    context: &ComposeContext,
    args: I,
    timeout: Duration,
) -> Result<crate::support::CommandOutput, ComposeProdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("docker");
    command
        .args(args)
        .current_dir(&context.repo_root)
        .stdin(Stdio::null());
    run_command(command, "docker", Some(timeout), DEFAULT_OUTPUT_LIMIT_BYTES)
        .await
        .map_err(ComposeProdError::from)
}

async fn down(
    context: &ComposeContext,
    runner: Option<RunnerDownPolicy>,
    dry_run: bool,
) -> Result<ExitCode, ComposeProdError> {
    let policy = runner.ok_or(ComposeProdError::MissingRunnerPolicy)?;
    match (policy, dry_run) {
        (RunnerDownPolicy::Keep, true) => Ok(print_reports(PREFIX, &dry_run_keep_reports())),
        (RunnerDownPolicy::Keep, false) => {
            context
                .run_compose_passthrough(["down", "--remove-orphans"])
                .await
        }
        (RunnerDownPolicy::Clean, true) => {
            let namespace = context.resolve_namespace(None)?;
            let mut reports = dry_run_clean_reports();
            reports.extend(
                clean_runners(context, &namespace, RunnerCleanupScope::HostedWorker, true).await?,
            );
            Ok(print_reports(PREFIX, &reports))
        }
        (RunnerDownPolicy::Clean, false) => {
            stop_running_workers(context).await?;
            let namespace = context.resolve_namespace(None)?;
            let reports =
                clean_runners(context, &namespace, RunnerCleanupScope::HostedWorker, false).await?;
            let cleanup_code = print_reports(PREFIX, &reports);
            if cleanup_code != ExitCode::SUCCESS {
                return Ok(cleanup_code);
            }
            context
                .run_compose_passthrough(["down", "--remove-orphans"])
                .await
        }
    }
}

fn dry_run_keep_reports() -> Vec<ReportLine> {
    vec![ReportLine::pass(
        "compose dry-run",
        format!("would stop services: {}", PROD_SERVICES.join(", ")),
    )]
}

fn dry_run_clean_reports() -> Vec<ReportLine> {
    vec![ReportLine::pass(
        "compose dry-run",
        format!(
            "would stop workers first, clean matching runners, then stop services: {}",
            PROD_SERVICES.join(", ")
        ),
    )]
}

async fn stop_running_workers(context: &ComposeContext) -> Result<(), ComposeProdError> {
    let services = context.compose_output(["ps", "--services"]).await?;
    let worker_services = services
        .stdout
        .lines()
        .map(str::trim)
        .filter(|service| WORKER_SERVICES.contains(service))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if worker_services.is_empty() {
        return Ok(());
    }
    let mut args = vec!["stop".to_string()];
    args.extend(worker_services);
    context.run_compose_passthrough(args).await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct ComposeContext {
    repo_root: PathBuf,
    env_file: PathBuf,
    process_env: RawComposeProdEnv,
    file_env: RawComposeProdEnv,
    project: String,
}

impl ComposeContext {
    fn from_cli(cli: &Cli) -> Result<Self, ComposeProdError> {
        let repo_root = repo_root()?;
        let process_env = RawComposeProdEnv::from_process()?;
        let env_file = resolve_env_file(cli.env_file.as_ref(), &repo_root, &process_env);
        if !env_file.exists() {
            return Err(ComposeProdError::MissingEnvFile(env_file));
        }
        let env_values = load_env_file(&env_file)?;
        let file_env = RawComposeProdEnv::from_map(&env_values)?;
        let project = resolve_project(cli.project.as_deref(), &process_env, &file_env);
        Ok(Self {
            repo_root,
            env_file,
            process_env,
            file_env,
            project,
        })
    }

    fn compose_args<I, S>(&self, args: I) -> Vec<OsString>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut output = vec![
            OsString::from("compose"),
            OsString::from("--env-file"),
            self.env_file.as_os_str().to_os_string(),
            OsString::from("-f"),
            self.repo_root
                .join("deploy/compose/compose.yml")
                .into_os_string(),
            OsString::from("-f"),
            self.repo_root
                .join("deploy/compose/compose.prod.yml")
                .into_os_string(),
            OsString::from("-p"),
            OsString::from(&self.project),
        ];
        output.extend(
            args.into_iter()
                .map(|arg| arg.as_ref().to_os_string())
                .collect::<Vec<_>>(),
        );
        output
    }

    async fn run_compose_passthrough<I, S>(&self, args: I) -> Result<ExitCode, ComposeProdError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_passthrough(
            "docker",
            self.compose_args(args),
            &self.repo_root,
            &self.env_file,
        )
        .await
    }

    async fn compose_output<I, S>(
        &self,
        args: I,
    ) -> Result<crate::support::CommandOutput, ComposeProdError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("docker");
        command
            .args(self.compose_args(args))
            .current_dir(&self.repo_root)
            .env(
                "AGENTICS_COMPOSE_PROD_SERVICE_ENV_FILE",
                self.env_file.as_os_str(),
            )
            .stdin(Stdio::null());
        let output = run_command(
            command,
            "docker compose",
            Some(Duration::from_secs(60)),
            DEFAULT_OUTPUT_LIMIT_BYTES,
        )
        .await?;
        if output.success() {
            Ok(output)
        } else {
            Err(ComposeProdError::Process(format!(
                "docker compose failed with {:?}: {}",
                output.status,
                output.combined()
            )))
        }
    }

    fn resolve_namespace(
        &self,
        override_namespace: Option<RunnerNamespace>,
    ) -> Result<RunnerNamespace, ComposeProdError> {
        if let Some(namespace) = override_namespace {
            return Ok(namespace);
        }
        if let Some(value) = env_value(
            self.process_env.runner_namespace.as_ref(),
            self.file_env.runner_namespace.as_ref(),
        ) {
            return RunnerNamespace::try_new(value)
                .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()));
        }
        RunnerNamespace::try_new(self.project.clone())
            .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))
    }

    fn database_url(&self) -> Option<String> {
        env_value(
            self.process_env.database_url.as_ref(),
            self.file_env.database_url.as_ref(),
        )
    }

    fn docker_host(&self) -> Option<String> {
        env_value(
            self.process_env.docker_host.as_ref(),
            self.file_env.docker_host.as_ref(),
        )
    }

    fn docker_socket_path(&self) -> Option<String> {
        env_value(
            self.process_env.docker_socket_path.as_ref(),
            self.file_env.docker_socket_path.as_ref(),
        )
    }

    fn string_value(
        &self,
        accessor: fn(&RawComposeProdEnv) -> Option<&String>,
        default: &str,
    ) -> Option<String> {
        env_value(accessor(&self.process_env), accessor(&self.file_env))
            .or_else(|| Some(default.to_string()))
            .filter(|value| !value.trim().is_empty())
    }

    fn path_value(
        &self,
        accessor: fn(&RawComposeProdEnv) -> Option<&String>,
        default: &str,
    ) -> PathBuf {
        PathBuf::from(
            self.string_value(accessor, default)
                .unwrap_or_else(|| default.to_string()),
        )
    }

    fn default_network_name(&self) -> String {
        format!("{}_default", self.project)
    }

    fn private_bundle_backup_container(&self) -> String {
        env_value(
            self.process_env.rustfs_backup_container.as_ref(),
            self.file_env.rustfs_backup_container.as_ref(),
        )
        .unwrap_or_else(|| DEFAULT_PRIVATE_BUNDLE_BACKUP_CONTAINER.to_string())
    }
}

async fn run_passthrough<I, S>(
    program: &str,
    args: I,
    cwd: &Path,
    env_file: &Path,
) -> Result<ExitCode, ComposeProdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env(
            "AGENTICS_COMPOSE_PROD_SERVICE_ENV_FILE",
            env_file.as_os_str(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map(ExitCode::from)
        .unwrap_or_else(|| ExitCode::from(1)))
}

fn repo_root() -> Result<PathBuf, ComposeProdError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| ComposeProdError::InvalidConfig("cannot determine repo root".to_string()))
}

fn resolve_env_file(
    cli_env_file: Option<&PathBuf>,
    repo_root: &Path,
    process_env: &RawComposeProdEnv,
) -> PathBuf {
    let path = cli_env_file
        .cloned()
        .or_else(|| env_value(process_env.compose_prod_env_file.as_ref(), None).map(PathBuf::from))
        .unwrap_or_else(|| repo_root.join(DEFAULT_ENV_FILE));
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn resolve_project(
    cli_project: Option<&str>,
    process_env: &RawComposeProdEnv,
    file_env: &RawComposeProdEnv,
) -> String {
    cli_project
        .map(str::to_string)
        .or_else(|| {
            env_value(
                process_env.compose_prod_project.as_ref(),
                file_env.compose_prod_project.as_ref(),
            )
        })
        .unwrap_or_else(|| DEFAULT_PROJECT.to_string())
}

fn load_env_file(path: &Path) -> Result<HashMap<String, String>, ComposeProdError> {
    let mut values = HashMap::new();
    for item in dotenvy::from_path_iter(path)? {
        let (key, value) = item?;
        values.insert(key, value);
    }
    Ok(values)
}

fn env_value(process_value: Option<&String>, file_value: Option<&String>) -> Option<String> {
    non_empty_value(process_value).or_else(|| non_empty_value(file_value))
}

fn non_empty_value(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Production Compose operation error.
#[derive(Debug, thiserror::Error)]
pub enum ComposeProdError {
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error(transparent)]
    Dotenv(#[from] dotenvy::Error),
    #[error(transparent)]
    Docker(#[from] bollard::errors::Error),
    #[error("invalid production Compose config: {0}")]
    InvalidConfig(String),
    #[error(
        "missing production Compose env file {0}; copy deploy/compose/env/prod.env.example to deploy/compose/env/prod.env and replace placeholders"
    )]
    MissingEnvFile(PathBuf),
    #[error("production down requires --runner <keep|clean>")]
    MissingRunnerPolicy,
    #[error("{0}")]
    Process(String),
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, ComposeContext, ComposeProdError, DEFAULT_PROJECT, ProdCommand, RawComposeProdEnv,
        RunnerDownPolicy, down, env_value, resolve_project, run,
    };
    use clap::ValueEnum;
    use std::path::PathBuf;

    /// Verifies production down cannot silently choose a runner policy.
    #[tokio::test]
    async fn down_requires_runner_policy() {
        let context = fake_context();
        let error = down(&context, None, true)
            .await
            .expect_err("missing policy should fail");
        assert!(matches!(error, ComposeProdError::MissingRunnerPolicy));
    }

    /// Verifies the public down command reports the missing runner policy before env-file setup.
    #[tokio::test]
    async fn run_down_requires_runner_before_env_file() {
        let error = run(Cli {
            env_file: Some(PathBuf::from("/tmp/agentics-missing-prod.env")),
            project: None,
            command: ProdCommand::Down {
                runner: None,
                dry_run: true,
            },
        })
        .await
        .expect_err("missing policy should fail before env lookup");
        assert!(matches!(error, ComposeProdError::MissingRunnerPolicy));
    }

    /// Verifies dry-run policy selection is explicit and non-mutating.
    #[test]
    fn runner_down_policy_parses_only_named_values() {
        assert_eq!(
            RunnerDownPolicy::from_str("keep", true).expect("keep parses"),
            RunnerDownPolicy::Keep
        );
        assert_eq!(
            RunnerDownPolicy::from_str("clean", true).expect("clean parses"),
            RunnerDownPolicy::Clean
        );
        assert!(RunnerDownPolicy::from_str("delete", true).is_err());
    }

    /// Verifies env file values can provide the project default.
    #[test]
    fn project_resolves_from_env_file_or_default() {
        let process_env = RawComposeProdEnv::default();
        let mut file_env = RawComposeProdEnv::default();
        assert_eq!(
            resolve_project(None, &process_env, &file_env),
            DEFAULT_PROJECT
        );
        file_env.compose_prod_project = Some("custom-prod".to_string());
        assert_eq!(
            resolve_project(None, &process_env, &file_env),
            "custom-prod"
        );
        assert_eq!(
            resolve_project(Some("cli-prod"), &process_env, &file_env),
            "cli-prod"
        );
        assert_eq!(
            env_value(None, file_env.compose_prod_project.as_ref()).as_deref(),
            Some("custom-prod")
        );
    }

    fn fake_context() -> ComposeContext {
        ComposeContext {
            repo_root: PathBuf::from("/tmp/agentics-test"),
            env_file: PathBuf::from("/tmp/agentics-test/prod.env"),
            process_env: RawComposeProdEnv::default(),
            file_env: RawComposeProdEnv::default(),
            project: DEFAULT_PROJECT.to_string(),
        }
    }
}
