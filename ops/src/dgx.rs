//! Typed DGX Spark operational configuration.
//!
//! DGX operational scripts historically encoded modes, phase names, paths, and
//! confirmation tokens as shell strings. This module keeps those values typed
//! inside `agentics-ops` and stringifies only at environment, process, Docker,
//! and filesystem boundaries.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use agentics_config::{
    DEFAULT_HOST_PROBE_MODE, DEFAULT_RUNNER_DOCKER_LAYER_QUOTA, DEFAULT_RUNNER_SECURITY_PROFILE,
    DEFAULT_RUNNER_WRITABLE_STORAGE_MODE, HostProbeMode, RunnerSecurityProfile,
    RunnerWritableStorageMode,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ENV_DGX_STATE_ROOT: &str = "AGENTICS_DGX_STATE_ROOT";
pub const ENV_DGX_TEST_STATE_ROOT: &str = "AGENTICS_DGX_TEST_STATE_ROOT";
pub const ENV_DGX_LOOP_IMAGE_ROOT: &str = "AGENTICS_DGX_LOOP_IMAGE_ROOT";
pub const ENV_DGX_DOCKER_DATA_ROOT: &str = "AGENTICS_DGX_DOCKER_DATA_ROOT";
pub const ENV_DGX_DOCKER_LOOP_IMAGE: &str = "AGENTICS_DGX_DOCKER_LOOP_IMAGE";
pub const ENV_DGX_PHASE_MOUNT_ROOT: &str = "AGENTICS_DGX_PHASE_MOUNT_ROOT";
pub const ENV_STORAGE_WORK_ROOT: &str = "AGENTICS_STORAGE_WORK_ROOT";
pub const ENV_DGX_DOCKER_LOOP_SIZE: &str = "AGENTICS_DGX_DOCKER_LOOP_SIZE";
pub const ENV_DGX_PHASE_LOOP_SIZE: &str = "AGENTICS_DGX_PHASE_LOOP_SIZE";
pub const ENV_DGX_PHASES: &str = "AGENTICS_DGX_PHASES";
pub const ENV_DGX_PHASE_SLOT_CLASSES_MB: &str = "AGENTICS_DGX_PHASE_SLOT_CLASSES_MB";
pub const ENV_DGX_PHASE_SLOTS_PER_CLASS: &str = "AGENTICS_DGX_PHASE_SLOTS_PER_CLASS";
pub const ENV_DGX_PHASE_PROJECT_ID_BASE: &str = "AGENTICS_DGX_PHASE_PROJECT_ID_BASE";
pub const ENV_DGX_PHASE_SLOT_INODES_PER_MB: &str = "AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB";
pub const ENV_DGX_PERSIST_FSTAB: &str = "AGENTICS_DGX_PERSIST_FSTAB";
pub const ENV_DGX_CONFIRM: &str = "AGENTICS_DGX_CONFIRM";
pub const ENV_DGX_TEST_CONFIRM: &str = "AGENTICS_DGX_TEST_CONFIRM";
pub const ENV_DGX_PRODUCTION_STATE_ROOT: &str = "AGENTICS_DGX_PRODUCTION_STATE_ROOT";
pub const ENV_DGX_TEST_DOCKER_LOOP_SIZE: &str = "AGENTICS_DGX_TEST_DOCKER_LOOP_SIZE";
pub const ENV_DGX_TEST_PHASE_LOOP_SIZE: &str = "AGENTICS_DGX_TEST_PHASE_LOOP_SIZE";
pub const ENV_DGX_TEST_PHASE_SLOT_CLASSES_MB: &str = "AGENTICS_DGX_TEST_PHASE_SLOT_CLASSES_MB";
pub const ENV_DGX_TEST_PHASE_SLOTS_PER_CLASS: &str = "AGENTICS_DGX_TEST_PHASE_SLOTS_PER_CLASS";
pub const ENV_DGX_TEST_PHASE_SLOT_INODES_PER_MB: &str =
    "AGENTICS_DGX_TEST_PHASE_SLOT_INODES_PER_MB";
