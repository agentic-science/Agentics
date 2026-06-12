use std::borrow::Cow;
use std::fmt;

use schemars::{Schema, SchemaGenerator, json_schema};
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::images::ChallengeImageReference;
use super::super::names::{ResourceProfileName, TargetName};
use super::serde_helpers::{required_nullable, required_nullable_schema};
use crate::zip_project::ZipProjectNetworkAccess;

/// Supported Docker platforms for targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum DockerPlatform {
    #[serde(rename = "linux/arm64")]
    LinuxArm64,
    #[serde(rename = "linux/amd64")]
    LinuxAmd64,
}

impl DockerPlatform {
    /// Canonical Docker platform string used in Docker API requests.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinuxArm64 => "linux/arm64",
            Self::LinuxAmd64 => "linux/amd64",
        }
    }
}

/// Accelerator selection used by a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetAccelerator {
    None,
    Gpu,
}

impl TargetAccelerator {
    /// Stable string form used in user-facing summaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gpu => "gpu",
        }
    }

    /// Parse a stable database string for required worker accelerator scheduling.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "gpu" => Some(Self::Gpu),
            _ => None,
        }
    }
}

impl Serialize for TargetAccelerator {
    /// Serialize no accelerator as explicit JSON null and GPU as the only accelerator string.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::None => serializer.serialize_none(),
            Self::Gpu => serializer.serialize_str("gpu"),
        }
    }
}

impl<'de> Deserialize<'de> for TargetAccelerator {
    /// Deserialize required nullable accelerator policy from challenge configs.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TargetAcceleratorVisitor;

        impl<'de> Visitor<'de> for TargetAcceleratorVisitor {
            type Value = TargetAccelerator;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("null for no accelerator or \"gpu\" for GPU acceleration")
            }

            fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(TargetAccelerator::None)
            }

            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(TargetAccelerator::None)
            }

            fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_any(self)
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: DeError,
            {
                match value {
                    "gpu" => Ok(TargetAccelerator::Gpu),
                    "cpu" => Err(E::custom(
                        "accelerator must be explicit null when no accelerator is required, not \"cpu\"",
                    )),
                    other => Err(E::unknown_variant(other, &["gpu"])),
                }
            }
        }

        deserializer.deserialize_any(TargetAcceleratorVisitor)
    }
}

impl schemars::JsonSchema for TargetAccelerator {
    /// Target accelerator is an inline required nullable field in target specs.
    fn inline_schema() -> bool {
        true
    }

    /// Stable schema name for target accelerator.
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("TargetAccelerator")
    }

    /// JSON schema for `null | "gpu"`.
    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "x-agentics-preserve-null": true,
            "oneOf": [
                { "type": "null" },
                { "type": "string", "enum": ["gpu"] }
            ]
        })
    }
}

/// One execution and ranking target declared by a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeTargetSpec {
    pub name: TargetName,
    pub docker_platform: DockerPlatform,
    /// Required nullable field: JSON null means no accelerator, "gpu" means GPU acceleration.
    pub accelerator: TargetAccelerator,
    pub validation_enabled: bool,
    pub resource_profile: ResourceProfileSpec,
}

/// Resource envelope and Docker images declared by a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct ResourceProfileSpec {
    pub name: ResourceProfileName,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub resource_description: Option<String>,
    pub solution_image: ChallengeImageReference,
    pub evaluator_image: ChallengeImageReference,
    pub solution: SolutionStageProfiles,
    pub evaluator: EvaluatorStageProfiles,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(
        required,
        schema_with = "required_nullable_schema::<HardwareProfileSpec>"
    )]
    pub hardware_metadata: Option<HardwareProfileSpec>,
}

/// Resource limits for participant-owned solution stages.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SolutionStageProfiles {
    pub setup: StageResourceProfile,
    pub build: StageResourceProfile,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(
        required,
        schema_with = "required_nullable_schema::<StageResourceProfile>"
    )]
    pub run: Option<StageResourceProfile>,
}

/// Resource limits for trusted challenge-owned evaluator stages.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluatorStageProfiles {
    pub setup: StageResourceProfile,
    pub run: StageResourceProfile,
}

/// Resource envelope for one Docker-executed stage.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageResourceProfile {
    #[garde(range(min = 1))]
    pub timeout_sec: u64,
    #[garde(range(min = 1))]
    pub memory_limit_mb: u64,
    #[garde(range(min = 1))]
    pub cpu_limit_millis: u32,
    #[garde(range(min = 1))]
    pub disk_limit_mb: u64,
    #[garde(skip)]
    pub network_access: ZipProjectNetworkAccess,
}

/// Optional hardware metadata advertised with a resource profile.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct HardwareProfileSpec {
    #[garde(custom(crate::validation::trimmed_non_empty))]
    pub kind: String,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub gpu_model: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<u32>")]
    #[garde(range(min = 1))]
    pub gpu_count: Option<u32>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<u64>")]
    #[garde(range(min = 1))]
    pub gpu_memory_gb: Option<u64>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub cuda_variant: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub cuda_version: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub driver_minimum: Option<String>,
}
