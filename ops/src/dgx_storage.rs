//! Rust-native DGX Spark storage preparation.
//!
//! This module oxidizes `prepare-dgx-spark-storage.sh` and
//! `prepare-dgx-spark-test-storage.sh` without editing those shell references.
//! It prepares loopback XFS images, project-quota mounts, runner runtime paths,
//! and bounded writable slot metadata. External commands are used only at OS
//! tooling boundaries where Rust does not provide stable native APIs for XFS
//! formatting, mounting, and project-quota assignment.
//!
//! Safety: destructive/rootful execution requires Linux, root, explicit
//! confirmation, and supports `--dry-run`. The implementation is idempotent and
//! tracks current-invocation file/fstab/mount changes for best-effort rollback.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use nix::unistd::Uid;

use crate::dgx::{
    self, DEFAULT_STATE_ROOT, DEFAULT_TEST_DOCKER_LOOP_SIZE, DEFAULT_TEST_PHASE_LOOP_SIZE,
    DEFAULT_TEST_STATE_ROOT, DgxStorageConfig, ENV_DGX_CONFIRM, ENV_DGX_PRODUCTION_STATE_ROOT,
    ENV_DGX_TEST_CONFIRM, ENV_DGX_TEST_DOCKER_LOOP_SIZE, ENV_DGX_TEST_GROUP,
    ENV_DGX_TEST_PERSIST_FSTAB, ENV_DGX_TEST_PHASE_LOOP_SIZE, ENV_DGX_TEST_PHASE_SLOT_CLASSES_MB,
    ENV_DGX_TEST_PHASE_SLOT_INODES_PER_MB, ENV_DGX_TEST_PHASE_SLOTS_PER_CLASS,
    ENV_DGX_TEST_STATE_ROOT, ENV_DGX_TEST_USER, STORAGE_CONFIRMATION, SlotMetadata,
    TEST_STORAGE_CONFIRMATION, phase_slot_path, slot_class_dir, slot_name,
};
use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, env_non_empty, parse_boolish,
    print_reports, require_safe_destructive_path, run_process, run_with_ctrl_c,
};

const PREFIX: &str = "agentics-dgx-storage";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// CLI for DGX storage preparation.
#[derive(Debug, Parser)]
#[command(
    about = "Prepares DGX Spark XFS project-quota storage for Agentics.",
    long_about = "Creates loopback XFS images, mounts them with project quotas, and prepares root-owned runner quota slots. Rootful execution requires AGENTICS_DGX_CONFIRM=prepare-storage or AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage. Use --dry-run to print the planned mutations."
)]
pub struct PrepareStorageCli {
    /// Print intended mutations without applying them.
    #[arg(long)]
    dry_run: bool,
}

/// CLI for isolated DGX storage test preparation.
#[derive(Debug, Parser)]
#[command(
    about = "Prepares isolated DGX Spark quota storage for integration tests.",
    long_about = "Uses the same storage library as production preparation, but defaults to AGENTICS_DGX_TEST_STATE_ROOT and refuses the production state root. Rootful execution requires AGENTICS_DGX_TEST_CONFIRM=prepare-test-storage. Use --dry-run to print the planned mutations."
)]
pub struct PrepareTestStorageCli {
    /// Print intended mutations without applying them.
    #[arg(long)]
    dry_run: bool,
}

