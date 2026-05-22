//! Rust-native DGX Spark profile management.
//!
//! This module oxidizes `manage-dgx-spark-profile.sh` into a separate
//! `agentics-manage-dgx-spark-profile` binary. It installs systemd units,
//! starts/stops services, removes Agentics-owned Docker containers, and can
//! uninstall quota storage. External commands remain only where the operating
//! system owns the behavior: service management, Unix identity management,
//! mounts, and system configuration files.
//!
//! Mutating commands support `--dry-run`. Destructive purge requires
//! `AGENTICS_DGX_PROFILE_CONFIRM=uninstall-purge` unless dry-run is used.
//! Install rollback restores files created by the current invocation on a
//! best-effort basis.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::query_parameters::{ListContainersOptionsBuilder, RemoveContainerOptionsBuilder};
use clap::{Parser, Subcommand};
use nix::unistd::Uid;

use crate::dgx::{
    DEFAULT_CONFIG_ROOT, DEFAULT_DOCKER_HOST_URI, DEFAULT_RELEASE_ROOT, DEFAULT_STATE_ROOT,
    DEFAULT_TEST_STATE_ROOT, DgxProfileConfig, ENV_DGX_PERSIST_FSTAB, ENV_DGX_PROFILE_CONFIRM,
    PROFILE_PURGE_CONFIRMATION, STORAGE_CONFIRMATION,
};
use crate::dgx_storage::{StorageError, prepare_storage_config};
use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, env_non_empty, print_reports,
    require_safe_destructive_path, run_process,
};

const PREFIX: &str = "agentics-dgx-profile";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const SERVICES: &[&str] = &[
    "agentics-web.service",
    "agentics-worker.service",
    "agentics-api.service",
    "agentics-docker.service",
];

/// CLI for managing the DGX Spark hosted systemd profile.
#[derive(Debug, Parser)]
#[command(
    about = "Installs, starts, stops, or uninstalls the Agentics DGX Spark profile.",
    long_about = "Installs Agentics DGX Spark systemd units and configuration, starts/stops services, and uninstalls profile-owned runtime state. Rootful mutations support --dry-run. Destructive data purge requires AGENTICS_DGX_PROFILE_CONFIRM=uninstall-purge."
)]
pub struct Cli {
    #[command(subcommand)]
    command: ProfileCommand,
}

/// DGX profile lifecycle command.
#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    /// Install systemd units, config files, service identity, and storage.
    Install {
        /// Print intended mutations without applying them.
        #[arg(long)]
        dry_run: bool,
        /// Do not prepare XFS quota storage during install.
        #[arg(long)]
        skip_storage: bool,
    },
    /// Start Agentics DGX profile services.
    Start {
        /// Print intended service operations without applying them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop Agentics DGX profile services.
    Stop {
        /// Print intended service operations without applying them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Uninstall systemd units and quota storage.
    Uninstall {
        /// Print intended mutations without applying them.
        #[arg(long)]
        dry_run: bool,
        /// Also remove config, release, durable state roots, and service identity.
        #[arg(long)]
        purge_data: bool,
    },
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(reports) => print_reports(PREFIX, &reports),
        Err(ProfileError::Interrupted) => {
            eprintln!("[{PREFIX}] interrupted by Ctrl-C");
            ExitCode::from(crate::support::INTERRUPTED_EXIT)
        }
        Err(error) => {
            eprintln!("[{PREFIX}] ERROR: {error}");
            ExitCode::from(2)
        }
    }
}

async fn run(cli: Cli) -> Result<Vec<ReportLine>, ProfileError> {
    let config = DgxProfileConfig::from_env();
    match cli.command {
        ProfileCommand::Install {
            dry_run,
            skip_storage,
        } => install_profile(&config, dry_run, skip_storage).await,
        ProfileCommand::Start { dry_run } => start_profile(dry_run).await,
        ProfileCommand::Stop { dry_run } => stop_profile(dry_run).await,
        ProfileCommand::Uninstall {
            dry_run,
            purge_data,
        } => uninstall_profile(&config, dry_run, purge_data).await,
    }
}

async fn install_profile(
    config: &DgxProfileConfig,
    dry_run: bool,
    skip_storage: bool,
) -> Result<Vec<ReportLine>, ProfileError> {
    require_linux_and_root(dry_run)?;
    let plan = InstallPlan::from_config(config, skip_storage);
    if dry_run {
        return Ok(plan
            .actions
            .iter()
            .map(|action| ReportLine::pass("dry-run", action.describe()))
            .collect());
    }

    let mut rollback = InstallRollback::default();
    let mut reports = Vec::new();
    for action in &plan.actions {
        let result = tokio::select! {
            result = apply_install_action(config, action, &mut rollback) => result,
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() {
                    rollback.rollback().await;
                    return Err(ProfileError::Interrupted);
                }
                Err(ProfileError::Interrupted)
            }
        };
        match result {
            Ok(message) => reports.push(ReportLine::pass(action.label(), message)),
            Err(error) => {
                rollback.rollback().await;
                return Err(error);
            }
        }
    }
    reports.push(ReportLine::pass(
        "DGX profile",
        format!(
            "installed profile files; edit {} before starting services",
            config.config_root.join("agentics.env").display()
        ),
    ));
    Ok(reports)
}

