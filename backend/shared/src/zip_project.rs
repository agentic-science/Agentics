//! `zip_project` solution submission protocol schema.
//!
//! This module defines manifest parsing plus the setup/build/run phase model
//! that the worker will consume in later execution milestones.

use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::paths::{LogRelativePath, ScriptPath};
use crate::validation::archive::{ArchiveEnvelopePolicy, inspect_zip_bytes};
use crate::validation::text;

pub const ZIP_PROJECT_MANIFEST_FILE: &str = "agentics.solution.json";
pub const ZIP_PROJECT_PROTOCOL: &str = "zip_project";
pub const ZIP_PROJECT_PROTOCOL_VERSION: u16 = 1;
pub const MAX_ZIP_PROJECT_NOTE_BYTES: usize = 1024;
pub const MAX_ZIP_PROJECT_ARTIFACT_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_ZIP_PROJECT_FILE_COUNT: usize = 256;
pub const MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024;

/// Validate the ZIP archive envelope before durable storage or extraction.
pub fn validate_zip_project_archive_envelope(bytes: &[u8]) -> Result<()> {
    inspect_zip_bytes(bytes, &zip_project_archive_policy())?;
    Ok(())
}

/// Shared archive envelope policy for `zip_project` solution ZIPs.
pub fn zip_project_archive_policy() -> ArchiveEnvelopePolicy {
    ArchiveEnvelopePolicy::new(
        "solution archive",
        MAX_ZIP_PROJECT_ARTIFACT_BYTES,
        MAX_ZIP_PROJECT_FILE_COUNT,
        MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    )
}

/// Parsed `agentics.solution.json` manifest for a ZIP project solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectManifest {
    pub protocol: String,
    pub protocol_version: u16,
    #[serde(default)]
    pub note: String,
    pub commands: ZipProjectCommands,
}

/// Script paths used by the future setup/build/run phase executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectCommands {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<ScriptPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<ScriptPath>,
    pub run: ScriptPath,
}

/// Concrete limits for one execution phase after challenge-owned policy is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ZipProjectPhaseLimits {
    pub timeout_sec: u64,
    pub memory_limit_mb: u64,
    pub cpu_limit_millis: u32,
    pub disk_limit_mb: u64,
    pub network_access: ZipProjectNetworkAccess,
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

    /// Handles rank for this module.
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
    pub command: ScriptPath,
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
    pub log_path: Option<LogRelativePath>,
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

impl ZipProjectManifest {
    /// Parse and validate a manifest JSON payload.
    pub fn parse_json(raw: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_str(raw).map_err(|e| {
            AppError::Validation(format!("invalid {ZIP_PROJECT_MANIFEST_FILE}: {e}"))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Parse and validate `agentics.solution.json` directly from a ZIP artifact.
    pub fn from_zip_bytes(bytes: &[u8]) -> Result<Self> {
        validate_zip_project_archive_envelope(bytes)?;
        let reader = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader)?;
        let mut manifest = archive.by_name(ZIP_PROJECT_MANIFEST_FILE).map_err(|_| {
            AppError::Validation(format!("{ZIP_PROJECT_MANIFEST_FILE} is required"))
        })?;
        if manifest.size() > 128 * 1024 {
            return Err(AppError::Validation(format!(
                "{ZIP_PROJECT_MANIFEST_FILE} must be at most 131072 bytes"
            )));
        }

        let mut raw = String::new();
        manifest.read_to_string(&mut raw)?;
        Self::parse_json(&raw)
    }

    /// Validate protocol versioning, submitter metadata, and script paths.
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

        validate_solution_note(&self.note)?;
        self.commands.validate()?;

        Ok(())
    }

    /// Resolve the ordered setup/build/run plan from commands and phase overrides.
    pub fn phase_execution_plan(&self) -> Vec<ZipProjectResolvedPhase> {
        let mut phases = Vec::new();
        if let Some(command) = &self.commands.setup {
            phases.push(ZipProjectResolvedPhase {
                name: ZipProjectPhaseName::Setup,
                command: command.clone(),
            });
        }
        if let Some(command) = &self.commands.build {
            phases.push(ZipProjectResolvedPhase {
                name: ZipProjectPhaseName::Build,
                command: command.clone(),
            });
        }
        phases.push(ZipProjectResolvedPhase {
            name: ZipProjectPhaseName::Run,
            command: self.commands.run.clone(),
        });

        phases
    }
}

impl ZipProjectCommands {
    /// Handles validate for this module.
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl ZipProjectPhaseFailureReport {
    /// Validate a future worker failure report before persistence or API output.
    pub fn validate(&self) -> Result<()> {
        require_non_empty(&self.message, "phase_failure.message")?;

        Ok(())
    }
}

/// Requires non empty and reports a domain error otherwise.
fn require_non_empty(value: &str, field: &str) -> Result<()> {
    text::require_non_empty(value, field)
}

/// Validate submitter-visible note text from `agentics.solution.json`.
pub fn validate_solution_note(note: &str) -> Result<()> {
    text::validate_solution_note(note, MAX_ZIP_PROJECT_NOTE_BYTES)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use serde_json::json;

    use crate::models::paths::LogRelativePath;

    use super::{
        MAX_ZIP_PROJECT_NOTE_BYTES, ZipProjectManifest, ZipProjectPhaseFailureReason,
        ZipProjectPhaseFailureReport, ZipProjectPhaseName, validate_zip_project_archive_envelope,
    };

    /// Builds a test ZIP archive with the supplied stored entries.
    fn zip_with_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (path, content) in entries {
                archive
                    .start_file(path, options)
                    .expect("test ZIP entry should start");
                archive
                    .write_all(content)
                    .expect("test ZIP entry content should write");
            }
            archive.finish().expect("test ZIP should finish");
        }
        cursor.into_inner()
    }

