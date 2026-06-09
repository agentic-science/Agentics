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

use std::collections::{BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use agentics_config::{DeploymentStage, EnvPolicyReport, EnvServiceRole, RunnerNamespace};
use agentics_runner::RUNNER_SCOPE_HOSTED_WORKER;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use tokio::process::Command;

use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, print_reports, run_command,
    run_with_ctrl_c,
};

mod bridge_egress;
mod runner_cleanup;
mod runner_docker;

use bridge_egress::{check_api_github_egress, ensure_compose_bridge_egress};
use runner_cleanup::clean_runners;
use runner_docker::{runner_docker_down, runner_docker_up};

const PREFIX: &str = "agentics-compose-prod";
const ENV_PREFIX: &str = "AGENTICS_";
const DEFAULT_PROJECT: &str = "agentics-prod";
const DEFAULT_ENV_FILE: &str = "deploy/compose/env/prod.env";
const DEFAULT_PRIVATE_BUNDLE_BACKUP_CONTAINER: &str = "agentics-rustfs-private-backup";
const REHEARSAL_PROJECT: &str = "agentics-rehearsal";
const REHEARSAL_ROOT: &str = "/srv/agentics-rehearsal";
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
    RestorePrivateBundles {
        /// Replace destination objects that differ from the source.
        #[arg(long)]
        overwrite: bool,
        /// List and verify planned work without uploading.
        #[arg(long)]
        dry_run: bool,
    },
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
    /// Purge disposable rehearsal resources and data.
    PurgeRehearsalData {
        /// Required for destructive purge. Dry-run may omit it.
        #[arg(long)]
        confirm_rehearsal_purge: bool,
        /// Show the rehearsal resources and paths that would be removed.
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
    deployment_stage: Option<String>,
    runner_namespace: Option<String>,
    database_url: Option<String>,
    docker_host: Option<String>,
    docker_socket_path: Option<String>,
    docker_socket_gid: Option<u32>,
    dgx_state_root: Option<String>,
    storage_work_root: Option<String>,
    challenge_review_repository_host_root: Option<String>,
    runner_runtime_root: Option<String>,
    runner_phase_mount_root: Option<String>,
    dgx_phase_mount_root: Option<String>,
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
            let compose_code = context
                .run_compose_passthrough(["up", "-d", "--remove-orphans"])
                .await?;
            if compose_code != ExitCode::SUCCESS {
                return Ok(compose_code);
            }
            let reports = ensure_compose_bridge_egress(&context).await?;
            Ok(print_reports(PREFIX, &reports))
        }
        ProdCommand::Down { runner, dry_run } => down(&context, runner, dry_run).await,
        ProdCommand::Logs => context.run_compose_passthrough(["logs", "-f"]).await,
        ProdCommand::Ps => context.run_compose_passthrough(["ps"]).await,
        ProdCommand::Check => {
            let reports = ensure_compose_bridge_egress(&context).await?;
            let egress_code = print_reports(PREFIX, &reports);
            if egress_code != ExitCode::SUCCESS {
                return Ok(egress_code);
            }
            let compose_code = context
                .run_compose_passthrough(["run", "--rm", "check"])
                .await?;
            if compose_code != ExitCode::SUCCESS {
                return Ok(compose_code);
            }
            let reports = check_api_github_egress(&context).await?;
            Ok(print_reports(PREFIX, &reports))
        }
        ProdCommand::RestorePrivateBundles { overwrite, dry_run } => {
            restore_private_bundles(&context, overwrite, dry_run).await
        }
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
        ProdCommand::PurgeRehearsalData {
            confirm_rehearsal_purge,
            dry_run,
        } => purge_rehearsal_data(&context, confirm_rehearsal_purge, dry_run).await,
    }
}