pub const ENV_DGX_TEST_PERSIST_FSTAB: &str = "AGENTICS_DGX_TEST_PERSIST_FSTAB";
pub const ENV_DGX_DOCKER_PULL_POLICY: &str = "AGENTICS_DGX_DOCKER_PULL_POLICY";
pub const ENV_DGX_PROBE_IMAGE: &str = "AGENTICS_DGX_PROBE_IMAGE";
pub const ENV_DGX_PROBE_SLOT_CLASS_MB: &str = "AGENTICS_DGX_PROBE_SLOT_CLASS_MB";
pub const ENV_DGX_RUN_MUTATING_PROBES: &str = "AGENTICS_DGX_RUN_MUTATING_PROBES";
pub const ENV_DGX_RUN_DOCKER_SMOKE: &str = "AGENTICS_DGX_RUN_DOCKER_SMOKE";
pub const ENV_DGX_CUDA_IMAGE: &str = "AGENTICS_DGX_CUDA_IMAGE";
pub use crate::support::{ENV_DOCKER_HOST, ENV_DOCKER_SOCKET_PATH};
pub const ENV_RUNNER_RUNTIME_ROOT: &str = "AGENTICS_RUNNER_RUNTIME_ROOT";
pub const ENV_RUNNER_PHASE_MOUNT_ROOT: &str = "AGENTICS_RUNNER_PHASE_MOUNT_ROOT";
pub const ENV_RUNNER_WRITABLE_SLOT_CLASSES_MB: &str = "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB";
pub const ENV_RUNNER_WRITABLE_STORAGE_MODE: &str = "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE";
pub const ENV_RUNNER_DOCKER_LAYER_QUOTA: &str = "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA";
pub const ENV_RUNNER_SECURITY_PROFILE: &str = "AGENTICS_RUNNER_SECURITY_PROFILE";
pub const ENV_HOST_PROBE_MODE: &str = "AGENTICS_HOST_PROBE_MODE";
pub const ENV_RUNTIME_UID: &str = "AGENTICS_RUNTIME_UID";
pub const ENV_RUNTIME_GID: &str = "AGENTICS_RUNTIME_GID";

pub const DEFAULT_STATE_ROOT: &str = "/srv/agentics";
pub const DEFAULT_TEST_STATE_ROOT: &str = "/srv/agentics-test";
pub const DEFAULT_DOCKER_LOOP_SIZE: &str = "200G";
pub const DEFAULT_PHASE_LOOP_SIZE: &str = "20G";
pub const DEFAULT_TEST_DOCKER_LOOP_SIZE: &str = "32G";
pub const DEFAULT_TEST_PHASE_LOOP_SIZE: &str = "8G";
pub const DEFAULT_SLOT_CLASSES: &[u64] = &[64, 256, 1024, 4096];
pub const DEFAULT_PHASE_SLOTS_PER_CLASS: u64 = 100;
pub const DEFAULT_PHASE_PROJECT_ID_BASE: u64 = 100_000;
pub const DEFAULT_PHASE_SLOT_INODES_PER_MB: u64 = 256;
pub const DEFAULT_RUNTIME_UID: u32 = 10001;
pub const DEFAULT_RUNTIME_GID: u32 = 10001;
pub const DEFAULT_DOCKER_HOST_URI: &str = "unix:///var/run/docker.sock";
pub const DEFAULT_PROBE_IMAGE: &str = "busybox:1.36";
pub const DEFAULT_CUDA_IMAGE: &str = "nvidia/cuda:13.0.1-base-ubuntu24.04";
pub const DEFAULT_DGX_PERSIST_FSTAB: bool = false;
pub const DEFAULT_DGX_RUN_MUTATING_PROBES: bool = false;
pub const DEFAULT_DGX_RUN_DOCKER_SMOKE: bool = false;
pub const DEFAULT_DGX_DOCKER_PULL_POLICY: DockerPullPolicy = DockerPullPolicy::Never;
pub const DEFAULT_DGX_PROBE_SLOT_CLASS_MB: u64 = 64;
pub const DEFAULT_DGX_TEST_PERSIST_FSTAB: bool = DEFAULT_DGX_PERSIST_FSTAB;
pub const STORAGE_CONFIRMATION: &str = "prepare-storage";
pub const TEST_STORAGE_CONFIRMATION: &str = "prepare-test-storage";
pub const ALL_DGX_PHASES: &[DgxPhase] = &[
    DgxPhase::SolutionSetup,
    DgxPhase::SolutionBuild,
    DgxPhase::SolutionRun,
    DgxPhase::EvaluatorSetup,
    DgxPhase::EvaluatorScore,
];