    /// Handles valid manifest for this module.
    fn valid_manifest() -> serde_json::Value {
        json!({
            "protocol": "zip_project",
            "protocol_version": 1,
            "note": "public note\nwith whitespace",
            "commands": {
                "setup": "scripts/setup.sh",
                "build": "scripts/build.sh",
                "run": "run.sh"
            }
        })
    }

    /// Verifies that accepts valid zip project manifest.
    #[test]
    fn accepts_valid_zip_project_manifest() {
        let raw = serde_json::to_string(&valid_manifest()).expect("serialize manifest");
        let manifest = ZipProjectManifest::parse_json(&raw).expect("manifest should parse");

        assert_eq!(manifest.protocol, "zip_project");
        assert_eq!(manifest.protocol_version, 1);
        assert_eq!(manifest.note, "public note\nwith whitespace");
        assert_eq!(manifest.commands.run.as_str(), "run.sh");

        let phases = manifest.phase_execution_plan();
        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0].name, ZipProjectPhaseName::Setup);
        assert_eq!(phases[0].command.as_str(), "scripts/setup.sh");
        assert_eq!(phases[1].name, ZipProjectPhaseName::Build);
        assert_eq!(phases[1].command.as_str(), "scripts/build.sh");
        assert_eq!(phases[2].name, ZipProjectPhaseName::Run);
        assert_eq!(phases[2].command.as_str(), "run.sh");
    }

    /// Verifies that note defaults to empty when omitted.
    #[test]
    fn note_defaults_to_empty_when_omitted() {
        let mut value = valid_manifest();
        value
            .as_object_mut()
            .expect("manifest object")
            .remove("note");

        let manifest =
            ZipProjectManifest::parse_json(&value.to_string()).expect("manifest should parse");

        assert_eq!(manifest.note, "");
    }

    /// Verifies that minimal manifest only requires a run command.
    #[test]
    fn accepts_minimal_manifest() {
        let manifest = ZipProjectManifest::parse_json(
            &json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        )
        .expect("minimal manifest should parse");

        let phases = manifest.phase_execution_plan();
        assert_eq!(manifest.note, "");
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].name, ZipProjectPhaseName::Run);
        assert_eq!(phases[0].command.as_str(), "run.sh");
    }

    /// Verifies that setup and build commands are optional.
    #[test]
    fn accepts_optional_setup_and_build_commands() {
        let manifest =
            ZipProjectManifest::parse_json(&valid_manifest().to_string()).expect("manifest");

        let phases = manifest.phase_execution_plan();
        assert_eq!(
            phases.iter().map(|phase| phase.name).collect::<Vec<_>>(),
            vec![
                ZipProjectPhaseName::Setup,
                ZipProjectPhaseName::Build,
                ZipProjectPhaseName::Run,
            ]
        );
    }

    /// Verifies that old participant-controlled fields are rejected.
    #[test]
    fn rejects_old_submitter_controlled_manifest_fields() {
        let mut value = valid_manifest();
        value["runtime"] = json!({ "language": "python" });

        let error = ZipProjectManifest::parse_json(&value.to_string())
            .expect_err("old runtime field should fail");
        assert!(error.to_string().contains("unknown field `runtime`"));

        for field in ["phases", "interface", "dependencies"] {
            let mut value = valid_manifest();
            value[field] = json!({});
            let error = ZipProjectManifest::parse_json(&value.to_string())
                .expect_err("old manifest field should fail");
            assert!(
                error
                    .to_string()
                    .contains(&format!("unknown field `{field}`")),
                "unexpected error for {field}: {error}"
            );
        }
    }

    /// Verifies that note rejects too many decoded UTF-8 bytes.
    #[test]
    fn rejects_over_limit_note() {
        let mut value = valid_manifest();
        value["note"] = json!("a".repeat(MAX_ZIP_PROJECT_NOTE_BYTES + 1));

        let error = ZipProjectManifest::parse_json(&value.to_string())
            .expect_err("over-limit note should fail");
        assert!(
            error
                .to_string()
                .contains("note must be at most 1024 UTF-8 bytes")
        );
    }

    /// Verifies that note allows normal whitespace and rejects non-text controls.
    #[test]
    fn validates_note_control_characters() {
        let mut value = valid_manifest();
        value["note"] = json!("line one\nline two\tok\r");
        ZipProjectManifest::parse_json(&value.to_string()).expect("normal whitespace should parse");

        value["note"] = json!("bad\u{0007}bell");
        let error = ZipProjectManifest::parse_json(&value.to_string())
            .expect_err("control character should fail");
        assert!(
            error
                .to_string()
                .contains("note must not contain non-text control characters")
        );
    }

    /// Verifies that rejects missing required run script.
    #[test]
    fn rejects_missing_required_run_script() {
        let mut value = valid_manifest();
        value["commands"]
            .as_object_mut()
            .expect("commands object")
            .remove("run");

        let error =
            ZipProjectManifest::parse_json(&value.to_string()).expect_err("run is required");
        assert!(error.to_string().contains("missing field `run`"));
    }

    /// Verifies that rejects unsupported protocol version.
    #[test]
    fn rejects_unsupported_protocol_version() {
        let mut value = valid_manifest();
        value["protocol_version"] = json!(2);

        let error =
            ZipProjectManifest::parse_json(&value.to_string()).expect_err("version should fail");
        assert!(error.to_string().contains("protocol_version must be 1"));
    }

    /// Verifies that rejects unsafe script paths.
    #[test]
    fn rejects_unsafe_script_paths() {
        let mut value = valid_manifest();
        value["commands"]["run"] = json!("../run.sh");

        let error =
            ZipProjectManifest::parse_json(&value.to_string()).expect_err("unsafe run path fails");
        assert!(error.to_string().contains("repo-relative paths"));
    }

    /// Verifies that rejects unknown manifest fields.
    #[test]
    fn rejects_unknown_manifest_fields() {
        let mut value = valid_manifest();
        value["unexpected"] = json!(true);

        let error = ZipProjectManifest::parse_json(&value.to_string())
            .expect_err("unknown fields should fail");
        assert!(error.to_string().contains("unknown field"));
    }

    /// Verifies that validates phase failure report payloads.
    #[test]
    fn validates_phase_failure_report_payloads() {
        let report = ZipProjectPhaseFailureReport {
            phase: ZipProjectPhaseName::Build,
            reason: ZipProjectPhaseFailureReason::NonZeroExit,
            message: "build script exited with status 1".to_string(),
            exit_code: Some(1),
            log_path: Some(
                LogRelativePath::try_new("logs/build.stderr.txt").expect("test log path is valid"),
            ),
        };
        report.validate().expect("failure report should validate");

        let invalid = json!({
            "phase": "build",
            "reason": "non_zero_exit",
            "message": "build script exited with status 1",
            "exit_code": 1,
            "log_path": "../outside.log"
        });
        let error = serde_json::from_value::<ZipProjectPhaseFailureReport>(invalid)
            .expect_err("unsafe log path should fail during deserialization");
        assert!(error.to_string().contains("repo-relative paths"));
    }

    /// Verifies archive envelope validation rejects traversal entries.
    #[test]
    fn archive_envelope_rejects_unsafe_entry_paths() {
        let bytes = zip_with_entries(&[("../escape.txt", b"escape")]);
        let error =
            validate_zip_project_archive_envelope(&bytes).expect_err("unsafe entry should fail");

        assert!(error.to_string().contains("unsafe path"));
    }

    /// Verifies archive envelope validation rejects duplicate normalized paths.
    #[test]
    fn archive_envelope_rejects_duplicate_entries() {
        let bytes = zip_with_entries(&[("dir/run.sh", b"one"), ("dir\\run.sh", b"two")]);
        let error =
            validate_zip_project_archive_envelope(&bytes).expect_err("duplicate entry should fail");

        assert!(error.to_string().contains("duplicate path"));
    }
}
