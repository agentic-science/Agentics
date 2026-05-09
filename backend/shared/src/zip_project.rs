//! `zip_project` solution submission protocol schema.
//!
//! This module defines manifest parsing plus the setup/build/run phase model
//! that the worker will consume in later execution milestones.

use std::collections::HashSet;
use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub const ZIP_PROJECT_MANIFEST_FILE: &str = "agentics.solution.json";
pub const ZIP_PROJECT_PROTOCOL: &str = "zip_project";
pub const ZIP_PROJECT_PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_SETUP_TIMEOUT_SEC: u64 = 300;
pub const DEFAULT_BUILD_TIMEOUT_SEC: u64 = 600;
pub const DEFAULT_RUN_TIMEOUT_SEC: u64 = 30;
pub const DEFAULT_PHASE_MEMORY_LIMIT_MB: u64 = 512;
pub const DEFAULT_PHASE_CPU_LIMIT_MILLIS: u32 = 1000;
pub const DEFAULT_PHASE_DISK_LIMIT_MB: u64 = 1024;
pub const DEFAULT_PHASE_LOG_LIMIT_BYTES: u64 = 1024 * 1024;
pub const MAX_ZIP_PROJECT_ARTIFACT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_ZIP_PROJECT_FILE_COUNT: usize = 256;
pub const MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024;

/// Parsed `agentics.solution.json` manifest for a ZIP project solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectManifest {
    pub protocol: String,
    pub protocol_version: u16,
    pub runtime: ZipProjectRuntime,
    pub commands: ZipProjectCommands,
    #[serde(default)]
    pub phases: ZipProjectPhases,
    pub interface: ZipProjectInterface,
    pub dependencies: ZipProjectDependencies,
}

/// Language and runtime metadata declared by the submitting solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectRuntime {
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_profile: Option<String>,
}

/// Script paths used by the future setup/build/run phase executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectCommands {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    pub run: String,
}

/// Optional per-phase resource and behavior overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectPhases {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<ZipProjectPhaseConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<ZipProjectPhaseConfig>,
    #[serde(default)]
    pub run: ZipProjectPhaseConfig,
}

/// A partial phase limit override from the manifest.
///
/// Missing values are resolved from Agentics protocol defaults. Runtime
/// configuration and challenge resource profiles can still clamp these values
/// in later worker/resource milestones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectPhaseConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_limit_millis: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_limit_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_access: Option<ZipProjectNetworkAccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_limit_bytes: Option<u64>,
}

/// Concrete limits for one execution phase after defaults are applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ZipProjectPhaseLimits {
    pub timeout_sec: u64,
    pub memory_limit_mb: u64,
    pub cpu_limit_millis: u32,
    pub disk_limit_mb: u64,
    pub network_access: ZipProjectNetworkAccess,
    pub log_limit_bytes: u64,
}

/// Network access policy requested for a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectNetworkAccess {
    Disabled,
    Loopback,
    Enabled,
}

impl ZipProjectNetworkAccess {
    /// Docker network mode used by the v0.2 runner.
    pub fn docker_network_mode(self) -> &'static str {
        match self {
            Self::Disabled | Self::Loopback => "none",
            Self::Enabled => "bridge",
        }
    }

    /// Clamp a requested phase policy to a challenge-owned maximum policy.
    pub fn clamp_to(self, maximum: Self) -> Self {
        if self.rank() <= maximum.rank() {
            self
        } else {
            maximum
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Disabled => 0,
            Self::Loopback => 1,
            Self::Enabled => 2,
        }
    }
}

/// Ordered phase names in the `zip_project` execution model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectPhaseName {
    Setup,
    Build,
    Run,
}

/// One executable phase after command paths and phase limits are resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ZipProjectResolvedPhase {
    pub name: ZipProjectPhaseName,
    pub command: String,
    pub limits: ZipProjectPhaseLimits,
}

/// Structured failure payload for phase-specific execution errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectPhaseFailureReport {
    pub phase: ZipProjectPhaseName,
    pub reason: ZipProjectPhaseFailureReason,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
}

/// Coarse failure classes used by workers when reporting phase outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectPhaseFailureReason {
    NonZeroExit,
    TimedOut,
    ResourceLimit,
    MissingCommand,
    RunnerError,
}

/// How the challenge harness should invoke and communicate with the solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectInterface {
    pub kind: ZipProjectInterfaceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_contract: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contract: Option<String>,
}