/// Runner phase with a prepared writable slot class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DgxPhase {
    SolutionSetup,
    SolutionBuild,
    SolutionRun,
    EvaluatorSetup,
    EvaluatorScore,
}

impl DgxPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SolutionSetup => "solution-setup",
            Self::SolutionBuild => "solution-build",
            Self::SolutionRun => "solution-run",
            Self::EvaluatorSetup => "evaluator-setup",
            Self::EvaluatorScore => "evaluator-score",
        }
    }
}

impl fmt::Display for DgxPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DgxPhase {
    type Err = DgxConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "solution-setup" => Ok(Self::SolutionSetup),
            "solution-build" => Ok(Self::SolutionBuild),
            "solution-run" => Ok(Self::SolutionRun),
            "evaluator-setup" => Ok(Self::EvaluatorSetup),
            "evaluator-score" => Ok(Self::EvaluatorScore),
            other => Err(DgxConfigError::InvalidValue {
                field: ENV_DGX_PHASES,
                value: other.to_string(),
                message: "expected one of solution-setup, solution-build, solution-run, evaluator-setup, evaluator-score".to_string(),
            }),
        }
    }
}

/// Docker pull behavior for DGX probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockerPullPolicy {
    Never,
    Missing,
    Always,
}

impl DockerPullPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Missing => "missing",
            Self::Always => "always",
        }
    }
}

impl FromStr for DockerPullPolicy {
    type Err = DgxConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "never" => Ok(Self::Never),
            "missing" => Ok(Self::Missing),
            "always" => Ok(Self::Always),
            other => Err(DgxConfigError::InvalidValue {
                field: ENV_DGX_DOCKER_PULL_POLICY,
                value: other.to_string(),
                message: "expected never, missing, or always".to_string(),
            }),
        }
    }
}

/// Slot metadata stored in each root-prepared quota slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotMetadata {
    pub phase: DgxPhase,
    pub slot_class_mb: u64,
    pub slot_index: u64,
    pub project_id: u64,
    pub inodes_per_mb: u64,
    pub inode_hard_limit: u64,
}