async fn restore_private_bundles(
    context: &ComposeContext,
    overwrite: bool,
    dry_run: bool,
) -> Result<ExitCode, ComposeProdError> {
    let network_name = context.default_network_name();
    let backup_container = context.private_bundle_backup_container();
    let joined_network =
        ensure_container_network(context, &backup_container, &network_name).await?;
    let restore_result = context
        .run_compose_passthrough(private_bundle_restore_args(context, overwrite, dry_run))
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

fn private_bundle_restore_args(
    context: &ComposeContext,
    overwrite: bool,
    dry_run: bool,
) -> Vec<OsString> {
    let work_dir = context
        .path_value(
            |env| env.storage_work_root.as_ref(),
            "/srv/agentics/storage-work",
        )
        .join("private-bundle-backup-copy");
    let mut args = vec![
        OsString::from("run"),
        OsString::from("--rm"),
        OsString::from("private-bundle-restore"),
        OsString::from("/usr/local/bin/agentics-copy-private-bundle-backups"),
        OsString::from("--work-dir"),
        work_dir.into_os_string(),
    ];
    if overwrite {
        args.push(OsString::from("--overwrite"));
    }
    if dry_run {
        args.push(OsString::from("--dry-run"));
    }
    args
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
            let namespace = context.resolve_namespace(None)?;
            drop(clean_runners(context, &namespace, RunnerCleanupScope::HostedWorker, true).await?);
            stop_running_workers(context).await?;
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

async fn purge_rehearsal_data(
    context: &ComposeContext,
    confirm_rehearsal_purge: bool,
    dry_run: bool,
) -> Result<ExitCode, ComposeProdError> {
    let plan = build_rehearsal_purge_plan(context, confirm_rehearsal_purge, dry_run)?;
    if dry_run {
        return Ok(print_reports(PREFIX, &plan.dry_run_reports()));
    }

    stop_running_workers(context).await?;
    let runner_reports = match unavailable_runner_cleanup_reports(context, &plan.namespace) {
        Some(reports) => reports,
        None => {
            clean_runners(
                context,
                &plan.namespace,
                RunnerCleanupScope::HostedWorker,
                false,
            )
            .await?
        }
    };
    let runner_code = print_reports(PREFIX, &runner_reports);
    if runner_code != ExitCode::SUCCESS {
        return Ok(runner_code);
    }

    let runner_stop_code = runner_docker_down(context, false).await?;
    if runner_stop_code != ExitCode::SUCCESS {
        return Ok(runner_stop_code);
    }

    let compose_code = context
        .run_compose_passthrough(["down", "-v", "--remove-orphans"])
        .await?;
    if compose_code != ExitCode::SUCCESS {
        return Ok(compose_code);
    }

    let mut reports = vec![ReportLine::pass(
        "rehearsal purge",
        format!(
            "removed Compose project {} and runner namespace {}",
            context.project,
            plan.namespace.as_str()
        ),
    )];
    for path in &plan.cleanup_paths {
        reports.push(remove_rehearsal_path(path).await?);
    }
    Ok(print_reports(PREFIX, &reports))
}

fn unavailable_runner_cleanup_reports(
    context: &ComposeContext,
    namespace: &RunnerNamespace,
) -> Option<Vec<ReportLine>> {
    let socket_path = context.docker_socket_path().map(PathBuf::from)?;
    if socket_path.exists() {
        return None;
    }
    Some(vec![ReportLine::skip(
        "runner cleanup",
        format!(
            "runner Docker socket {} did not exist; assuming daemon already stopped for namespace {}",
            socket_path.display(),
            namespace.as_str()
        ),
    )])
}

#[derive(Debug, Clone)]
struct RehearsalPurgePlan {
    namespace: RunnerNamespace,
    reported_paths: Vec<PathBuf>,
    cleanup_paths: Vec<PathBuf>,
}

impl RehearsalPurgePlan {
    fn dry_run_reports(&self) -> Vec<ReportLine> {
        let mut reports = vec![
            ReportLine::pass(
                "rehearsal purge",
                format!("would remove Compose project {REHEARSAL_PROJECT}"),
            ),
            ReportLine::pass(
                "rehearsal purge",
                format!(
                    "would remove runner containers in namespace {}",
                    self.namespace.as_str()
                ),
            ),
            ReportLine::pass(
                "rehearsal purge",
                "would stop the dedicated rehearsal runner Docker daemon",
            ),
            ReportLine::pass(
                "rehearsal purge",
                "would remove rehearsal Compose volumes with docker compose down -v",
            ),
        ];
        reports.extend(self.reported_paths.iter().map(|path| {
            ReportLine::pass(
                "rehearsal purge path",
                format!("would remove or verify {}", path.display()),
            )
        }));
        reports
    }
}

fn build_rehearsal_purge_plan(
    context: &ComposeContext,
    confirm_rehearsal_purge: bool,
    dry_run: bool,
) -> Result<RehearsalPurgePlan, ComposeProdError> {
    if !dry_run && !confirm_rehearsal_purge {
        return Err(ComposeProdError::InvalidConfig(
            "destructive rehearsal purge requires --confirm-rehearsal-purge".to_string(),
        ));
    }
    if context.project == DEFAULT_PROJECT {
        return Err(ComposeProdError::InvalidConfig(format!(
            "refusing to purge production Compose project {DEFAULT_PROJECT}"
        )));
    }
    if context.project != REHEARSAL_PROJECT {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge requires --project {REHEARSAL_PROJECT}, got {}",
            context.project
        )));
    }
    if context.file_deployment_stage() != Some(DeploymentStage::Rehearsal) {
        return Err(ComposeProdError::InvalidConfig(
            "rehearsal purge requires AGENTICS_DEPLOYMENT_STAGE=rehearsal in the env file"
                .to_string(),
        ));
    }
    let namespace = context.resolve_namespace(None)?;
    if namespace.as_str() != REHEARSAL_PROJECT {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge requires AGENTICS_RUNNER_NAMESPACE={REHEARSAL_PROJECT}, got {}",
            namespace.as_str()
        )));
    }

    let reported_paths = collect_rehearsal_paths(context)?;
    for path in &reported_paths {
        require_rehearsal_path(path)?;
    }
    let cleanup_paths = collapse_cleanup_paths(&reported_paths);
    Ok(RehearsalPurgePlan {
        namespace,
        reported_paths,
        cleanup_paths,
    })
}