/// Invocation styles that a challenge harness may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectInterfaceKind {
    ChallengeDefined,
    Argv,
    Stdio,
    FileSystem,
    Http,
}

/// Declared dependency strategy for the submitted ZIP project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectDependencies {
    pub policy: ZipProjectDependencyPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lockfiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vendor_dirs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Dependency source policy declared by the solution author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectDependencyPolicy {
    Vendored,
    Lockfile,
    ImageProvided,
}

/// Parse and validate a manifest JSON payload.
pub fn parse_zip_project_manifest(raw: &str) -> Result<ZipProjectManifest> {
    let manifest: ZipProjectManifest = serde_json::from_str(raw)
        .map_err(|e| AppError::Validation(format!("invalid {ZIP_PROJECT_MANIFEST_FILE}: {e}")))?;
    manifest.validate()?;
    Ok(manifest)
}

/// Parse and validate `agentics.solution.json` directly from a ZIP artifact.
pub fn parse_zip_project_manifest_from_zip_bytes(bytes: &[u8]) -> Result<ZipProjectManifest> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut manifest = archive
        .by_name(ZIP_PROJECT_MANIFEST_FILE)
        .map_err(|_| AppError::Validation(format!("{ZIP_PROJECT_MANIFEST_FILE} is required")))?;
    if manifest.size() > 128 * 1024 {
        return Err(AppError::Validation(format!(
            "{ZIP_PROJECT_MANIFEST_FILE} must be at most 131072 bytes"
        )));
    }

    let mut raw = String::new();
    manifest.read_to_string(&mut raw)?;
    parse_zip_project_manifest(&raw)
}

/// Return whether `value` can be safely joined under a project root.
pub fn is_safe_relative_path(value: &str) -> bool {
    if value.starts_with('/') {
        return false;
    }
    value.split(['/', '\\']).all(|s| !s.is_empty() && s != "..")
}

impl ZipProjectManifest {
    /// Validate protocol versioning, metadata, script paths, and dependency references.
    pub fn validate(&self) -> Result<()> {
        if self.protocol != ZIP_PROJECT_PROTOCOL {
            return Err(AppError::Validation(format!(
                "protocol must be {ZIP_PROJECT_PROTOCOL}"
            )));
        }
        if self.protocol_version != ZIP_PROJECT_PROTOCOL_VERSION {
            return Err(AppError::Validation(format!(
                "protocol_version must be {ZIP_PROJECT_PROTOCOL_VERSION}"
            )));
        }

        self.runtime.validate()?;
        self.commands.validate()?;
        self.phases.validate(&self.commands)?;
        self.interface.validate()?;
        self.dependencies.validate()?;

        Ok(())
    }

    /// Resolve the ordered setup/build/run plan from commands and phase overrides.
    pub fn phase_execution_plan(&self) -> Vec<ZipProjectResolvedPhase> {
        let mut phases = Vec::new();
        if let Some(command) = &self.commands.setup {
            phases.push(ZipProjectResolvedPhase {
                name: ZipProjectPhaseName::Setup,
                command: command.clone(),
                limits: self
                    .phases
                    .setup
                    .as_ref()
                    .unwrap_or(&ZipProjectPhaseConfig::default())
                    .resolve(ZipProjectPhaseName::Setup),
            });
        }
        if let Some(command) = &self.commands.build {
            phases.push(ZipProjectResolvedPhase {
                name: ZipProjectPhaseName::Build,
                command: command.clone(),
                limits: self
                    .phases
                    .build
                    .as_ref()
                    .unwrap_or(&ZipProjectPhaseConfig::default())
                    .resolve(ZipProjectPhaseName::Build),
            });
        }
        phases.push(ZipProjectResolvedPhase {
            name: ZipProjectPhaseName::Run,
            command: self.commands.run.clone(),
            limits: self.phases.run.resolve(ZipProjectPhaseName::Run),
        });

        phases
    }
}

impl ZipProjectRuntime {
    fn validate(&self) -> Result<()> {
        require_non_empty(&self.language, "runtime.language")?;
        if let Some(language_version) = &self.language_version {
            require_non_empty(language_version, "runtime.language_version")?;
        }
        if let Some(runtime_profile) = &self.runtime_profile {
            require_non_empty(runtime_profile, "runtime.runtime_profile")?;
        }

        Ok(())
    }
}