impl SlotMetadata {
    /// Construct metadata from typed phase and slot parameters.
    pub fn new(
        phase: DgxPhase,
        slot_class_mb: u64,
        slot_index: u64,
        project_id: u64,
        inodes_per_mb: u64,
    ) -> Self {
        Self {
            phase,
            slot_class_mb,
            slot_index,
            project_id,
            inodes_per_mb,
            inode_hard_limit: slot_class_mb.saturating_mul(inodes_per_mb),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawDgxStorageEnv {
    dgx_state_root: Option<String>,
    dgx_loop_image_root: Option<String>,
    dgx_docker_data_root: Option<String>,
    dgx_docker_loop_image: Option<String>,
    dgx_phase_mount_root: Option<String>,
    storage_work_root: Option<String>,
    dgx_docker_loop_size: Option<String>,
    dgx_phase_loop_size: Option<String>,
    runtime_uid: Option<u32>,
    runtime_gid: Option<u32>,
    dgx_phases: Option<String>,
    dgx_phase_slot_classes_mb: Option<String>,
    runner_writable_slot_classes_mb: Option<String>,
    dgx_phase_slots_per_class: Option<u64>,
    dgx_phase_project_id_base: Option<u64>,
    dgx_phase_slot_inodes_per_mb: Option<u64>,
    dgx_persist_fstab: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawDgxProfileEnv {
    dgx_state_root: Option<String>,
    dgx_docker_data_root: Option<String>,
    dgx_phase_mount_root: Option<String>,
    runner_runtime_root: Option<String>,
    storage_work_root: Option<String>,
    runner_security_profile: Option<RunnerSecurityProfile>,
    runner_writable_storage_mode: Option<RunnerWritableStorageMode>,
    runner_phase_mount_root: Option<String>,
    runner_writable_slot_classes_mb: Option<String>,
    runner_docker_layer_quota: Option<bool>,
    dgx_phase_slot_inodes_per_mb: Option<u64>,
    dgx_phase_slots_per_class: Option<u64>,
    docker_host: Option<String>,
    docker_socket_path: Option<String>,
    dgx_probe_image: Option<String>,
    dgx_docker_pull_policy: Option<DockerPullPolicy>,
    dgx_phases: Option<String>,
    dgx_probe_slot_class_mb: Option<u64>,
    dgx_run_mutating_probes: Option<bool>,
    host_probe_mode: Option<HostProbeMode>,
}

/// Common DGX storage layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DgxStorageConfig {
    pub state_root: PathBuf,
    pub loop_image_root: PathBuf,
    pub docker_data_root: PathBuf,
    pub docker_loop_image: PathBuf,
    pub phase_mount_root: PathBuf,
    pub storage_work_root: PathBuf,
    pub docker_loop_size: String,
    pub phase_loop_size: String,
    pub runtime_uid: u32,
    pub runtime_gid: u32,
    pub phases: Vec<DgxPhase>,
    pub slot_classes_mb: Vec<u64>,
    pub slots_per_class: u64,
    pub project_id_base: u64,
    pub slot_inodes_per_mb: u64,
    pub persist_fstab: bool,
}

impl DgxStorageConfig {
    /// Resolve production DGX storage configuration from environment.
    pub fn from_env() -> Result<Self, DgxConfigError> {
        let raw = load_agentics_env::<RawDgxStorageEnv>()?;
        let state_root = path_or_default(raw.dgx_state_root, DEFAULT_STATE_ROOT);
        let loop_image_root = path_or_join(
            raw.dgx_loop_image_root,
            &state_root,
            Path::new("loop-images"),
        );
        let docker_data_root = path_or_join(
            raw.dgx_docker_data_root,
            &state_root,
            Path::new("docker-data-root"),
        );
        let docker_loop_image = path_or_join(
            raw.dgx_docker_loop_image,
            &loop_image_root,
            Path::new("docker-data-root.xfs"),
        );
        let phase_mount_root = path_or_join(
            raw.dgx_phase_mount_root,
            &state_root,
            Path::new("phase-mounts"),
        );
        let storage_work_root = path_or_join(
            raw.storage_work_root,
            &state_root,
            Path::new("storage-work"),
        );

        Ok(Self {
            state_root,
            loop_image_root,
            docker_data_root,
            docker_loop_image,
            phase_mount_root,
            storage_work_root,
            docker_loop_size: string_or_default(raw.dgx_docker_loop_size, DEFAULT_DOCKER_LOOP_SIZE),
            phase_loop_size: string_or_default(raw.dgx_phase_loop_size, DEFAULT_PHASE_LOOP_SIZE),
            runtime_uid: positive_or_default(
                ENV_RUNTIME_UID,
                raw.runtime_uid,
                DEFAULT_RUNTIME_UID,
            )?,
            runtime_gid: positive_or_default(
                ENV_RUNTIME_GID,
                raw.runtime_gid,
                DEFAULT_RUNTIME_GID,
            )?,
            phases: parse_optional_phases(raw.dgx_phases)?,
            slot_classes_mb: parse_slot_classes_optional(
                ENV_DGX_PHASE_SLOT_CLASSES_MB,
                raw.dgx_phase_slot_classes_mb.as_deref(),
                raw.runner_writable_slot_classes_mb.as_deref(),
            )?,
            slots_per_class: positive_or_default(
                ENV_DGX_PHASE_SLOTS_PER_CLASS,
                raw.dgx_phase_slots_per_class,
                DEFAULT_PHASE_SLOTS_PER_CLASS,
            )?,
            project_id_base: positive_or_default(
                ENV_DGX_PHASE_PROJECT_ID_BASE,
                raw.dgx_phase_project_id_base,
                DEFAULT_PHASE_PROJECT_ID_BASE,
            )?,
            slot_inodes_per_mb: positive_or_default(
                ENV_DGX_PHASE_SLOT_INODES_PER_MB,
                raw.dgx_phase_slot_inodes_per_mb,
                DEFAULT_PHASE_SLOT_INODES_PER_MB,
            )?,
            persist_fstab: raw.dgx_persist_fstab.unwrap_or(DEFAULT_DGX_PERSIST_FSTAB),
        })
    }
}

/// DGX profile validation config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DgxProfileCheckConfig {
    pub host_probe_mode: HostProbeMode,
    pub state_root: PathBuf,
    pub docker_data_root: PathBuf,
    pub phase_mount_root: PathBuf,
    pub runner_runtime_root: PathBuf,
    pub storage_work_root: PathBuf,
    pub runner_security_profile: RunnerSecurityProfile,
    pub runner_storage_mode: RunnerWritableStorageMode,
    pub runner_phase_mount_root: PathBuf,
    pub runner_slot_classes_mb: Vec<u64>,
    pub runner_docker_layer_quota: bool,
    pub phase_slot_inodes_per_mb: u64,
    pub phase_slots_per_class: u64,
    pub docker_host_uri: String,
    pub expected_docker_host_uri: String,
    pub probe_image: String,
    pub pull_policy: DockerPullPolicy,
    pub phases: Vec<DgxPhase>,
    pub slot_probe_class_mb: u64,
    pub run_mutating_probes: bool,
}

impl DgxProfileCheckConfig {
    /// Resolve profile check config from environment.
    pub fn from_env() -> Result<Self, DgxConfigError> {
        let raw = load_agentics_env::<RawDgxProfileEnv>()?;
        let state_root = path_or_default(raw.dgx_state_root, DEFAULT_STATE_ROOT);
        let phase_mount_root = path_or_join(
            raw.dgx_phase_mount_root,
            &state_root,
            Path::new("phase-mounts"),
        );
        let host_probe_mode = raw.host_probe_mode.unwrap_or(DEFAULT_HOST_PROBE_MODE);
        let runner_security_profile = raw
            .runner_security_profile
            .unwrap_or(DEFAULT_RUNNER_SECURITY_PROFILE);
        let runner_storage_mode = raw
            .runner_writable_storage_mode
            .unwrap_or(DEFAULT_RUNNER_WRITABLE_STORAGE_MODE);
        Ok(Self {
            host_probe_mode,
            docker_data_root: path_or_join(
                raw.dgx_docker_data_root,
                &state_root,
                Path::new("docker-data-root"),
            ),
            runner_runtime_root: path_or_join(
                raw.runner_runtime_root,
                &state_root,
                Path::new("runtime"),
            ),
            storage_work_root: path_or_join(
                raw.storage_work_root,
                &state_root,
                Path::new("storage-work"),
            ),
            runner_security_profile,
            runner_storage_mode,
            runner_phase_mount_root: raw
                .runner_phase_mount_root
                .map(PathBuf::from)
                .unwrap_or_else(|| phase_mount_root.clone()),
            runner_slot_classes_mb: parse_slot_classes_optional(
                ENV_RUNNER_WRITABLE_SLOT_CLASSES_MB,
                raw.runner_writable_slot_classes_mb.as_deref(),
                None,
            )?,
            runner_docker_layer_quota: raw
                .runner_docker_layer_quota
                .unwrap_or(DEFAULT_RUNNER_DOCKER_LAYER_QUOTA),
            phase_slot_inodes_per_mb: positive_or_default(
                ENV_DGX_PHASE_SLOT_INODES_PER_MB,
                raw.dgx_phase_slot_inodes_per_mb,
                DEFAULT_PHASE_SLOT_INODES_PER_MB,
            )?,
            phase_slots_per_class: positive_or_default(
                ENV_DGX_PHASE_SLOTS_PER_CLASS,
                raw.dgx_phase_slots_per_class,
                DEFAULT_PHASE_SLOTS_PER_CLASS,
            )?,
            docker_host_uri: string_or_default(raw.docker_host, DEFAULT_DOCKER_HOST_URI),
            expected_docker_host_uri: expected_profile_docker_host_uri(
                raw.docker_socket_path.as_deref(),
            ),
            probe_image: string_or_default(raw.dgx_probe_image, DEFAULT_PROBE_IMAGE),
            pull_policy: raw
                .dgx_docker_pull_policy
                .unwrap_or(DEFAULT_DGX_DOCKER_PULL_POLICY),
            phases: parse_optional_phases(raw.dgx_phases)?,
            slot_probe_class_mb: positive_or_default(
                ENV_DGX_PROBE_SLOT_CLASS_MB,
                raw.dgx_probe_slot_class_mb,
                DEFAULT_DGX_PROBE_SLOT_CLASS_MB,
            )?,
            run_mutating_probes: raw
                .dgx_run_mutating_probes
                .unwrap_or(DEFAULT_DGX_RUN_MUTATING_PROBES),
            phase_mount_root,
            state_root,
        })
    }
}

/// Resolve the Docker host URI expected by the profile checker.
pub fn expected_profile_docker_host_uri(socket_path: Option<&str>) -> String {
    socket_path
        .map(docker_host_uri_for_socket_path)
        .unwrap_or_else(|| DEFAULT_DOCKER_HOST_URI.to_string())
}

/// Convert a Unix Docker socket path into the URI format accepted by Bollard.
pub fn docker_host_uri_for_socket_path(socket_path: &str) -> String {
    format!("unix://{socket_path}")
}

fn load_agentics_env<T>() -> Result<T, DgxConfigError>
where
    T: DeserializeOwned,
{
    envy::prefixed("AGENTICS_")
        .from_env::<T>()
        .map_err(|error| DgxConfigError::InvalidValue {
            field: "AGENTICS_*",
            value: "process environment".to_string(),
            message: error.to_string(),
        })
}

fn string_or_default(value: Option<String>, default: &str) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn path_or_default(value: Option<String>, default: &str) -> PathBuf {
    PathBuf::from(string_or_default(value, default))
}

fn path_or_join(value: Option<String>, root: &Path, child: &Path) -> PathBuf {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(child))
}

fn parse_optional_phases(value: Option<String>) -> Result<Vec<DgxPhase>, DgxConfigError> {
    match value {
        Some(value) if !value.trim().is_empty() => parse_phases(&value),
        Some(value) => Err(DgxConfigError::InvalidValue {
            field: ENV_DGX_PHASES,
            value,
            message: "must contain at least one phase".to_string(),
        }),
        None => Ok(ALL_DGX_PHASES.to_vec()),
    }
}

fn parse_slot_classes_optional(
    field: &'static str,
    value: Option<&str>,
    fallback: Option<&str>,
) -> Result<Vec<u64>, DgxConfigError> {
    match value.or(fallback) {
        Some(value) => parse_slot_classes(field, value),
        None => Ok(DEFAULT_SLOT_CLASSES.to_vec()),
    }
}

fn positive_or_default<T>(
    field: &'static str,
    value: Option<T>,
    default: T,
) -> Result<T, DgxConfigError>
where
    T: PartialOrd + From<u8> + Copy + fmt::Display,
{
    let Some(value) = value else {
        return Ok(default);
    };
    if value <= T::from(0) {
        return Err(DgxConfigError::InvalidValue {
            field,
            value: value.to_string(),
            message: "must be greater than zero".to_string(),
        });
    }
    Ok(value)
}

pub fn parse_phases(value: &str) -> Result<Vec<DgxPhase>, DgxConfigError> {
    let mut phases = Vec::new();
    for raw in value.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let phase = raw.parse::<DgxPhase>()?;
        if !phases.contains(&phase) {
            phases.push(phase);
        }
    }
    if phases.is_empty() {
        return Err(DgxConfigError::InvalidValue {
            field: ENV_DGX_PHASES,
            value: value.to_string(),
            message: "must contain at least one phase".to_string(),
        });
    }
    Ok(phases)
}

