use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::config::{Config, RunnerWritableStorageMode};
use crate::error::{AppError, Result};
use crate::zip_project::ZipProjectPhaseLimits;

#[derive(Debug, Clone)]
/// Carries runner storage data across this module boundary.
pub(super) struct RunnerStorage {
    mode: RunnerWritableStorageMode,
    phase_mount_root: Option<PathBuf>,
    slot_classes_mb: Vec<u64>,
    docker_layer_quota: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates writable phase variants supported by this module.
pub(super) enum WritablePhase {
    SolutionSetup,
    SolutionBuild,
    SolutionRun,
    ScorerPrepare,
    ScorerScore,
}

#[derive(Debug)]
/// Enumerates writable mount lease variants supported by this module.
pub(super) enum WritableMountLease {
    Unbounded(PathBuf),
    Bounded(BoundedSlotLease),
}

#[derive(Debug)]
/// Carries bounded slot lease data across this module boundary.
pub(super) struct BoundedSlotLease {
    work_path: PathBuf,
    _lock_file: fs::File,
}

impl RunnerStorage {
    /// Handles from config for this module.
    pub(super) fn from_config(config: &Config) -> Result<Self> {
        Ok(Self {
            mode: config
                .runner_writable_storage_mode()
                .map_err(|e| AppError::Runner(e.to_string()))?,
            phase_mount_root: config.runner_phase_mount_root.as_ref().map(PathBuf::from),
            slot_classes_mb: config
                .runner_writable_slot_classes_mb()
                .map_err(|e| AppError::Runner(e.to_string()))?,
            docker_layer_quota: config.runner_docker_layer_quota,
        })
    }

    /// Handles docker layer quota mb for this module.
    pub(super) fn docker_layer_quota_mb(&self, limits: &ZipProjectPhaseLimits) -> Option<u64> {
        self.docker_layer_quota.then_some(limits.disk_limit_mb)
    }

    /// Handles uses bounded slots for this module.
    pub(super) fn uses_bounded_slots(&self) -> bool {
        self.mode == RunnerWritableStorageMode::XfsProjectQuotaSlots
    }

    /// Handles writable mount for this module.
    pub(super) async fn writable_mount(
        &self,
        fallback_path: &Path,
        phase: WritablePhase,
        disk_limit_mb: u64,
    ) -> Result<WritableMountLease> {
        match self.mode {
            RunnerWritableStorageMode::Unbounded => {
                tokio::fs::create_dir_all(fallback_path).await?;
                Ok(WritableMountLease::Unbounded(fallback_path.to_path_buf()))
            }
            RunnerWritableStorageMode::XfsProjectQuotaSlots => {
                let phase_mount_root = self.phase_mount_root.as_ref().ok_or_else(|| {
                    AppError::Runner(
                        "AGENTICS_RUNNER_PHASE_MOUNT_ROOT must be configured".to_string(),
                    )
                })?;
                let slot_class_mb = choose_slot_class(&self.slot_classes_mb, disk_limit_mb)?;
                acquire_slot(phase_mount_root, phase, slot_class_mb).await
            }
        }
    }
}

impl WritableMountLease {
    /// Handles path for this module.
    pub(super) fn path(&self) -> &Path {
        match self {
            Self::Unbounded(path) => path,
            Self::Bounded(lease) => lease.path(),
        }
    }
}

impl BoundedSlotLease {
    /// Handles path for this module.
    fn path(&self) -> &Path {
        &self.work_path
    }
}

impl Drop for BoundedSlotLease {
    /// Handles drop for this module.
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.work_path)
            && error.kind() != ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.work_path.display(),
                error = %error,
                "failed to clean bounded runner slot work path"
            );
        }
    }
}

impl WritablePhase {
    /// Handles dir name for this module.
    fn dir_name(self) -> &'static str {
        match self {
            Self::SolutionSetup => "solution-setup",
            Self::SolutionBuild => "solution-build",
            Self::SolutionRun => "solution-run",
            Self::ScorerPrepare => "scorer-prepare",
            Self::ScorerScore => "scorer-score",
        }
    }
}

/// Handles choose slot class for this module.
fn choose_slot_class(classes: &[u64], disk_limit_mb: u64) -> Result<u64> {
    classes
        .iter()
        .copied()
        .find(|class_mb| *class_mb >= disk_limit_mb)
        .ok_or_else(|| {
            AppError::Runner(format!(
                "no bounded writable slot class can satisfy {disk_limit_mb} MiB; configure AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB and rerun DGX storage preparation"
            ))
        })
}