impl ZipProjectCommands {
    fn validate(&self) -> Result<()> {
        if let Some(setup) = &self.setup {
            require_safe_relative_path(setup, "commands.setup")?;
        }
        if let Some(build) = &self.build {
            require_safe_relative_path(build, "commands.build")?;
        }
        require_safe_relative_path(&self.run, "commands.run")?;

        Ok(())
    }
}

impl ZipProjectPhases {
    fn validate(&self, commands: &ZipProjectCommands) -> Result<()> {
        if self.setup.is_some() && commands.setup.is_none() {
            return Err(AppError::Validation(
                "phases.setup requires commands.setup".to_string(),
            ));
        }
        if self.build.is_some() && commands.build.is_none() {
            return Err(AppError::Validation(
                "phases.build requires commands.build".to_string(),
            ));
        }

        if let Some(setup) = &self.setup {
            setup.validate("phases.setup")?;
        }
        if let Some(build) = &self.build {
            build.validate("phases.build")?;
        }
        self.run.validate("phases.run")?;

        Ok(())
    }
}

impl ZipProjectPhaseConfig {
    fn validate(&self, field: &str) -> Result<()> {
        validate_positive_u64(self.timeout_sec, &format!("{field}.timeout_sec"))?;
        validate_positive_u64(self.memory_limit_mb, &format!("{field}.memory_limit_mb"))?;
        validate_positive_u32(self.cpu_limit_millis, &format!("{field}.cpu_limit_millis"))?;
        validate_positive_u64(self.disk_limit_mb, &format!("{field}.disk_limit_mb"))?;
        validate_positive_u64(self.log_limit_bytes, &format!("{field}.log_limit_bytes"))?;

        Ok(())
    }

    fn resolve(&self, phase: ZipProjectPhaseName) -> ZipProjectPhaseLimits {
        let default_timeout = match phase {
            ZipProjectPhaseName::Setup => DEFAULT_SETUP_TIMEOUT_SEC,
            ZipProjectPhaseName::Build => DEFAULT_BUILD_TIMEOUT_SEC,
            ZipProjectPhaseName::Run => DEFAULT_RUN_TIMEOUT_SEC,
        };

        ZipProjectPhaseLimits {
            timeout_sec: self.timeout_sec.unwrap_or(default_timeout),
            memory_limit_mb: self
                .memory_limit_mb
                .unwrap_or(DEFAULT_PHASE_MEMORY_LIMIT_MB),
            cpu_limit_millis: self
                .cpu_limit_millis
                .unwrap_or(DEFAULT_PHASE_CPU_LIMIT_MILLIS),
            disk_limit_mb: self.disk_limit_mb.unwrap_or(DEFAULT_PHASE_DISK_LIMIT_MB),
            network_access: self
                .network_access
                .unwrap_or(ZipProjectNetworkAccess::Disabled),
            log_limit_bytes: self
                .log_limit_bytes
                .unwrap_or(DEFAULT_PHASE_LOG_LIMIT_BYTES),
        }
    }
}

impl ZipProjectPhaseFailureReport {
    /// Validate a future worker failure report before persistence or API output.
    pub fn validate(&self) -> Result<()> {
        require_non_empty(&self.message, "phase_failure.message")?;
        if let Some(log_path) = &self.log_path {
            require_safe_relative_path(log_path, "phase_failure.log_path")?;
        }

        Ok(())
    }
}

impl ZipProjectInterface {
    fn validate(&self) -> Result<()> {
        if let Some(input_contract) = &self.input_contract {
            require_non_empty(input_contract, "interface.input_contract")?;
        }
        if let Some(output_contract) = &self.output_contract {
            require_non_empty(output_contract, "interface.output_contract")?;
        }

        Ok(())
    }
}

impl ZipProjectDependencies {
    fn validate(&self) -> Result<()> {
        validate_unique_paths(&self.lockfiles, "dependencies.lockfiles")?;
        validate_unique_paths(&self.vendor_dirs, "dependencies.vendor_dirs")?;
        if let Some(notes) = &self.notes {
            require_non_empty(notes, "dependencies.notes")?;
        }

        Ok(())
    }
}

fn validate_unique_paths(values: &[String], field: &str) -> Result<()> {
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        require_safe_relative_path(value, field)?;
        if !seen.insert(value.as_str()) {
            return Err(AppError::Validation(format!(
                "{field} contains duplicate path `{value}`"
            )));
        }
    }

    Ok(())
}

fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }

    Ok(())
}