pub fn parse_slot_classes(field: &'static str, value: &str) -> Result<Vec<u64>, DgxConfigError> {
    let mut classes = Vec::new();
    for raw in value.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let parsed = raw
            .parse::<u64>()
            .map_err(|error| DgxConfigError::InvalidValue {
                field,
                value: raw.to_string(),
                message: error.to_string(),
            })?;
        if parsed == 0 {
            return Err(DgxConfigError::InvalidValue {
                field,
                value: raw.to_string(),
                message: "slot class must be greater than zero".to_string(),
            });
        }
        classes.push(parsed);
    }
    classes.sort_unstable();
    classes.dedup();
    if classes.is_empty() {
        return Err(DgxConfigError::InvalidValue {
            field,
            value: value.to_string(),
            message: "must contain at least one slot class".to_string(),
        });
    }
    Ok(classes)
}

pub fn slot_name(slot_index: u64) -> String {
    format!("slot-{slot_index:03}")
}

pub fn slot_class_dir(class_mb: u64) -> String {
    format!("{class_mb}mb")
}

pub fn phase_slot_path(
    phase_mount_root: &Path,
    phase: DgxPhase,
    class_mb: u64,
    slot_index: u64,
) -> PathBuf {
    phase_mount_root
        .join(phase.as_str())
        .join("slots")
        .join(slot_class_dir(class_mb))
        .join(slot_name(slot_index))
}