fn collect_rehearsal_paths(context: &ComposeContext) -> Result<Vec<PathBuf>, ComposeProdError> {
    let mut paths = BTreeSet::new();
    paths.insert(context.path_value(|env| env.dgx_state_root.as_ref(), "/srv/agentics"));
    paths.insert(context.path_value(
        |env| env.storage_work_root.as_ref(),
        "/srv/agentics/storage-work",
    ));
    paths.insert(context.path_value(
        |env| env.challenge_review_repository_host_root.as_ref(),
        "/srv/agentics/review-checkouts/agentics-challenges",
    ));
    paths.insert(context.path_value(
        |env| env.runner_runtime_root.as_ref(),
        "/srv/agentics/runtime",
    ));
    paths.insert(context.path_value(
        |env| env.runner_phase_mount_root.as_ref(),
        "/srv/agentics/phase-mounts",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_phase_mount_root.as_ref(),
        "/srv/agentics/phase-mounts",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_docker_data_root.as_ref(),
        "/srv/agentics/docker-data-root",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_runner_docker_exec_root.as_ref(),
        "/srv/agentics/docker-exec",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_runner_docker_pidfile.as_ref(),
        "/srv/agentics/docker.pid",
    ));
    paths.insert(context.path_value(
        |env| env.dgx_runner_docker_log.as_ref(),
        "/srv/agentics/dockerd.log",
    ));
    if let Some(socket_path) = context.docker_socket_path() {
        paths.insert(PathBuf::from(socket_path));
    } else {
        paths.insert(PathBuf::from("/srv/agentics/docker.sock"));
    }

    let paths = paths.into_iter().collect::<Vec<_>>();
    if paths.is_empty() {
        return Err(ComposeProdError::InvalidConfig(
            "rehearsal purge resolved no cleanup paths".to_string(),
        ));
    }
    Ok(paths)
}

fn require_rehearsal_path(path: &Path) -> Result<(), ComposeProdError> {
    let rehearsal_root = Path::new(REHEARSAL_ROOT);
    if !path.is_absolute() {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge path must be absolute, got {}",
            path.display()
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge path must not contain parent traversal, got {}",
            path.display()
        )));
    }
    if !path.starts_with(rehearsal_root) {
        return Err(ComposeProdError::InvalidConfig(format!(
            "rehearsal purge path {} is outside {REHEARSAL_ROOT}",
            path.display()
        )));
    }
    Ok(())
}

fn collapse_cleanup_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = paths.to_vec();
    sorted.sort_by_key(|path| path.components().count());
    let mut collapsed = Vec::new();
    'path: for path in sorted {
        for parent in &collapsed {
            if path == *parent || path.starts_with(parent) {
                continue 'path;
            }
        }
        collapsed.push(path);
    }
    collapsed
}

async fn remove_rehearsal_path(path: &Path) -> Result<ReportLine, ComposeProdError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReportLine::skip(
                "rehearsal purge path",
                format!("{} did not exist", path.display()),
            ));
        }
        Err(error) => return Err(ComposeProdError::Process(error.to_string())),
    };
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    } else {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| ComposeProdError::Process(error.to_string()))?;
    }
    Ok(ReportLine::pass(
        "rehearsal purge path",
        format!("removed {}", path.display()),
    ))
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
        let env_report = agentics_config::validate_env_policy(&env_values, EnvServiceRole::Compose)
            .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))?;
        print_env_policy_warnings(&env_report);
        let file_env = RawComposeProdEnv::from_map(&env_values)?;
        let project = resolve_project(cli.project.as_deref(), &process_env, &file_env);
        let context = Self {
            repo_root,
            env_file,
            process_env,
            file_env,
            project,
        };
        Ok(context)
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
        ];
        if self.is_rehearsal_environment() {
            output.push(OsString::from("-f"));
            output.push(
                self.repo_root
                    .join("deploy/compose/compose.rehearsal.yml")
                    .into_os_string(),
            );
        }
        output.extend([OsString::from("-p"), OsString::from(&self.project)]);
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

    fn is_rehearsal_environment(&self) -> bool {
        self.file_deployment_stage() == Some(DeploymentStage::Rehearsal)
    }

    fn file_deployment_stage(&self) -> Option<DeploymentStage> {
        self.file_env
            .deployment_stage
            .as_deref()
            .and_then(|value| value.parse().ok())
    }
}

fn print_env_policy_warnings(report: &EnvPolicyReport) {
    for warning in &report.warnings {
        eprintln!("[{PREFIX}] WARN env {}: {}", warning.name, warning.message);
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
        "missing Compose env file {0}; copy the matching deploy/compose/env/*.env.example file and replace placeholders"
    )]
    MissingEnvFile(PathBuf),
    #[error("production down requires --runner <keep|clean>")]
    MissingRunnerPolicy,
    #[error("{0}")]
    Process(String),
}

#[cfg(test)]
mod tests;