async fn start_profile(dry_run: bool) -> Result<Vec<ReportLine>, ProfileError> {
    require_linux_and_root(dry_run)?;
    let actions = vec![
        ServiceAction::DaemonReload,
        ServiceAction::EnableNow("agentics-docker.service"),
        ServiceAction::Start("agentics-api.service"),
        ServiceAction::Start("agentics-worker.service"),
        ServiceAction::Start("agentics-web.service"),
    ];
    run_service_actions(actions, dry_run).await
}

async fn stop_profile(dry_run: bool) -> Result<Vec<ReportLine>, ProfileError> {
    require_linux_and_root(dry_run)?;
    let actions = vec![
        ServiceAction::Stop("agentics-web.service"),
        ServiceAction::Stop("agentics-worker.service"),
        ServiceAction::Stop("agentics-api.service"),
        ServiceAction::Stop("agentics-docker.service"),
    ];
    run_service_actions(actions, dry_run).await
}

async fn uninstall_profile(
    config: &DgxProfileConfig,
    dry_run: bool,
    purge_data: bool,
) -> Result<Vec<ReportLine>, ProfileError> {
    require_linux_and_root(dry_run)?;
    require_purge_confirmation(purge_data, dry_run)?;
    validate_uninstall_roots(config, purge_data)?;
    let plan = UninstallPlan::from_config(config, purge_data);
    if dry_run {
        return Ok(plan
            .actions
            .iter()
            .map(|action| ReportLine::pass("dry-run", action.describe()))
            .collect());
    }

    let mut reports = Vec::new();
    for action in plan.actions {
        let result = tokio::select! {
            result = apply_uninstall_action(config, &action) => result,
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() {
                    return Err(ProfileError::Interrupted);
                }
                Err(ProfileError::Interrupted)
            }
        };
        match result {
            Ok(message) => reports.push(ReportLine::pass(action.label(), message)),
            Err(error) => reports.push(ReportLine::fail(action.label(), error.to_string())),
        }
    }
    Ok(reports)
}

fn require_linux_and_root(dry_run: bool) -> Result<(), ProfileError> {
    if !cfg!(target_os = "linux") {
        return Err(ProfileError::Unsafe(format!(
            "DGX Spark profile management is Linux-only; detected {}",
            std::env::consts::OS
        )));
    }
    if !dry_run && !Uid::effective().is_root() {
        return Err(ProfileError::Unsafe(
            "DGX Spark profile management must run as root; use sudo".to_string(),
        ));
    }
    Ok(())
}

fn require_purge_confirmation(purge_data: bool, dry_run: bool) -> Result<(), ProfileError> {
    if !purge_data || dry_run {
        return Ok(());
    }
    if env_non_empty(ENV_DGX_PROFILE_CONFIRM).as_deref() == Some(PROFILE_PURGE_CONFIRMATION) {
        Ok(())
    } else {
        Err(ProfileError::Unsafe(format!(
            "refusing to purge profile data without {ENV_DGX_PROFILE_CONFIRM}={PROFILE_PURGE_CONFIRMATION}"
        )))
    }
}