fn validate_positive_u64(value: Option<u64>, field: &str) -> Result<()> {
    if value == Some(0) {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

fn validate_positive_u32(value: Option<u32>, field: &str) -> Result<()> {
    if value == Some(0) {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

fn require_safe_relative_path(value: &str, field: &str) -> Result<()> {
    require_non_empty(value, field)?;
    if !is_safe_relative_path(value) {
        return Err(AppError::Validation(format!(
            "{field} must be a safe relative path"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ZipProjectDependencyPolicy, ZipProjectInterfaceKind, ZipProjectNetworkAccess,
        ZipProjectPhaseFailureReason, ZipProjectPhaseFailureReport, ZipProjectPhaseName,
        parse_zip_project_manifest,
    };

    fn valid_manifest() -> serde_json::Value {
        json!({
            "protocol": "zip_project",
            "protocol_version": 1,
            "runtime": {
                "language": "python",
                "language_version": "3.12",
                "runtime_profile": "python-cpu"
            },
            "commands": {
                "setup": "scripts/setup.sh",
                "build": "scripts/build.sh",
                "run": "run.sh"
            },
            "phases": {
                "setup": {
                    "timeout_sec": 120,
                    "memory_limit_mb": 1024,
                    "cpu_limit_millis": 1500,
                    "disk_limit_mb": 2048,
                    "network_access": "enabled",
                    "log_limit_bytes": 2097152
                },
                "build": {
                    "timeout_sec": 300,
                    "network_access": "disabled"
                },
                "run": {
                    "timeout_sec": 45,
                    "network_access": "loopback"
                }
            },
            "interface": {
                "kind": "challenge_defined",
                "input_contract": "Challenge-defined JSON input.",
                "output_contract": "Challenge-defined stdout output."
            },
            "dependencies": {
                "policy": "lockfile",
                "lockfiles": ["requirements.lock"]
            }
        })
    }

    #[test]
    fn accepts_valid_zip_project_manifest() {
        let raw = serde_json::to_string(&valid_manifest()).expect("serialize manifest");
        let manifest = parse_zip_project_manifest(&raw).expect("manifest should parse");

        assert_eq!(manifest.protocol, "zip_project");
        assert_eq!(manifest.protocol_version, 1);
        assert_eq!(manifest.runtime.language, "python");
        assert_eq!(manifest.commands.run, "run.sh");
        assert_eq!(
            manifest.interface.kind,
            ZipProjectInterfaceKind::ChallengeDefined
        );
        assert_eq!(
            manifest.dependencies.policy,
            ZipProjectDependencyPolicy::Lockfile
        );

        let phases = manifest.phase_execution_plan();
        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0].name, ZipProjectPhaseName::Setup);
        assert_eq!(phases[0].command, "scripts/setup.sh");
        assert_eq!(phases[0].limits.timeout_sec, 120);
        assert_eq!(phases[0].limits.memory_limit_mb, 1024);
        assert_eq!(phases[0].limits.cpu_limit_millis, 1500);
        assert_eq!(phases[0].limits.disk_limit_mb, 2048);
        assert_eq!(
            phases[0].limits.network_access,
            ZipProjectNetworkAccess::Enabled
        );
        assert_eq!(phases[0].limits.log_limit_bytes, 2_097_152);
        assert_eq!(phases[1].name, ZipProjectPhaseName::Build);
        assert_eq!(phases[1].limits.timeout_sec, 300);
        assert_eq!(
            phases[1].limits.network_access,
            ZipProjectNetworkAccess::Disabled
        );
        assert_eq!(phases[2].name, ZipProjectPhaseName::Run);
        assert_eq!(phases[2].limits.timeout_sec, 45);
        assert_eq!(
            phases[2].limits.network_access,
            ZipProjectNetworkAccess::Loopback
        );
    }

    #[test]
    fn resolves_default_phase_limits_when_overrides_are_absent() {
        let mut value = valid_manifest();
        value
            .as_object_mut()
            .expect("manifest object")
            .remove("phases");

        let manifest =
            parse_zip_project_manifest(&value.to_string()).expect("defaults should parse");
        let phases = manifest.phase_execution_plan();

        assert_eq!(phases[0].name, ZipProjectPhaseName::Setup);
        assert_eq!(
            phases[0].limits.timeout_sec,
            super::DEFAULT_SETUP_TIMEOUT_SEC
        );
        assert_eq!(phases[1].name, ZipProjectPhaseName::Build);
        assert_eq!(
            phases[1].limits.timeout_sec,
            super::DEFAULT_BUILD_TIMEOUT_SEC
        );
        assert_eq!(phases[2].name, ZipProjectPhaseName::Run);
        assert_eq!(phases[2].limits.timeout_sec, super::DEFAULT_RUN_TIMEOUT_SEC);
        assert!(phases.iter().all(|phase| {
            phase.limits.memory_limit_mb == super::DEFAULT_PHASE_MEMORY_LIMIT_MB
                && phase.limits.cpu_limit_millis == super::DEFAULT_PHASE_CPU_LIMIT_MILLIS
                && phase.limits.disk_limit_mb == super::DEFAULT_PHASE_DISK_LIMIT_MB
                && phase.limits.network_access == ZipProjectNetworkAccess::Disabled
                && phase.limits.log_limit_bytes == super::DEFAULT_PHASE_LOG_LIMIT_BYTES
        }));
    }

    #[test]
    fn rejects_phase_config_without_matching_optional_command() {
        let mut value = valid_manifest();
        value["commands"]
            .as_object_mut()
            .expect("commands object")
            .remove("setup");

        let error = parse_zip_project_manifest(&value.to_string())
            .expect_err("setup phase without setup command should fail");
        assert!(
            error
                .to_string()
                .contains("phases.setup requires commands.setup")
        );
    }

    #[test]
    fn rejects_zero_phase_limit_overrides() {
        let mut value = valid_manifest();
        value["phases"]["run"]["timeout_sec"] = json!(0);

        let error =
            parse_zip_project_manifest(&value.to_string()).expect_err("zero timeout should fail");
        assert!(
            error
                .to_string()
                .contains("phases.run.timeout_sec must be greater than 0")
        );
    }

    #[test]
    fn rejects_missing_required_run_script() {
        let mut value = valid_manifest();
        value["commands"]
            .as_object_mut()
            .expect("commands object")
            .remove("run");

        let error = parse_zip_project_manifest(&value.to_string()).expect_err("run is required");
        assert!(error.to_string().contains("missing field `run`"));
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        let mut value = valid_manifest();
        value["protocol_version"] = json!(2);

        let error =
            parse_zip_project_manifest(&value.to_string()).expect_err("version should fail");
        assert!(error.to_string().contains("protocol_version must be 1"));
    }

    #[test]
    fn rejects_unsafe_script_paths() {
        let mut value = valid_manifest();
        value["commands"]["run"] = json!("../run.sh");

        let error =
            parse_zip_project_manifest(&value.to_string()).expect_err("unsafe run path fails");
        assert!(
            error
                .to_string()
                .contains("commands.run must be a safe relative path")
        );
    }

    #[test]
    fn rejects_invalid_dependency_paths() {
        let mut value = valid_manifest();
        value["dependencies"]["lockfiles"] = json!(["requirements.lock", "/tmp/lock"]);

        let error = parse_zip_project_manifest(&value.to_string())
            .expect_err("absolute dependency path fails");
        assert!(
            error
                .to_string()
                .contains("dependencies.lockfiles must be a safe relative path")
        );
    }

    #[test]
    fn rejects_duplicate_dependency_paths() {
        let mut value = valid_manifest();
        value["dependencies"]["vendor_dirs"] = json!(["vendor", "vendor"]);

        let error = parse_zip_project_manifest(&value.to_string())
            .expect_err("duplicate dependency path fails");
        assert!(
            error
                .to_string()
                .contains("dependencies.vendor_dirs contains duplicate path")
        );
    }

    #[test]
    fn rejects_unknown_manifest_fields() {
        let mut value = valid_manifest();
        value["unexpected"] = json!(true);

        let error =
            parse_zip_project_manifest(&value.to_string()).expect_err("unknown fields should fail");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn validates_phase_failure_report_payloads() {
        let report = ZipProjectPhaseFailureReport {
            phase: ZipProjectPhaseName::Build,
            reason: ZipProjectPhaseFailureReason::NonZeroExit,
            message: "build script exited with status 1".to_string(),
            exit_code: Some(1),
            log_path: Some("logs/build.stderr.txt".to_string()),
        };
        report.validate().expect("failure report should validate");

        let invalid = ZipProjectPhaseFailureReport {
            log_path: Some("../outside.log".to_string()),
            ..report
        };
        let error = invalid.validate().expect_err("unsafe log path should fail");
        assert!(
            error
                .to_string()
                .contains("phase_failure.log_path must be a safe relative path")
        );
    }
}
