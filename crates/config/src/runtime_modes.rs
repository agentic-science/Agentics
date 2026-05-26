//! Typed runtime mode and runner namespace configuration values.

use std::str::FromStr;

use agentics_domain::models::challenge::TargetAccelerator;
use serde::{Deserialize, Deserializer};

use crate::ENV_AGENTICS_RUNNER_NAMESPACE;

/// Runner strategy for Docker bind-mounted writable paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerWritableStorageMode {
    /// Keep writable paths under `AGENTICS_STORAGE_ROOT`.
    Unbounded,
    /// Lease root-prepared XFS project-quota slots for writable container paths.
    XfsProjectQuotaSlots,
}

/// Policy for unauthenticated agent-account registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRegistrationMode {
    /// Require a valid pioneer code for every new agent account.
    PioneerCode,
    /// Allow code-free registration for local testing and development only.
    Public,
}

/// Worker startup host-profile probe policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostProbeMode {
    /// Do not run hosted profile checks.
    Off,
    /// Run hosted profile checks and log failures without blocking startup.
    Warn,
    /// Run hosted profile checks and fail worker startup if they fail or are skipped.
    Require,
}

/// Worker runner safety profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerSecurityProfile {
    /// Local development and test profile. Host isolation checks are opt-in.
    Development,
    /// Production profile. Runner storage, Docker layers, and host probes fail closed.
    Production,
}

/// Logical owner namespace for Docker runner containers on a shared daemon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunnerNamespace(String);

/// Worker accelerator capability advertised to the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerAccelerators {
    /// Worker accepts only jobs that require no accelerator.
    None,
    /// Worker accepts no-accelerator jobs and GPU jobs.
    Gpu,
}

impl HostProbeMode {
    /// Stable environment string for this policy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Require => "require",
        }
    }
}

impl FromStr for HostProbeMode {
    type Err = anyhow::Error;

    /// Parse the configured host-probe mode.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "off" => Ok(Self::Off),
            "warn" => Ok(Self::Warn),
            "require" => Ok(Self::Require),
            other => anyhow::bail!(
                "AGENTICS_HOST_PROBE_MODE must be `off`, `warn`, or `require`, got `{other}`"
            ),
        }
    }
}

impl RunnerSecurityProfile {
    /// Stable environment string for this policy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }
}

impl FromStr for RunnerSecurityProfile {
    type Err = anyhow::Error;

    /// Parse the configured runner security profile.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "development" => Ok(Self::Development),
            "production" => Ok(Self::Production),
            other => anyhow::bail!(
                "AGENTICS_RUNNER_SECURITY_PROFILE must be `development` or `production`, got `{other}`"
            ),
        }
    }
}

impl WorkerAccelerators {
    /// Stable environment string for this capability set.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gpu => "gpu",
        }
    }

    /// Return whether this worker can claim a job requiring the given accelerator.
    pub fn supports(self, accelerator: TargetAccelerator) -> bool {
        match (self, accelerator) {
            (_, TargetAccelerator::None) | (Self::Gpu, TargetAccelerator::Gpu) => true,
            (Self::None, TargetAccelerator::Gpu) => false,
        }
    }

    /// Return heartbeat-friendly accelerator capability labels.
    pub fn heartbeat_values(self) -> Vec<String> {
        match self {
            Self::None => vec!["none".to_string()],
            Self::Gpu => vec!["none".to_string(), "gpu".to_string()],
        }
    }
}

impl FromStr for WorkerAccelerators {
    type Err = anyhow::Error;

    /// Parse the configured worker accelerator capability.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "none" => Ok(Self::None),
            "gpu" => Ok(Self::Gpu),
            other => {
                anyhow::bail!("AGENTICS_WORKER_ACCELERATORS must be `none` or `gpu`, got `{other}`")
            }
        }
    }
}

impl AgentRegistrationMode {
    /// Stable environment string for this registration policy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PioneerCode => "pioneer_code",
            Self::Public => "public",
        }
    }
}

impl RunnerNamespace {
    /// Parse and validate one Docker-runner namespace.
    pub fn try_new(value: impl Into<String>) -> anyhow::Result<Self> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("{ENV_AGENTICS_RUNNER_NAMESPACE} must not be empty");
        }
        if trimmed.len() > 63 {
            anyhow::bail!("{ENV_AGENTICS_RUNNER_NAMESPACE} must be at most 63 bytes");
        }
        if !trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            anyhow::bail!(
                "{ENV_AGENTICS_RUNNER_NAMESPACE} may contain only ASCII letters, digits, '.', '_', and '-'"
            );
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Return the canonical namespace label value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RunnerNamespace {
    type Err = anyhow::Error;

    /// Parse a runner namespace from an environment/config boundary value.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        Self::try_new(value)
    }
}

impl<'de> Deserialize<'de> for RunnerNamespace {
    /// Deserialize one runner namespace through the canonical parser.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

impl FromStr for AgentRegistrationMode {
    type Err = anyhow::Error;

    /// Parse the configured agent-registration mode.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "pioneer_code" => Ok(Self::PioneerCode),
            "public" => Ok(Self::Public),
            other => anyhow::bail!(
                "AGENTICS_AGENT_REGISTRATION_MODE must be `pioneer_code` or `public`, got `{other}`"
            ),
        }
    }
}

impl<'de> Deserialize<'de> for AgentRegistrationMode {
    /// Deserialize one agent-registration mode through the canonical parser.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}

impl RunnerWritableStorageMode {
    /// Stable environment string for this runner writable-storage strategy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unbounded => "unbounded",
            Self::XfsProjectQuotaSlots => "xfs-project-quota-slots",
        }
    }
}

impl FromStr for RunnerWritableStorageMode {
    type Err = anyhow::Error;

    /// Handles from str for this module.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "unbounded" => Ok(Self::Unbounded),
            "xfs-project-quota-slots" => Ok(Self::XfsProjectQuotaSlots),
            other => anyhow::bail!(
                "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE must be `unbounded` or `xfs-project-quota-slots`, got `{other}`"
            ),
        }
    }
}

impl<'de> Deserialize<'de> for RunnerWritableStorageMode {
    /// Deserialize one runner writable-storage mode through the canonical parser.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(serde::de::Error::custom)
    }
}