fn validate_uninstall_roots(
    config: &DgxProfileConfig,
    purge_data: bool,
) -> Result<(), ProfileError> {
    if config.state_root == config.test_state_root {
        return Err(ProfileError::Unsafe(
            "refusing uninstall because state root and test state root are identical".to_string(),
        ));
    }
    for (label, path, allowed) in [
        (
            "state root",
            &config.state_root,
            vec![PathBuf::from(DEFAULT_STATE_ROOT)],
        ),
        (
            "test state root",
            &config.test_state_root,
            vec![PathBuf::from(DEFAULT_TEST_STATE_ROOT)],
        ),
    ] {
        require_safe_destructive_path(path, label, &allowed)?;
    }
    if purge_data {
        for (label, path, allowed) in [
            (
                "config root",
                &config.config_root,
                vec![PathBuf::from(DEFAULT_CONFIG_ROOT)],
            ),
            (
                "release root",
                &config.release_root,
                vec![PathBuf::from(DEFAULT_RELEASE_ROOT)],
            ),
        ] {
            require_safe_destructive_path(path, label, &allowed)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallPlan {
    actions: Vec<InstallAction>,
}

impl InstallPlan {
    fn from_config(config: &DgxProfileConfig, skip_storage: bool) -> Self {
        let mut actions = vec![
            InstallAction::EnsureIdentity,
            InstallAction::EnsureDir {
                path: config.config_root.clone(),
                mode: "0750",
                service_group_owned: true,
            },
            InstallAction::EnsureDir {
                path: config.systemd_root.clone(),
                mode: "0755",
                service_group_owned: false,
            },
            InstallAction::CopyFile {
                source: config
                    .source_root
                    .join("deploy/dgx-spark/dockerd-agentics.json"),
                destination: config.config_root.join("dockerd-agentics.json"),
                overwrite: true,
                mode: "0644",
                service_group_readable: false,
            },
            InstallAction::CopyFile {
                source: config
                    .source_root
                    .join("deploy/dgx-spark/agentics.env.example"),
                destination: config.config_root.join("agentics.env"),
                overwrite: false,
                mode: "0640",
                service_group_readable: true,
            },
        ];
        for service in SERVICES {
            actions.push(InstallAction::CopyFile {
                source: config.source_root.join("deploy/dgx-spark").join(service),
                destination: config.systemd_root.join(service),
                overwrite: true,
                mode: "0644",
                service_group_readable: false,
            });
        }
        if !skip_storage {
            actions.push(InstallAction::PrepareStorage);
        }
        actions.push(InstallAction::SystemdDaemonReload);
        Self { actions }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallAction {
    EnsureIdentity,
    EnsureDir {
        path: PathBuf,
        mode: &'static str,
        service_group_owned: bool,
    },
    CopyFile {
        source: PathBuf,
        destination: PathBuf,
        overwrite: bool,
        mode: &'static str,
        service_group_readable: bool,
    },
    PrepareStorage,
    SystemdDaemonReload,
}

impl InstallAction {
    fn label(&self) -> &'static str {
        match self {
            Self::EnsureIdentity => "service identity",
            Self::EnsureDir { .. } => "directory",
            Self::CopyFile { .. } => "file",
            Self::PrepareStorage => "storage",
            Self::SystemdDaemonReload => "systemd",
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::EnsureIdentity => "ensure service user and group exist".to_string(),
            Self::EnsureDir { path, mode, .. } => {
                format!("ensure directory {} mode {mode}", path.display())
            }
            Self::CopyFile {
                source,
                destination,
                overwrite,
                mode,
                ..
            } => format!(
                "copy {} to {} mode {mode}{}",
                source.display(),
                destination.display(),
                if *overwrite { "" } else { " if absent" }
            ),
            Self::PrepareStorage => {
                "prepare DGX quota storage through Rust storage library".to_string()
            }
            Self::SystemdDaemonReload => "run systemctl daemon-reload if available".to_string(),
        }
    }
}

async fn apply_install_action(
    config: &DgxProfileConfig,
    action: &InstallAction,
    rollback: &mut InstallRollback,
) -> Result<String, ProfileError> {
    match action {
        InstallAction::EnsureIdentity => ensure_service_identity(config).await,
        InstallAction::EnsureDir {
            path,
            mode,
            service_group_owned,
        } => ensure_profile_directory(config, path, mode, *service_group_owned, rollback).await,
        InstallAction::CopyFile {
            source,
            destination,
            overwrite,
            mode,
            service_group_readable,
        } => {
            install_profile_file(
                config,
                source,
                destination,
                *overwrite,
                mode,
                *service_group_readable,
                rollback,
            )
            .await
        }
        InstallAction::PrepareStorage => prepare_profile_storage().await,
        InstallAction::SystemdDaemonReload => reload_systemd_units().await,
    }
}

async fn ensure_service_identity(config: &DgxProfileConfig) -> Result<String, ProfileError> {
    ensure_service_group(&config.service_group).await?;
    ensure_service_user(config).await?;
    Ok(format!(
        "ensured {}:{}",
        config.service_user, config.service_group
    ))
}

async fn ensure_service_group(service_group: &str) -> Result<(), ProfileError> {
    if command_success(
        "getent",
        vec!["group".to_string(), service_group.to_string()],
    )
    .await?
    {
        return Ok(());
    }
    checked_process(
        "groupadd",
        vec!["--system".to_string(), service_group.to_string()],
    )
    .await
}

async fn ensure_service_user(config: &DgxProfileConfig) -> Result<(), ProfileError> {
    if command_success(
        "getent",
        vec!["passwd".to_string(), config.service_user.clone()],
    )
    .await?
    {
        return Ok(());
    }
    checked_process(
        "useradd",
        vec![
            "--system".to_string(),
            "--gid".to_string(),
            config.service_group.clone(),
            "--home-dir".to_string(),
            config.state_root.to_string_lossy().to_string(),
            "--shell".to_string(),
            "/usr/sbin/nologin".to_string(),
            config.service_user.clone(),
        ],
    )
    .await
}

async fn ensure_profile_directory(
    config: &DgxProfileConfig,
    path: &Path,
    mode: &str,
    service_group_owned: bool,
    rollback: &mut InstallRollback,
) -> Result<String, ProfileError> {
    if !path.exists() {
        tokio::fs::create_dir_all(path).await?;
        rollback.created_paths.push(path.to_path_buf());
    }
    checked_process(
        "chmod",
        vec![mode.to_string(), path.to_string_lossy().to_string()],
    )
    .await?;
    if service_group_owned {
        checked_process(
            "chown",
            vec![
                format!("root:{}", config.service_group),
                path.to_string_lossy().to_string(),
            ],
        )
        .await?;
    }
    Ok(format!("ensured {}", path.display()))
}

async fn install_profile_file(
    config: &DgxProfileConfig,
    source: &Path,
    destination: &Path,
    overwrite: bool,
    mode: &str,
    service_group_readable: bool,
    rollback: &mut InstallRollback,
) -> Result<String, ProfileError> {
    if destination.exists() && !overwrite {
        apply_file_permissions(config, destination, mode, service_group_readable).await?;
        return Ok(format!("{} already exists", destination.display()));
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if destination.exists() {
        rollback.backup_file(destination).await?;
    } else {
        rollback.created_paths.push(destination.to_path_buf());
    }
    tokio::fs::copy(source, destination).await?;
    apply_file_permissions(config, destination, mode, service_group_readable).await?;
    Ok(format!("installed {}", destination.display()))
}

async fn prepare_profile_storage() -> Result<String, ProfileError> {
    let mut storage_config = crate::dgx::DgxStorageConfig::from_env()?;
    if crate::support::env_non_empty(ENV_DGX_PERSIST_FSTAB).is_none() {
        storage_config.persist_fstab = true;
    }
    let reports = prepare_storage_config(storage_config, false, Some(STORAGE_CONFIRMATION), false)
        .await
        .map_err(ProfileError::Storage)?;
    let failures = reports.iter().filter(|line| line.is_failure()).count();
    if failures == 0 {
        Ok("prepared storage".to_string())
    } else {
        Err(ProfileError::Unsafe(format!(
            "storage preparation reported {failures} failure(s)"
        )))
    }
}

async fn reload_systemd_units() -> Result<String, ProfileError> {
    systemctl_if_available(["daemon-reload"]).await?;
    Ok("reloaded systemd units".to_string())
}

async fn apply_file_permissions(
    config: &DgxProfileConfig,
    path: &Path,
    mode: &str,
    service_group_readable: bool,
) -> Result<(), ProfileError> {
    checked_process(
        "chmod",
        vec![mode.to_string(), path.to_string_lossy().to_string()],
    )
    .await?;
    let owner = if service_group_readable {
        format!("root:{}", config.service_group)
    } else {
        "root:root".to_string()
    };
    checked_process("chown", vec![owner, path.to_string_lossy().to_string()]).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UninstallPlan {
    actions: Vec<UninstallAction>,
}

impl UninstallPlan {
    fn from_config(config: &DgxProfileConfig, purge_data: bool) -> Self {
        let mut actions = vec![
            UninstallAction::StopServices,
            UninstallAction::RemoveDockerContainers,
            UninstallAction::StopDockerService,
            UninstallAction::RemoveFstabEntries,
            UninstallAction::RemoveProjectEntries,
            UninstallAction::UnmountTree(config.test_state_root.clone()),
            UninstallAction::UnmountTree(config.state_root.clone()),
            UninstallAction::RemoveQuotaStorage,
            UninstallAction::RemoveSystemdUnits,
            UninstallAction::RemoveRuntimeDir,
        ];
        if purge_data {
            actions.extend([
                UninstallAction::RemovePath(config.config_root.clone()),
                UninstallAction::RemovePath(config.release_root.clone()),
                UninstallAction::RemovePath(config.state_root.clone()),
                UninstallAction::RemovePath(config.test_state_root.clone()),
                UninstallAction::RemoveIdentity,
            ]);
        }
        Self { actions }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UninstallAction {
    StopServices,
    StopDockerService,
    RemoveDockerContainers,
    RemoveFstabEntries,
    RemoveProjectEntries,
    UnmountTree(PathBuf),
    RemoveQuotaStorage,
    RemoveSystemdUnits,
    RemoveRuntimeDir,
    RemovePath(PathBuf),
    RemoveIdentity,
}

impl UninstallAction {
    fn label(&self) -> &'static str {
        match self {
            Self::StopServices | Self::StopDockerService => "systemd",
            Self::RemoveDockerContainers => "Docker cleanup",
            Self::RemoveFstabEntries => "fstab",
            Self::RemoveProjectEntries => "XFS projects",
            Self::UnmountTree(_) => "unmount",
            Self::RemoveQuotaStorage | Self::RemoveRuntimeDir | Self::RemovePath(_) => "filesystem",
            Self::RemoveSystemdUnits => "systemd units",
            Self::RemoveIdentity => "service identity",
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::StopServices => "stop API, worker, and web services".to_string(),
            Self::StopDockerService => "stop agentics-docker.service".to_string(),
            Self::RemoveDockerContainers => {
                "remove containers from Agentics Docker daemon".to_string()
            }
            Self::RemoveFstabEntries => "backup and remove DGX quota fstab entries".to_string(),
            Self::RemoveProjectEntries => "backup and remove DGX XFS project entries".to_string(),
            Self::UnmountTree(root) => {
                format!("unmount mounted filesystems under {}", root.display())
            }
            Self::RemoveQuotaStorage => {
                "remove loop image, Docker data root, phase mounts, and test state root".to_string()
            }
            Self::RemoveSystemdUnits => "disable and remove Agentics systemd units".to_string(),
            Self::RemoveRuntimeDir => "remove /run/agentics".to_string(),
            Self::RemovePath(path) => format!("remove {}", path.display()),
            Self::RemoveIdentity => "remove service user and group if present".to_string(),
        }
    }
}

async fn apply_uninstall_action(
    config: &DgxProfileConfig,
    action: &UninstallAction,
) -> Result<String, ProfileError> {
    match action {
        UninstallAction::StopServices => stop_application_services().await,
        UninstallAction::StopDockerService => stop_agentics_docker_service().await,
        UninstallAction::RemoveDockerContainers => remove_agentics_docker_containers(config).await,
        UninstallAction::RemoveFstabEntries => remove_agentics_fstab_entries(config).await,
        UninstallAction::RemoveProjectEntries => remove_agentics_project_entries(config).await,
        UninstallAction::UnmountTree(root) => unmount_tree(root).await,
        UninstallAction::RemoveQuotaStorage => remove_quota_storage(config).await,
        UninstallAction::RemoveSystemdUnits => remove_systemd_units(config).await,
        UninstallAction::RemoveRuntimeDir => remove_runtime_dir().await,
        UninstallAction::RemovePath(path) => remove_profile_path(path).await,
        UninstallAction::RemoveIdentity => remove_service_identity(config).await,
    }
}

async fn stop_application_services() -> Result<String, ProfileError> {
    for service in SERVICES
        .iter()
        .copied()
        .filter(|service| *service != "agentics-docker.service")
    {
        let _ignored = systemctl_if_available(["stop", service]).await;
    }
    Ok("stopped application services".to_string())
}

async fn stop_agentics_docker_service() -> Result<String, ProfileError> {
    let _ignored = systemctl_if_available(["stop", "agentics-docker.service"]).await;
    Ok("stopped Agentics Docker service".to_string())
}

async fn remove_agentics_fstab_entries(config: &DgxProfileConfig) -> Result<String, ProfileError> {
    remove_fstab_entries(
        Path::new("/etc/fstab"),
        &[&config.state_root, &config.test_state_root],
    )
    .await
}

async fn remove_agentics_project_entries(
    config: &DgxProfileConfig,
) -> Result<String, ProfileError> {
    remove_lines_matching_paths(
        Path::new("/etc/projects"),
        &[&config.state_root, &config.test_state_root],
    )
    .await?;
    remove_lines_matching_paths(
        Path::new("/etc/projid"),
        &[&config.state_root, &config.test_state_root],
    )
    .await
}

async fn remove_quota_storage(config: &DgxProfileConfig) -> Result<String, ProfileError> {
    for path in [
        config.state_root.join("loop-images"),
        config.state_root.join("docker-data-root"),
        config.state_root.join("phase-mounts"),
        config.test_state_root.clone(),
    ] {
        remove_path_if_exists(&path).await?;
    }
    Ok("removed quota storage paths".to_string())
}

async fn remove_systemd_units(config: &DgxProfileConfig) -> Result<String, ProfileError> {
    for service in SERVICES {
        let _ignored = systemctl_if_available(["disable", service]).await;
        remove_path_if_exists(&config.systemd_root.join(service)).await?;
    }
    let _ignored = systemctl_if_available(["daemon-reload"]).await;
    let _ignored = systemctl_if_available(["reset-failed"]).await;
    Ok("removed systemd units".to_string())
}

async fn remove_runtime_dir() -> Result<String, ProfileError> {
    remove_path_if_exists(Path::new("/run/agentics")).await?;
    Ok("removed /run/agentics".to_string())
}

async fn remove_profile_path(path: &Path) -> Result<String, ProfileError> {
    remove_path_if_exists(path).await?;
    Ok(format!("removed {}", path.display()))
}

async fn remove_service_identity(config: &DgxProfileConfig) -> Result<String, ProfileError> {
    if command_success(
        "getent",
        vec!["passwd".to_string(), config.service_user.clone()],
    )
    .await?
    {
        let _ignored = checked_process("userdel", vec![config.service_user.clone()]).await;
    }
    if command_success(
        "getent",
        vec!["group".to_string(), config.service_group.clone()],
    )
    .await?
    {
        let _ignored = checked_process("groupdel", vec![config.service_group.clone()]).await;
    }
    Ok("removed service identity if unused".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceAction {
    DaemonReload,
    EnableNow(&'static str),
    Start(&'static str),
    Stop(&'static str),
}

impl ServiceAction {
    fn describe(self) -> String {
        match self {
            Self::DaemonReload => "systemctl daemon-reload".to_string(),
            Self::EnableNow(service) => format!("systemctl enable --now {service}"),
            Self::Start(service) => format!("systemctl start {service}"),
            Self::Stop(service) => format!("systemctl stop {service}"),
        }
    }
}

async fn run_service_actions(
    actions: Vec<ServiceAction>,
    dry_run: bool,
) -> Result<Vec<ReportLine>, ProfileError> {
    if dry_run {
        return Ok(actions
            .into_iter()
            .map(|action| ReportLine::pass("dry-run", action.describe()))
            .collect());
    }
    let mut reports = Vec::new();
    for action in actions {
        let result = tokio::select! {
            result = async {
                match action {
                    ServiceAction::DaemonReload => systemctl_if_available(["daemon-reload"]).await,
                    ServiceAction::EnableNow(service) => {
                        systemctl_if_available(["enable", "--now", service]).await
                    }
                    ServiceAction::Start(service) => systemctl_if_available(["start", service]).await,
                    ServiceAction::Stop(service) => systemctl_if_available(["stop", service]).await,
                }
            } => result,
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() {
                    return Err(ProfileError::Interrupted);
                }
                Err(ProfileError::Interrupted)
            }
        };
        match result {
            Ok(()) => reports.push(ReportLine::pass("systemd", action.describe())),
            Err(error) => reports.push(ReportLine::fail("systemd", error.to_string())),
        }
    }
    Ok(reports)
}

async fn systemctl_if_available<const N: usize>(args: [&str; N]) -> Result<(), ProfileError> {
    if !tool_exists("systemctl") {
        return Ok(());
    }
    checked_process("systemctl", args.into_iter().map(String::from).collect()).await
}

async fn remove_agentics_docker_containers(
    config: &DgxProfileConfig,
) -> Result<String, ProfileError> {
    if config.docker_host_uri != DEFAULT_DOCKER_HOST_URI {
        return Err(ProfileError::Unsafe(format!(
            "refusing Docker cleanup on non-Agentics Docker host {}; expected {DEFAULT_DOCKER_HOST_URI}",
            config.docker_host_uri
        )));
    }
    let docker = match Docker::connect_with_host(&config.docker_host_uri) {
        Ok(docker) => docker,
        Err(error) => return Ok(format!("skipped Docker cleanup: {error}")),
    };
    let options = ListContainersOptionsBuilder::default().all(true).build();
    let containers = docker.list_containers(Some(options)).await?;
    let mut removed = 0usize;
    let mut skipped = 0usize;
    for container in containers {
        if !container
            .labels
            .as_ref()
            .is_some_and(has_agentics_container_label)
        {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let Some(id) = container.id else {
            continue;
        };
        docker
            .remove_container(
                &id,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await?;
        removed = removed.saturating_add(1);
    }
    Ok(format!(
        "removed {removed} Agentics Docker container(s); skipped {skipped} unlabelled/non-Agentics container(s)"
    ))
}

async fn remove_fstab_entries(path: &Path, roots: &[&PathBuf]) -> Result<String, ProfileError> {
    if !path.exists() {
        return Ok(format!("{} absent", path.display()));
    }
    let current = tokio::fs::read_to_string(path).await?;
    let kept = current
        .lines()
        .filter(|line| !is_agentics_fstab_entry(line, roots))
        .collect::<Vec<_>>();
    if kept.len() == current.lines().count() {
        return Ok(format!("no Agentics entries in {}", path.display()));
    }
    let backup = backup_path(path);
    tokio::fs::copy(path, &backup).await?;
    let mut next = kept.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    tokio::fs::write(path, next).await?;
    Ok(format!(
        "updated {}; backup {}",
        path.display(),
        backup.display()
    ))
}

async fn remove_lines_matching_paths(
    path: &Path,
    roots: &[&PathBuf],
) -> Result<String, ProfileError> {
    if !path.exists() {
        return Ok(format!("{} absent", path.display()));
    }
    let current = tokio::fs::read_to_string(path).await?;
    let kept = current
        .lines()
        .filter(|line| {
            !roots.iter().any(|root| {
                let needle = root.to_string_lossy();
                line.contains(needle.as_ref())
            })
        })
        .collect::<Vec<_>>();
    if kept.len() == current.lines().count() {
        return Ok(format!("no Agentics entries in {}", path.display()));
    }
    let backup = backup_path(path);
    tokio::fs::copy(path, &backup).await?;
    let mut next = kept.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    tokio::fs::write(path, next).await?;
    Ok(format!(
        "updated {}; backup {}",
        path.display(),
        backup.display()
    ))
}

fn has_agentics_container_label(labels: &std::collections::HashMap<String, String>) -> bool {
    labels
        .keys()
        .any(|key| key.starts_with("agentics.") || key.starts_with("ai.agentics."))
}

fn is_agentics_fstab_entry(line: &str, roots: &[&PathBuf]) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    let fields = trimmed.split_whitespace().collect::<Vec<_>>();
    let Some(source) = fields.first().map(PathBuf::from) else {
        return false;
    };
    let Some(target) = fields.get(1).map(PathBuf::from) else {
        return false;
    };
    let Some(fstype) = fields.get(2) else {
        return false;
    };
    let Some(options) = fields.get(3) else {
        return false;
    };
    if *fstype != "xfs" {
        return false;
    }
    let has_loop = options.split(',').any(|option| option == "loop");
    let has_project_quota = options
        .split(',')
        .any(|option| matches!(option, "prjquota" | "pquota"));
    if !has_loop || !has_project_quota {
        return false;
    }
    roots.iter().any(|root| {
        let loop_image_root = root.join("loop-images");
        let docker_data_root = root.join("docker-data-root");
        let phase_mount_root = root.join("phase-mounts");
        source.starts_with(&loop_image_root)
            && (target == docker_data_root || target.starts_with(&phase_mount_root))
    })
}

async fn unmount_tree(root: &Path) -> Result<String, ProfileError> {
    if !root.exists() {
        return Ok(format!("{} absent", root.display()));
    }
    let output = run_process(
        "findmnt",
        vec![
            "-R".to_string(),
            root.to_string_lossy().to_string(),
            "-n".to_string(),
            "-o".to_string(),
            "TARGET".to_string(),
        ],
        Some(Duration::from_secs(15)),
        DEFAULT_OUTPUT_LIMIT_BYTES,
    )
    .await?;
    if !output.success() || output.stdout.trim().is_empty() {
        return Ok(format!("no mounts under {}", root.display()));
    }
    let mut targets = output.stdout.lines().map(PathBuf::from).collect::<Vec<_>>();
    targets.sort_by_key(|target| std::cmp::Reverse(target.as_os_str().len()));
    for target in &targets {
        let normal = checked_process("umount", vec![target.to_string_lossy().to_string()]).await;
        if normal.is_err() {
            let _ignored = checked_process(
                "umount",
                vec!["-l".to_string(), target.to_string_lossy().to_string()],
            )
            .await;
        }
    }
    Ok(format!("unmounted {} mount(s)", targets.len()))
}

async fn remove_path_if_exists(path: &Path) -> Result<(), ProfileError> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        tokio::fs::remove_dir_all(path).await?;
    } else {
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

async fn command_success(program: &str, args: Vec<String>) -> Result<bool, ProfileError> {
    let output = run_process(
        program,
        args,
        Some(COMMAND_TIMEOUT),
        DEFAULT_OUTPUT_LIMIT_BYTES,
    )
    .await?;
    Ok(output.success())
}

async fn checked_process(program: &str, args: Vec<String>) -> Result<(), ProfileError> {
    let output = run_process(
        program,
        args,
        Some(COMMAND_TIMEOUT),
        DEFAULT_OUTPUT_LIMIT_BYTES,
    )
    .await?;
    if output.success() {
        Ok(())
    } else {
        Err(ProfileError::Command(format!(
            "{program} failed with {:?}: {}",
            output.status,
            output.combined()
        )))
    }
}

fn tool_exists(tool: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(tool).is_file())
}

fn backup_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    path.with_extension(format!(
        "{}.agentics-dgx-profile-backup.{stamp}",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("bak")
    ))
}

#[derive(Debug, Default)]
struct InstallRollback {
    created_paths: Vec<PathBuf>,
    file_backups: Vec<(PathBuf, Vec<u8>)>,
}

impl InstallRollback {
    async fn backup_file(&mut self, path: &Path) -> Result<(), ProfileError> {
        if self
            .file_backups
            .iter()
            .any(|(existing, _)| existing == path)
        {
            return Ok(());
        }
        let bytes = tokio::fs::read(path).await?;
        self.file_backups.push((path.to_path_buf(), bytes));
        Ok(())
    }

    async fn rollback(self) {
        for (path, bytes) in self.file_backups {
            let _ignored = tokio::fs::write(path, bytes).await;
        }
        for path in self.created_paths.iter().rev() {
            let _ignored = if path.is_dir() {
                tokio::fs::remove_dir_all(path).await
            } else {
                tokio::fs::remove_file(path).await
            };
        }
    }
}

/// DGX profile management error.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error(transparent)]
    Config(#[from] crate::dgx::DgxConfigError),
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Docker(#[from] bollard::errors::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unsafe operation: {0}")]
    Unsafe(String),
    #[error("interrupted by Ctrl-C")]
    Interrupted,
    #[error("{0}")]
    Command(String),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        InstallPlan, UninstallPlan, is_agentics_fstab_entry, remove_lines_matching_paths,
        validate_uninstall_roots,
    };
    use crate::dgx::DgxProfileConfig;

    fn config() -> DgxProfileConfig {
        DgxProfileConfig {
            service_user: "agentics".to_string(),
            service_group: "agentics".to_string(),
            config_root: "/etc/agentics".into(),
            release_root: "/opt/agentics/current".into(),
            source_root: "/opt/agentics/current".into(),
            state_root: "/srv/agentics".into(),
            test_state_root: "/srv/agentics-test".into(),
            systemd_root: "/etc/systemd/system".into(),
            docker_host_uri: "unix:///run/agentics/docker.sock".to_string(),
        }
    }

    /// Verifies install can skip storage as a separate lifecycle decision.
    #[test]
    fn install_plan_can_skip_storage() {
        let plan = InstallPlan::from_config(&config(), true);
        assert!(
            !plan
                .actions
                .iter()
                .any(|action| action.describe().contains("quota storage"))
        );
    }

    /// Verifies purge adds identity and durable path removal to uninstall.
    #[test]
    fn purge_plan_removes_identity() {
        let plan = UninstallPlan::from_config(&config(), true);
        assert!(
            plan.actions
                .iter()
                .any(|action| action.describe().contains("service user"))
        );
    }

    /// Verifies purge refuses broad release/config overrides before deletion.
    #[test]
    fn purge_root_validation_rejects_broad_overrides() {
        let mut release_config = config();
        release_config.release_root = "/opt".into();
        assert!(validate_uninstall_roots(&release_config, true).is_err());

        let mut state_config = config();
        state_config.state_root = "/srv".into();
        assert!(validate_uninstall_roots(&state_config, false).is_err());
    }

    /// Verifies fstab cleanup targets only storage-prepared loop quota mounts.
    #[test]
    fn fstab_cleanup_matches_only_loop_quota_entries() {
        let root = std::path::PathBuf::from("/srv/agentics");
        let roots = [&root];

        assert!(is_agentics_fstab_entry(
            "/srv/agentics/loop-images/phase-solution-run.xfs /srv/agentics/phase-mounts/solution-run xfs loop,prjquota,nofail 0 0",
            &roots,
        ));
        assert!(!is_agentics_fstab_entry(
            "/dev/nvme0n1 /srv/agentics xfs defaults 0 0",
            &roots,
        ));
        assert!(!is_agentics_fstab_entry(
            "/srv/agentics/loop-images/unrelated.xfs /mnt/unrelated xfs loop,prjquota 0 0",
            &roots,
        ));
    }

    /// Verifies project-file cleanup rewrites only Agentics-owned lines and writes a backup.
    #[tokio::test]
    async fn project_cleanup_backs_up_and_preserves_unrelated_lines() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let projects = tempdir.path().join("projects");
        tokio::fs::write(
            &projects,
            "100001:/srv/agentics/phase-mounts/solution-run/slots/64mb/slot-001\n2:/var/lib/other\n",
        )
        .await
        .expect("write projects");
        let root = PathBuf::from("/srv/agentics");

        let message = remove_lines_matching_paths(&projects, &[&root])
            .await
            .expect("remove project entries");

        assert!(message.contains("backup"));
        assert_eq!(
            tokio::fs::read_to_string(&projects)
                .await
                .expect("read rewritten projects"),
            "2:/var/lib/other\n",
        );
        let backups = std::fs::read_dir(tempdir.path())
            .expect("read tempdir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("projects."))
            .count();
        assert_eq!(backups, 1);
    }
}
