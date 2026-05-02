//! `zip_project` solution submission protocol schema.
//!
//! The parser in this module defines the manifest contract only. Worker
//! execution of setup, build, and run phases is handled by later milestones.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub const ZIP_PROJECT_MANIFEST_FILE: &str = "agentics.solution.json";
pub const ZIP_PROJECT_PROTOCOL: &str = "zip_project";
pub const ZIP_PROJECT_PROTOCOL_VERSION: u16 = 1;

/// Parsed `agentics.solution.json` manifest for a ZIP project solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectManifest {
    pub protocol: String,
    pub protocol_version: u16,
    pub runtime: ZipProjectRuntime,
    pub commands: ZipProjectCommands,
    pub interface: ZipProjectInterface,
    pub dependencies: ZipProjectDependencies,
}

/// Language and runtime metadata declared by the submitting solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectRuntime {
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_profile: Option<String>,
}

/// Script paths used by the future setup/build/run phase executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectCommands {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    pub run: String,
}

/// How the challenge harness should invoke and communicate with the solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZipProjectInterface {
    pub kind: ZipProjectInterfaceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_contract: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contract: Option<String>,
}

/// Invocation styles that a challenge harness may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectInterfaceKind {
    ChallengeDefined,
    Argv,
    Stdio,
    FileSystem,
    Http,
}

/// Declared dependency strategy for the submitted ZIP project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        self.interface.validate()?;
        self.dependencies.validate()?;

        Ok(())
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

fn require_safe_relative_path(value: &str, field: &str) -> Result<()> {
    require_non_empty(value, field)?;
    if value.starts_with('/') {
        return Err(AppError::Validation(format!(
            "{field} must be a safe relative path"
        )));
    }
    if !value.split(['/', '\\']).all(|s| !s.is_empty() && s != "..") {
        return Err(AppError::Validation(format!(
            "{field} must be a safe relative path"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ZipProjectDependencyPolicy, ZipProjectInterfaceKind, parse_zip_project_manifest};

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
}