/// Error while resolving typed DGX config.
#[derive(Debug, Error)]
pub enum DgxConfigError {
    #[error("invalid {field} value {value:?}: {message}")]
    InvalidValue {
        field: &'static str,
        value: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_DOCKER_HOST_URI, DgxPhase, DockerPullPolicy, RawDgxProfileEnv, SlotMetadata,
        docker_host_uri_for_socket_path, expected_profile_docker_host_uri, parse_phases,
        parse_slot_classes, slot_name,
    };
    use agentics_config::{HostProbeMode, RunnerSecurityProfile};

    /// Verifies phase parsing is typed and deduplicated.
    #[test]
    fn parses_phases_as_typed_values() {
        let phases = parse_phases("solution-run, evaluator-score solution-run").unwrap();
        assert_eq!(
            phases,
            vec![DgxPhase::SolutionRun, DgxPhase::EvaluatorScore]
        );
        assert!(parse_phases("unknown").is_err());
    }

    /// Verifies slot class parsing sorts and rejects zero.
    #[test]
    fn parses_slot_classes() {
        assert_eq!(
            parse_slot_classes("x", "1024,64 256,64").unwrap(),
            vec![64, 256, 1024]
        );
        assert!(parse_slot_classes("x", "0").is_err());
    }

    /// Verifies pull policy is an enum, not a free-form string.
    #[test]
    fn parses_docker_pull_policy() {
        assert_eq!(
            "never".parse::<DockerPullPolicy>().unwrap(),
            DockerPullPolicy::Never
        );
        assert!("sometimes".parse::<DockerPullPolicy>().is_err());
    }

