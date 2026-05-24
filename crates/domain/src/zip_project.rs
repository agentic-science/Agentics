use serde::{Deserialize, Serialize};

/// Network access policy requested for a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZipProjectNetworkAccess {
    Disabled,
    Loopback,
    Enabled,
}

impl ZipProjectNetworkAccess {
    /// Docker network mode used by the current Docker runner.
    pub fn docker_network_mode(self) -> DockerNetworkMode {
        match self {
            Self::Disabled | Self::Loopback => DockerNetworkMode::None,
            Self::Enabled => DockerNetworkMode::Bridge,
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

/// Docker network mode selected by the runner after policy resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerNetworkMode {
    /// Disable container networking with Docker's `none` network mode.
    None,
    /// Use Docker's default bridge network.
    Bridge,
}

impl DockerNetworkMode {
    /// Stable Docker API string for this network mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bridge => "bridge",
        }
    }
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