/// Entrypoint for production storage binary.
pub async fn run_prepare_from_process() -> ExitCode {
    let cli = PrepareStorageCli::parse();
    run_with_ctrl_c(PREFIX, async {
        match prepare_storage(false, env_non_empty(ENV_DGX_CONFIRM), cli.dry_run).await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

/// Entrypoint for test storage binary.
pub async fn run_prepare_test_from_process() -> ExitCode {
    let cli = PrepareTestStorageCli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match prepare_storage(true, env_non_empty(ENV_DGX_TEST_CONFIRM), cli.dry_run).await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

/// Prepare storage, optionally using test-root defaults.
pub async fn prepare_storage(
    test_mode: bool,
    confirmation: Option<String>,
    dry_run: bool,
) -> Result<Vec<ReportLine>, StorageError> {
    require_linux_and_root(dry_run)?;
    require_confirmation(test_mode, confirmation.as_deref(), dry_run)?;
    let config = if test_mode {
        test_storage_config()?
    } else {
        DgxStorageConfig::from_env()?
    };
    validate_destructive_roots(&config, test_mode)?;
    let plan = StoragePlan::from_config(&config);
    if dry_run {
        return Ok(plan
            .actions
            .iter()
            .map(|action| ReportLine::pass("dry-run", action.describe()))
            .collect());
    }

    let mut rollback = RollbackLog::default();
    let mut reports = Vec::new();
    for action in plan.actions {
        match apply_action(&config, &action, &mut rollback).await {
            Ok(message) => reports.push(ReportLine::pass(action.label(), message)),
            Err(error) => {
                rollback.rollback().await;
                return Err(error);
            }
        }
    }
    reports.push(ReportLine::pass(
        "DGX storage",
        format!(
            "Docker data root: {}; phase mount root: {}",
            config.docker_data_root.display(),
            config.phase_mount_root.display()
        ),
    ));
    Ok(reports)
}

fn test_storage_config() -> Result<DgxStorageConfig, StorageError> {
    let invoking_user = env_non_empty(ENV_DGX_TEST_USER)
        .or_else(|| env_non_empty("SUDO_USER"))
        .or_else(|| env_non_empty("USER"))
        .unwrap_or_else(|| "root".to_string());
    let invoking_group = env_non_empty(ENV_DGX_TEST_GROUP).unwrap_or_else(|| invoking_user.clone());
    let test_state_root = dgx::env_path(ENV_DGX_TEST_STATE_ROOT, DEFAULT_TEST_STATE_ROOT);
    let production_state_root = dgx::env_path(ENV_DGX_PRODUCTION_STATE_ROOT, DEFAULT_STATE_ROOT);
    if test_state_root == production_state_root {
        return Err(StorageError::Unsafe(
            "refusing to use production state root for test storage".to_string(),
        ));
    }

    let mut config = DgxStorageConfig::from_env()?;
    config.state_root = test_state_root.clone();
    config.loop_image_root = test_state_root.join("loop-images");
    config.docker_data_root = test_state_root.join("docker-data-root");
    config.docker_loop_image = config.loop_image_root.join("docker-data-root.xfs");
    config.phase_mount_root = test_state_root.join("phase-mounts");
    config.docker_loop_size = env_non_empty(ENV_DGX_TEST_DOCKER_LOOP_SIZE)
        .unwrap_or_else(|| DEFAULT_TEST_DOCKER_LOOP_SIZE.to_string());
    config.phase_loop_size = env_non_empty(ENV_DGX_TEST_PHASE_LOOP_SIZE)
        .unwrap_or_else(|| DEFAULT_TEST_PHASE_LOOP_SIZE.to_string());
    if let Some(value) = env_non_empty(ENV_DGX_TEST_PHASE_SLOT_CLASSES_MB) {
        config.slot_classes_mb =
            dgx::parse_slot_classes(ENV_DGX_TEST_PHASE_SLOT_CLASSES_MB, &value)?;
    }
    config.slots_per_class = env_non_empty(ENV_DGX_TEST_PHASE_SLOTS_PER_CLASS)
        .as_deref()
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|error| StorageError::Unsafe(error.to_string()))?
        .unwrap_or(config.slots_per_class);
    config.slot_inodes_per_mb = env_non_empty(ENV_DGX_TEST_PHASE_SLOT_INODES_PER_MB)
        .as_deref()
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|error| StorageError::Unsafe(error.to_string()))?
        .unwrap_or(config.slot_inodes_per_mb);
    config.persist_fstab = env_non_empty(ENV_DGX_TEST_PERSIST_FSTAB)
        .as_deref()
        .map(|value| parse_boolish(ENV_DGX_TEST_PERSIST_FSTAB, value))
        .transpose()?
        .unwrap_or(false);
    config.service_user = invoking_user;
    config.service_group = invoking_group;
    Ok(config)
}

fn require_linux_and_root(dry_run: bool) -> Result<(), StorageError> {
    if !cfg!(target_os = "linux") {
        return Err(StorageError::Unsafe(format!(
            "DGX storage preparation is Linux-only; detected {}",
            std::env::consts::OS
        )));
    }
    if !dry_run && !Uid::effective().is_root() {
        return Err(StorageError::Unsafe(
            "DGX storage preparation must run as root; use sudo".to_string(),
        ));
    }
    Ok(())
}

fn require_confirmation(
    test_mode: bool,
    confirmation: Option<&str>,
    dry_run: bool,
) -> Result<(), StorageError> {
    if dry_run {
        return Ok(());
    }
    let expected = if test_mode {
        TEST_STORAGE_CONFIRMATION
    } else {
        STORAGE_CONFIRMATION
    };
    if confirmation == Some(expected) {
        Ok(())
    } else {
        Err(StorageError::Unsafe(format!(
            "refusing to prepare storage without explicit confirmation {expected:?}"
        )))
    }
}

fn validate_destructive_roots(
    config: &DgxStorageConfig,
    test_mode: bool,
) -> Result<(), StorageError> {
    let allowed = if test_mode {
        vec![config.state_root.clone()]
    } else {
        vec![config.state_root.clone(), PathBuf::from(DEFAULT_STATE_ROOT)]
    };
    for (label, path) in [
        ("state root", &config.state_root),
        ("loop image root", &config.loop_image_root),
        ("Docker data root", &config.docker_data_root),
        ("phase mount root", &config.phase_mount_root),
    ] {
        require_safe_destructive_path(path, label, &allowed)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePlan {
    actions: Vec<StorageAction>,
}

impl StoragePlan {
    pub fn from_config(config: &DgxStorageConfig) -> Self {
        let mut actions = vec![
            StorageAction::EnsureDir(config.state_root.clone()),
            StorageAction::EnsureDir(config.loop_image_root.clone()),
            StorageAction::EnsureDir(config.docker_data_root.clone()),
            StorageAction::EnsureDir(config.phase_mount_root.clone()),
            StorageAction::EnsureDir(config.state_root.join("storage")),
            StorageAction::EnsureDir(config.state_root.join("challenges")),
            StorageAction::EnsureDir(config.state_root.join("runtime")),
            StorageAction::EnsureImage(
                config.docker_loop_image.clone(),
                config.docker_loop_size.clone(),
            ),
            StorageAction::EnsureMount {
                image: config.docker_loop_image.clone(),
                mount: config.docker_data_root.clone(),
            },
        ];
        if config.persist_fstab {
            actions.push(StorageAction::EnsureFstab {
                image: config.docker_loop_image.clone(),
                mount: config.docker_data_root.clone(),
            });
        }
        for phase in &config.phases {
            let image = config.loop_image_root.join(format!("phase-{phase}.xfs"));
            let mount = config.phase_mount_root.join(phase.as_str());
            actions.push(StorageAction::EnsureImage(
                image.clone(),
                config.phase_loop_size.clone(),
            ));
            actions.push(StorageAction::EnsureMount {
                image: image.clone(),
                mount: mount.clone(),
            });
            if config.persist_fstab {
                actions.push(StorageAction::EnsureFstab {
                    image: image.clone(),
                    mount: mount.clone(),
                });
            }
            for class_mb in &config.slot_classes_mb {
                for slot_index in 1..=config.slots_per_class {
                    let class_offset = config
                        .slot_classes_mb
                        .iter()
                        .position(|value| value == class_mb)
                        .unwrap_or(0) as u64;
                    let project_id = config
                        .project_id_base
                        .saturating_add(class_offset.saturating_mul(config.slots_per_class))
                        .saturating_add(slot_index);
                    actions.push(StorageAction::EnsureSlot {
                        phase: *phase,
                        class_mb: *class_mb,
                        slot_index,
                        project_id,
                    });
                }
            }
        }
        actions.push(StorageAction::ChownOwnedPaths);
        Self { actions }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StorageAction {
    EnsureDir(PathBuf),
    EnsureImage(PathBuf, String),
    EnsureMount {
        image: PathBuf,
        mount: PathBuf,
    },
    EnsureFstab {
        image: PathBuf,
        mount: PathBuf,
    },
    EnsureSlot {
        phase: dgx::DgxPhase,
        class_mb: u64,
        slot_index: u64,
        project_id: u64,
    },
    ChownOwnedPaths,
}

impl StorageAction {
    fn label(&self) -> &'static str {
        match self {
            Self::EnsureDir(_) => "directory",
            Self::EnsureImage(_, _) => "XFS image",
            Self::EnsureMount { .. } => "mount",
            Self::EnsureFstab { .. } => "fstab",
            Self::EnsureSlot { .. } => "quota slot",
            Self::ChownOwnedPaths => "ownership",
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::EnsureDir(path) => format!("ensure directory {}", path.display()),
            Self::EnsureImage(path, size) => {
                format!("ensure XFS image {} with size {size}", path.display())
            }
            Self::EnsureMount { image, mount } => {
                format!(
                    "mount {} at {} with loop,prjquota",
                    image.display(),
                    mount.display()
                )
            }
            Self::EnsureFstab { image, mount } => {
                format!(
                    "ensure fstab entry {} -> {}",
                    image.display(),
                    mount.display()
                )
            }
            Self::EnsureSlot {
                phase,
                class_mb,
                slot_index,
                project_id,
            } => format!(
                "ensure slot {phase}/{}mb/{} project_id={project_id}",
                class_mb,
                slot_name(*slot_index)
            ),
            Self::ChownOwnedPaths => "ensure service ownership for writable roots".to_string(),
        }
    }
}

async fn apply_action(
    config: &DgxStorageConfig,
    action: &StorageAction,
    rollback: &mut RollbackLog,
) -> Result<String, StorageError> {
    match action {
        StorageAction::EnsureDir(path) => {
            if path.exists() {
                return Ok(format!("{} exists", path.display()));
            }
            tokio::fs::create_dir_all(path).await?;
            rollback.created_paths.push(path.clone());
            Ok(format!("created {}", path.display()))
        }
        StorageAction::EnsureImage(path, size) => {
            if path.exists() {
                return Ok(format!("{} exists", path.display()));
            }
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            checked_process(
                "truncate",
                vec![
                    "-s".to_string(),
                    size.clone(),
                    path.to_string_lossy().to_string(),
                ],
            )
            .await?;
            rollback.created_paths.push(path.clone());
            checked_process(
                "mkfs.xfs",
                vec!["-f".to_string(), path.to_string_lossy().to_string()],
            )
            .await?;
            Ok(format!("created XFS image {}", path.display()))
        }
        StorageAction::EnsureMount { image, mount } => {
            tokio::fs::create_dir_all(mount).await?;
            if mount_is_active(mount).await? {
                return Ok(format!("{} already mounted", mount.display()));
            }
            checked_process(
                "mount",
                vec![
                    "-o".to_string(),
                    "loop,prjquota".to_string(),
                    image.to_string_lossy().to_string(),
                    mount.to_string_lossy().to_string(),
                ],
            )
            .await?;
            rollback.mounted_paths.push(mount.clone());
            Ok(format!("mounted {}", mount.display()))
        }
        StorageAction::EnsureFstab { image, mount } => {
            let line = format!(
                "{} {} xfs loop,prjquota,nofail 0 0\n",
                image.display(),
                mount.display()
            );
            let fstab = Path::new("/etc/fstab");
            let current = tokio::fs::read_to_string(fstab).await.unwrap_or_default();
            let mount_text = mount.to_string_lossy().to_string();
            if current
                .lines()
                .any(|existing| existing.split_whitespace().nth(1) == Some(mount_text.as_str()))
            {
                return Ok(format!("fstab entry exists for {}", mount.display()));
            }
            rollback.backup_file(fstab).await?;
            tokio::fs::write(fstab, format!("{current}{line}")).await?;
            Ok(format!("added fstab entry for {}", mount.display()))
        }
        StorageAction::EnsureSlot {
            phase,
            class_mb,
            slot_index,
            project_id,
        } => {
            let mount = config.phase_mount_root.join(phase.as_str());
            let slot_path =
                phase_slot_path(&config.phase_mount_root, *phase, *class_mb, *slot_index);
            tokio::fs::create_dir_all(slot_path.parent().unwrap_or(&slot_path)).await?;
            tokio::fs::create_dir_all(&slot_path).await?;
            let metadata = SlotMetadata::new(
                *phase,
                *class_mb,
                *slot_index,
                *project_id,
                config.slot_inodes_per_mb,
            );
            checked_process(
                "xfs_quota",
                vec![
                    "-x".to_string(),
                    "-c".to_string(),
                    format!("project -s -p {} {project_id}", slot_path.display()),
                    mount.to_string_lossy().to_string(),
                ],
            )
            .await?;
            checked_process(
                "xfs_quota",
                vec![
                    "-x".to_string(),
                    "-c".to_string(),
                    format!(
                        "limit -p bhard={}m ihard={} {project_id}",
                        class_mb, metadata.inode_hard_limit
                    ),
                    mount.to_string_lossy().to_string(),
                ],
            )
            .await?;
            tokio::fs::write(
                slot_path.join(".agentics-slot.json"),
                serde_json::to_string(&metadata)?,
            )
            .await?;
            Ok(format!(
                "ensured {}",
                config
                    .phase_mount_root
                    .join(phase.as_str())
                    .join("slots")
                    .join(slot_class_dir(*class_mb))
                    .join(slot_name(*slot_index))
                    .display()
            ))
        }
        StorageAction::ChownOwnedPaths => {
            let user_group = format!("{}:{}", config.service_user, config.service_group);
            for path in [
                config.state_root.join("storage"),
                config.state_root.join("challenges"),
                config.state_root.join("runtime"),
                config.phase_mount_root.clone(),
            ] {
                if path.exists() {
                    checked_process(
                        "chown",
                        vec![
                            "-R".to_string(),
                            user_group.clone(),
                            path.to_string_lossy().to_string(),
                        ],
                    )
                    .await?;
                }
            }
            Ok(format!("applied ownership {user_group}"))
        }
    }
}

async fn checked_process(program: &str, args: Vec<String>) -> Result<(), StorageError> {
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
        Err(StorageError::Command(format!(
            "{program} failed with {:?}: {}",
            output.status,
            output.combined()
        )))
    }
}

async fn mount_is_active(mount: &Path) -> Result<bool, StorageError> {
    let output = run_process(
        "findmnt",
        vec![
            "--mountpoint".to_string(),
            mount.to_string_lossy().to_string(),
        ],
        Some(Duration::from_secs(10)),
        DEFAULT_OUTPUT_LIMIT_BYTES,
    )
    .await?;
    Ok(output.success())
}

#[derive(Debug, Default)]
struct RollbackLog {
    created_paths: Vec<PathBuf>,
    mounted_paths: Vec<PathBuf>,
    file_backups: Vec<(PathBuf, String)>,
}

impl RollbackLog {
    async fn backup_file(&mut self, path: &Path) -> Result<(), StorageError> {
        if self
            .file_backups
            .iter()
            .any(|(existing, _)| existing == path)
        {
            return Ok(());
        }
        let content = tokio::fs::read_to_string(path).await.unwrap_or_default();
        self.file_backups.push((path.to_path_buf(), content));
        Ok(())
    }

    async fn rollback(self) {
        for mount in self.mounted_paths.iter().rev() {
            let _ignored = run_process(
                "umount",
                vec![mount.to_string_lossy().to_string()],
                Some(Duration::from_secs(15)),
                DEFAULT_OUTPUT_LIMIT_BYTES,
            )
            .await;
        }
        for (path, content) in self.file_backups {
            let _ignored = tokio::fs::write(path, content).await;
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

/// Storage preparation error.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Config(#[from] crate::dgx::DgxConfigError),
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsafe operation: {0}")]
    Unsafe(String),
    #[error("{0}")]
    Command(String),
}

#[cfg(test)]
mod tests {
    use super::StoragePlan;
    use crate::dgx;

    /// Verifies storage planning covers images, mounts, and slots.
    #[test]
    fn storage_plan_contains_slot_actions() {
        let config = dgx::DgxStorageConfig {
            state_root: "/srv/agentics".into(),
            loop_image_root: "/srv/agentics/loop-images".into(),
            docker_data_root: "/srv/agentics/docker-data-root".into(),
            docker_loop_image: "/srv/agentics/loop-images/docker-data-root.xfs".into(),
            phase_mount_root: "/srv/agentics/phase-mounts".into(),
            docker_loop_size: "1G".to_string(),
            phase_loop_size: "1G".to_string(),
            service_user: "agentics".to_string(),
            service_group: "agentics".to_string(),
            phases: vec![dgx::DgxPhase::SolutionRun],
            slot_classes_mb: vec![64],
            slots_per_class: 2,
            project_id_base: 100_000,
            slot_inodes_per_mb: 256,
            persist_fstab: true,
        };
        let plan = StoragePlan::from_config(&config);
        assert!(
            plan.actions
                .iter()
                .any(|action| action.describe().contains("slot-001"))
        );
        assert!(
            plan.actions
                .iter()
                .any(|action| action.describe().contains("fstab"))
        );
    }
}