    /// Verifies DGX env loading uses typed scalar deserialization.
    #[test]
    fn profile_env_uses_typed_scalar_deserialization() {
        let raw = envy::prefixed("AGENTICS_")
            .from_iter::<_, RawDgxProfileEnv>([
                (
                    "AGENTICS_DGX_RUN_MUTATING_PROBES".to_string(),
                    "false".to_string(),
                ),
                (
                    "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA".to_string(),
                    "true".to_string(),
                ),
                (
                    "AGENTICS_RUNNER_SECURITY_PROFILE".to_string(),
                    "production".to_string(),
                ),
                (
                    "AGENTICS_DGX_DOCKER_PULL_POLICY".to_string(),
                    "missing".to_string(),
                ),
                (
                    "AGENTICS_HOST_PROBE_MODE".to_string(),
                    "require".to_string(),
                ),
            ])
            .expect("typed env values should deserialize");
        assert_eq!(raw.dgx_run_mutating_probes, Some(false));
        assert_eq!(raw.runner_docker_layer_quota, Some(true));
        assert_eq!(
            raw.runner_security_profile,
            Some(RunnerSecurityProfile::Production)
        );
        assert_eq!(raw.dgx_docker_pull_policy, Some(DockerPullPolicy::Missing));
        assert_eq!(raw.host_probe_mode, Some(HostProbeMode::Require));

        let error = envy::prefixed("AGENTICS_")
            .from_iter::<_, RawDgxProfileEnv>([(
                "AGENTICS_DGX_RUN_MUTATING_PROBES".to_string(),
                "1".to_string(),
            )])
            .expect_err("legacy bool-ish alias should fail during raw env parsing");
        assert!(error.to_string().contains("DGX_RUN_MUTATING_PROBES"));
    }

    /// Verifies the profile checker can expect the production Compose host socket.
    #[test]
    fn resolves_expected_profile_docker_host_uri() {
        assert_eq!(
            expected_profile_docker_host_uri(None),
            DEFAULT_DOCKER_HOST_URI
        );
        assert_eq!(
            expected_profile_docker_host_uri(Some("/var/run/docker.sock")),
            "unix:///var/run/docker.sock"
        );
        assert_eq!(
            docker_host_uri_for_socket_path("/tmp/agentics-docker.sock"),
            "unix:///tmp/agentics-docker.sock"
        );
    }

    /// Verifies slot metadata derives expected inode limit.
    #[test]
    fn slot_metadata_derives_inode_limit() {
        let metadata = SlotMetadata::new(DgxPhase::SolutionRun, 64, 1, 100_001, 256);
        assert_eq!(metadata.phase, DgxPhase::SolutionRun);
        let value = serde_json::to_value(&metadata).unwrap();
        assert_eq!(value.get("phase"), Some(&serde_json::json!("solution-run")));
        assert_eq!(metadata.inode_hard_limit, 16_384);
        assert_eq!(slot_name(7), "slot-007");
    }
}