/// Handles acquire slot for this module.
async fn acquire_slot(
    phase_mount_root: &Path,
    phase: WritablePhase,
    slot_class_mb: u64,
) -> Result<WritableMountLease> {
    let slot_class_root = phase_mount_root
        .join(phase.dir_name())
        .join("slots")
        .join(format!("{slot_class_mb}mb"));
    let phase_label = phase.dir_name().to_string();
    let lease = tokio::task::spawn_blocking(move || {
        acquire_slot_blocking(&slot_class_root, &phase_label, slot_class_mb)
    })
    .await
    .map_err(|e| AppError::Internal(format!("bounded slot acquisition task failed: {e}")))??;
    Ok(WritableMountLease::Bounded(lease))
}

/// Handles acquire slot blocking for this module.
fn acquire_slot_blocking(
    slot_class_root: &Path,
    phase_label: &str,
    slot_class_mb: u64,
) -> Result<BoundedSlotLease> {
    let slots = list_slot_dirs(slot_class_root)?;
    if slots.is_empty() {
        return Err(AppError::Runner(format!(
            "no bounded writable slots found for phase `{phase_label}` class {slot_class_mb} MiB at {}",
            slot_class_root.display()
        )));
    }

    for slot_path in slots {
        let lock_path = slot_path.join(".agentics-slot.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        match lock_file.try_lock_exclusive() {
            Ok(()) => {
                let work_path = slot_path.join("work");
                if let Err(error) = fs::remove_dir_all(&work_path)
                    && error.kind() != ErrorKind::NotFound
                {
                    return Err(AppError::Io(error));
                }
                fs::create_dir_all(&work_path)?;
                return Ok(BoundedSlotLease {
                    work_path,
                    _lock_file: lock_file,
                });
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(AppError::Io(error)),
        }
    }

    Err(AppError::Runner(format!(
        "all bounded writable slots are busy for phase `{phase_label}` class {slot_class_mb} MiB"
    )))
}

/// Lists slot dirs using the configured query scope.
fn list_slot_dirs(slot_class_root: &Path) -> Result<Vec<PathBuf>> {
    let mut slots = Vec::new();
    let entries = fs::read_dir(slot_class_root).map_err(|error| {
        AppError::Runner(format!(
            "bounded writable slot class directory is missing or unreadable at {}: {error}",
            slot_class_root.display()
        ))
    })?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() && entry.file_name().to_string_lossy().starts_with("slot-") {
            slots.push(path);
        }
    }
    slots.sort();
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Handles temp path for this module.
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-storage-{name}-{}", uuid::Uuid::new_v4()))
    }

    /// Verifies that chooses smallest sufficient slot class.
    #[test]
    fn chooses_smallest_sufficient_slot_class() {
        assert_eq!(choose_slot_class(&[64, 256, 1024], 64).unwrap(), 64);
        assert_eq!(choose_slot_class(&[64, 256, 1024], 65).unwrap(), 256);
        assert_eq!(choose_slot_class(&[64, 256, 1024], 1024).unwrap(), 1024);
    }

    /// Verifies that rejects limits without slot class.
    #[test]
    fn rejects_limits_without_slot_class() {
        let result = choose_slot_class(&[64, 256], 1024);
        assert!(
            matches!(result, Err(AppError::Runner(message)) if message.contains("no bounded writable slot class"))
        );
    }

    /// Verifies that bounded slot lease creates and cleans work path.
    #[test]
    fn bounded_slot_lease_creates_and_cleans_work_path() {
        let root = temp_path("lease");
        let slot = root.join("slot-001");
        fs::create_dir_all(&slot).expect("failed to create slot");
        fs::create_dir_all(slot.join("work")).expect("failed to create stale work path");
        fs::write(slot.join("work").join("stale.txt"), b"stale")
            .expect("failed to write stale file");

        {
            let lease =
                acquire_slot_blocking(&root, "solution-run", 64).expect("slot should be acquired");
            assert!(lease.path().is_dir());
            fs::write(lease.path().join("probe.txt"), b"ok").expect("failed to write probe");
        }

        assert!(!slot.join("work").exists());
        drop(fs::remove_dir_all(root));
    }
}
