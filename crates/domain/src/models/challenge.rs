//! Challenge bundle and challenge-facing DTOs.

use std::borrow::Cow;
use std::fmt;

use schemars::{Schema, SchemaGenerator, json_schema};
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ids::ChallengeId;
use super::images::ChallengeImageReference;
use super::localization::LocalizedText;
use super::names::{
    ChallengeKeyword, ChallengeName, MetricName, MoltbookSubmoltName, ResourceProfileName, RunName,
    TargetName,
};
use super::paths::{
    BundleRelativePath, ManagedBundlePath, ManagedStatementPath, RunInputPath, RunOutputPath,
};
use super::urls::{MoltbookPostUrl, MoltbookSubmoltUrl};
use crate::zip_project::ZipProjectNetworkAccess;

/// Persistent lifecycle state for a challenge shell or published benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeLifecycleStatus {
    Draft,
    Active,
    Archived,
}

impl ChallengeLifecycleStatus {
    /// Stable database string for a challenge lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    /// Parse a stable database string for a challenge lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

impl fmt::Display for ChallengeLifecycleStatus {
    /// Format the challenge status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Minimum public keywords that a challenge must declare.
pub const MIN_CHALLENGE_KEYWORDS: usize = 1;

/// Maximum public keywords that a challenge may declare.
pub const MAX_CHALLENGE_KEYWORDS: usize = 6;

/// Parsed `spec.json` contract for a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    /// Localized summary used in compact challenge catalog surfaces.
    pub summary: LocalizedText,
    /// Required public keywords used by catalog search and filtering.
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub solution: SolutionSpec,
    pub targets: Vec<ChallengeTargetSpec>,
    pub starts_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_submission_limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_submission_limit: Option<i64>,
    pub visibility: ChallengeVisibilitySpec,
    pub solution_publication: ChallengeSolutionPublicationPolicy,
    pub execution: ChallengeExecutionSpec,
    pub datasets: DatasetsSpec,
    /// Metric definitions and ranking metadata used to interpret evaluator output.
    #[serde(default)]
    #[schemars(required)]
    pub metric_schema: MetricSchemaSpec,
}

impl ChallengeBundleSpec {
    /// Look up one target declared by this challenge.
    pub fn target(&self, target: &TargetName) -> Option<&ChallengeTargetSpec> {
        self.targets
            .iter()
            .find(|candidate| &candidate.name == target)
    }

    /// Return the only target name when a challenge is unambiguous.
    pub fn sole_target(&self) -> Option<&TargetName> {
        match self.targets.as_slice() {
            [target] => Some(&target.name),
            _ => None,
        }
    }
}

/// Public projection of a challenge contract safe for unauthenticated clients.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    /// Localized summary used in compact challenge catalog surfaces.
    pub summary: LocalizedText,
    /// Required public keywords used by catalog search and filtering.
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub solution: SolutionSpec,
    pub targets: Vec<ChallengeTargetSpec>,
    pub starts_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_submission_limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_submission_limit: Option<i64>,
    pub visibility: ChallengeVisibilitySpec,
    pub solution_publication: ChallengeSolutionPublicationPolicy,
    pub execution: PublicChallengeExecutionSpec,
    pub datasets: PublicDatasetsSpec,
    /// Metric definitions and ranking metadata used to interpret evaluator output.
    #[serde(default)]
    #[schemars(required)]
    pub metric_schema: MetricSchemaSpec,
}

impl PublicChallengeBundleSpec {
    /// Look up one public target declared by this challenge.
    pub fn target(&self, target: &TargetName) -> Option<&ChallengeTargetSpec> {
        self.targets
            .iter()
            .find(|candidate| &candidate.name == target)
    }

    /// Return the only target name when a public challenge is unambiguous.
    pub fn sole_target(&self) -> Option<&TargetName> {
        match self.targets.as_slice() {
            [target] => Some(&target.name),
            _ => None,
        }
    }
}

impl From<ChallengeBundleSpec> for PublicChallengeBundleSpec {
    /// Remove private benchmark locator metadata from a full challenge contract.
    fn from(spec: ChallengeBundleSpec) -> Self {
        Self {
            schema_version: spec.schema_version,
            challenge_name: spec.challenge_name,
            challenge_title: spec.challenge_title,
            summary: spec.summary,
            keywords: spec.keywords,
            solution: spec.solution,
            targets: spec.targets,
            starts_at: spec.starts_at,
            closes_at: spec.closes_at,
            eligibility: spec.eligibility,
            validation_submission_limit: spec.validation_submission_limit,
            official_submission_limit: spec.official_submission_limit,
            visibility: spec.visibility,
            solution_publication: spec.solution_publication,
            execution: spec.execution.into(),
            datasets: PublicDatasetsSpec {
                public_dir: spec.datasets.public_dir,
                public_policy: spec.datasets.public_policy,
                private_benchmark_policy: spec.datasets.private_benchmark_policy,
                private_benchmark_enabled: spec.datasets.private_benchmark_enabled,
            },
            metric_schema: spec.metric_schema,
        }
    }
}

/// Eligibility policy for a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeEligibilitySpec {
    #[serde(rename = "type")]
    pub eligibility_type: ChallengeEligibilityType,
}

/// Stable eligibility policy names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeEligibilityType {
    Open,
    PrivateShortlist,
}

/// Visibility policy for challenge result surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeVisibilitySpec {
    pub leaderboard: ChallengeVisibility,
    pub score_distribution: ChallengeVisibility,
    pub result_detail: ChallengeResultDetailVisibility,
}

/// Visibility for public aggregate surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeVisibility {
    PublicLive,
    PublicAfterClose,
    Hidden,
}

/// Visibility for solution submission details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeResultDetailVisibility {
    SubmitterLivePublicLive,
    SubmitterLivePublicAfterClose,
    SubmitterOnly,
}

/// Policy controlling when solution artifacts may become public.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeSolutionPublicationPolicy {
    Private,
    Public,
    PublicAfterClose,
}

/// Local solution format constraints declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SolutionSpec {
    pub protocol: String,
    pub manifest_file: BundleRelativePath,
}

/// Evaluator entrypoint and output-file contract for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluatorSpec {
    pub command: Vec<String>,
    pub result_file: BundleRelativePath,
}

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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceProfileSpec {
    pub name: ResourceProfileName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_description: Option<String>,
    pub solution_image: ChallengeImageReference,
    pub evaluator_image: ChallengeImageReference,
    pub solution: SolutionStageProfiles,
    pub evaluator: EvaluatorStageProfiles,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_metadata: Option<HardwareProfileSpec>,
}

/// Resource limits for participant-owned solution stages.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SolutionStageProfiles {
    pub setup: StageResourceProfile,
    pub build: StageResourceProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageResourceProfile {
    pub timeout_sec: u64,
    pub memory_limit_mb: u64,
    pub cpu_limit_millis: u32,
    pub disk_limit_mb: u64,
    pub network_access: ZipProjectNetworkAccess,
}

/// Optional hardware metadata advertised with a resource profile.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HardwareProfileSpec {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_memory_gb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_minimum: Option<String>,
}

/// Supported challenge execution topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeExecutionMode {
    SeparatedEvaluator,
    PipedStdio,
    CoexecutedBenchmark,
}

/// Challenge-owned execution topology and run manifest locations for `zip_project`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ChallengeExecutionSpec {
    SeparatedEvaluator(SeparatedEvaluatorExecutionSpec),
    PipedStdio(PipedStdioExecutionSpec),
    CoexecutedBenchmark(CoexecutedBenchmarkExecutionSpec),
}

impl ChallengeExecutionSpec {
    /// Return the current execution topology mode.
    pub fn mode(&self) -> ChallengeExecutionMode {
        match self {
            Self::SeparatedEvaluator(_) => ChallengeExecutionMode::SeparatedEvaluator,
            Self::PipedStdio(_) => ChallengeExecutionMode::PipedStdio,
            Self::CoexecutedBenchmark(_) => ChallengeExecutionMode::CoexecutedBenchmark,
        }
    }

    /// Borrow the current piped-stdio execution contract.
    pub fn piped_stdio(&self) -> Option<&PipedStdioExecutionSpec> {
        match self {
            Self::SeparatedEvaluator(_) => None,
            Self::PipedStdio(spec) => Some(spec),
            Self::CoexecutedBenchmark(_) => None,
        }
    }

    /// Borrow the current coexecuted-evaluator contract.
    pub fn coexecuted_benchmark(&self) -> Option<&CoexecutedBenchmarkExecutionSpec> {
        match self {
            Self::SeparatedEvaluator(_) | Self::PipedStdio(_) => None,
            Self::CoexecutedBenchmark(spec) => Some(spec),
        }
    }

    /// Borrow the trusted evaluator command contract for the current topology.
    pub fn trusted_evaluator(&self) -> &EvaluatorSpec {
        match self {
            Self::SeparatedEvaluator(spec) => &spec.separated_evaluator,
            Self::PipedStdio(spec) => &spec.interactive_evaluator,
            Self::CoexecutedBenchmark(spec) => &spec.coexecuted_evaluator,
        }
    }

    /// Borrow public validation run locator if declared.
    pub fn validation_runs(&self) -> Option<&BundleRelativePath> {
        match self {
            Self::SeparatedEvaluator(spec) => spec.validation_runs.as_ref(),
            Self::PipedStdio(_) | Self::CoexecutedBenchmark(_) => None,
        }
    }

    /// Borrow public validation setup contract if declared.
    pub fn validation_setup(&self) -> Option<&ChallengeSetupSpec> {
        match self {
            Self::SeparatedEvaluator(spec) => spec.validation_setup.as_ref(),
            Self::PipedStdio(_) | Self::CoexecutedBenchmark(_) => None,
        }
    }

    /// Borrow official benchmark run locator if declared.
    pub fn official_runs(&self) -> Option<&BundleRelativePath> {
        match self {
            Self::SeparatedEvaluator(spec) => spec.official_runs.as_ref(),
            Self::PipedStdio(_) | Self::CoexecutedBenchmark(_) => None,
        }
    }

    /// Borrow official benchmark setup contract if declared.
    pub fn official_evaluation_setup(&self) -> Option<&ChallengeSetupSpec> {
        match self {
            Self::SeparatedEvaluator(spec) => spec.official_evaluation_setup.as_ref(),
            Self::PipedStdio(_) | Self::CoexecutedBenchmark(_) => None,
        }
    }
}

/// Current separated-container evaluator topology.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeparatedEvaluatorExecutionSpec {
    pub separated_evaluator: EvaluatorSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_runs: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_setup: Option<ChallengeSetupSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_runs: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_evaluation_setup: Option<ChallengeSetupSpec>,
}

/// Interactive topology where a trusted interactive-evaluator exchanges stdio with one solution run.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipedStdioExecutionSpec {
    pub interactive_evaluator: EvaluatorSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_session: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_setup: Option<PipedStdioSetupSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_session: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_evaluation_setup: Option<PipedStdioSetupSpec>,
}

/// Coexecuted topology where a trusted coexecuted-evaluator imports participant code in one container.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoexecutedBenchmarkExecutionSpec {
    pub coexecuted_evaluator: EvaluatorSpec,
    pub acknowledge_danger: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_setup: Option<CoexecutedBenchmarkSetupSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_evaluation_setup: Option<CoexecutedBenchmarkSetupSpec>,
}

/// Public execution metadata that excludes official private benchmark locators.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PublicChallengeExecutionSpec {
    SeparatedEvaluator(PublicSeparatedEvaluatorExecutionSpec),
    PipedStdio(PublicPipedStdioExecutionSpec),
    CoexecutedBenchmark(PublicCoexecutedBenchmarkExecutionSpec),
}

impl PublicChallengeExecutionSpec {
    /// Borrow the trusted evaluator command contract for the public execution topology.
    pub fn trusted_evaluator(&self) -> &EvaluatorSpec {
        match self {
            Self::SeparatedEvaluator(spec) => &spec.separated_evaluator,
            Self::PipedStdio(spec) => &spec.interactive_evaluator,
            Self::CoexecutedBenchmark(spec) => &spec.coexecuted_evaluator,
        }
    }
}

impl From<ChallengeExecutionSpec> for PublicChallengeExecutionSpec {
    fn from(execution: ChallengeExecutionSpec) -> Self {
        match execution {
            ChallengeExecutionSpec::SeparatedEvaluator(spec) => {
                Self::SeparatedEvaluator(PublicSeparatedEvaluatorExecutionSpec {
                    separated_evaluator: spec.separated_evaluator,
                    validation_runs: spec.validation_runs,
                    validation_setup: spec.validation_setup,
                })
            }
            ChallengeExecutionSpec::PipedStdio(spec) => {
                Self::PipedStdio(PublicPipedStdioExecutionSpec {
                    interactive_evaluator: spec.interactive_evaluator,
                    validation_session: spec.validation_session,
                    validation_setup: spec.validation_setup,
                })
            }
            ChallengeExecutionSpec::CoexecutedBenchmark(spec) => {
                Self::CoexecutedBenchmark(PublicCoexecutedBenchmarkExecutionSpec {
                    coexecuted_evaluator: spec.coexecuted_evaluator,
                    acknowledge_danger: spec.acknowledge_danger,
                    validation_setup: spec.validation_setup,
                })
            }
        }
    }
}

/// Public separated-evaluator topology metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicSeparatedEvaluatorExecutionSpec {
    pub separated_evaluator: EvaluatorSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_runs: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_setup: Option<ChallengeSetupSpec>,
}

/// Public piped-stdio topology metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicPipedStdioExecutionSpec {
    pub interactive_evaluator: EvaluatorSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_session: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_setup: Option<PipedStdioSetupSpec>,
}

/// Public coexecuted-evaluator topology metadata.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicCoexecutedBenchmarkExecutionSpec {
    pub coexecuted_evaluator: EvaluatorSpec,
    pub acknowledge_danger: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_setup: Option<CoexecutedBenchmarkSetupSpec>,
}

/// Optional separated-evaluator command that sets up generated benchmark inputs.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChallengeSetupSpec {
    pub command: Vec<String>,
    /// Relative path, under the setup workspace, to the generated run manifest.
    pub result_runs_file: BundleRelativePath,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproducibility_notes: Option<String>,
}

/// Optional interactive-evaluator command that sets up one generated interactive session.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipedStdioSetupSpec {
    pub command: Vec<String>,
    /// Relative path, under the setup workspace, to the generated session manifest.
    pub result_session_file: BundleRelativePath,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproducibility_notes: Option<String>,
}

/// Optional coexecuted-evaluator command that sets up files for a coexecuted run.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoexecutedBenchmarkSetupSpec {
    pub command: Vec<String>,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproducibility_notes: Option<String>,
}

/// Challenge-owned list of evaluator-controlled solution invocations.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeRunManifest {
    #[serde(default)]
    pub runs: Vec<ChallengeRunSpec>,
}

/// One solution invocation generated by the worker and later evaluated by the evaluator.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeRunSpec {
    pub run_name: RunName,
    pub interface: ChallengeRunInterface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_files: Vec<ChallengeRunInputFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_files: Vec<RunOutputPath>,
}

/// Supported worker-managed solution input/output interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeRunInterface {
    Stdio,
    FileSystem,
}

/// One input file materialized into `AGENTICS_INPUT_DIR` for a file-mode run.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeRunInputFile {
    pub path: RunInputPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<BundleRelativePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_json: Option<serde_json::Value>,
}

/// Challenge-owned single interactive session manifest for `piped_stdio`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipedStdioSessionManifest {
    pub session_name: RunName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_files: Vec<ChallengeRunInputFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Dataset layout and visibility policy declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Directory containing private benchmark data or private setup config used by official runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_benchmark_dir: Option<BundleRelativePath>,
    /// Visibility policy for public validation case results.
    pub public_policy: super::evaluation::ScoreVisibility,
    /// Visibility policy for private benchmark results.
    pub private_benchmark_policy: PrivateBenchmarkPolicy,
    /// Whether official runs can evaluate against private benchmark data.
    pub private_benchmark_enabled: bool,
}

/// Public dataset metadata with private benchmark paths removed.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublicDatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Visibility policy for public validation case results.
    pub public_policy: super::evaluation::ScoreVisibility,
    /// Visibility policy for private benchmark results.
    pub private_benchmark_policy: PrivateBenchmarkPolicy,
    /// Whether official runs can evaluate against private benchmark data.
    pub private_benchmark_enabled: bool,
}

/// Visibility policy allowed for private benchmark results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrivateBenchmarkPolicy {
    ScoreOnly,
}

/// Whether a metric is better when it is larger or smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    Maximize,
    Minimize,
}

/// Visibility level for a metric emitted by the evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricVisibility {
    /// Visible in validation feedback and official result views.
    Public,
    /// Visible only after a ranking-visible official evaluation.
    Official,
}

/// One metric that an evaluator may emit in aggregate or per-run result payloads.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetricDefinitionSpec {
    pub name: MetricName,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub direction: MetricDirection,
    pub visibility: MetricVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_description: Option<String>,
}

/// Ranking configuration for a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RankingSpec {
    pub primary_metric_name: MetricName,
    #[serde(default)]
    #[schemars(required)]
    pub tie_breaker_metric_names: Vec<MetricName>,
}

/// Metric schema embedded in `spec.json`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MetricSchemaSpec {
    pub metrics: Vec<MetricDefinitionSpec>,
    pub ranking: RankingSpec,
}

impl MetricSchemaSpec {
    /// Look up a metric definition by name.
    pub fn metric(&self, metric_name: &MetricName) -> Option<&MetricDefinitionSpec> {
        self.metrics
            .iter()
            .find(|metric| &metric.name == metric_name)
    }

    /// Primary ranking metric declared by this challenge.
    pub fn primary_metric(&self) -> Option<&MetricDefinitionSpec> {
        self.metric(&self.ranking.primary_metric_name)
    }
}

impl Default for MetricSchemaSpec {
    /// Handles default for this module.
    fn default() -> Self {
        Self {
            metrics: vec![MetricDefinitionSpec {
                name: MetricName::score(),
                label: "Score".to_string(),
                unit: None,
                direction: MetricDirection::Maximize,
                visibility: MetricVisibility::Public,
                metric_description: Some("Challenge-defined compatibility score.".to_string()),
            }],
            ranking: RankingSpec {
                primary_metric_name: MetricName::score(),
                tie_breaker_metric_names: vec![],
            },
        }
    }
}

/// One row in the public challenge catalog.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeListItemDto {
    pub challenge_id: ChallengeId,
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub starts_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
}

/// Public challenge catalog response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeListResponse {
    pub items: Vec<ChallengeListItemDto>,
    pub total_count: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

/// Public Moltbook community metadata exposed on challenge detail surfaces.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MoltbookCommunityDto {
    pub submolt_name: MoltbookSubmoltName,
    pub submolt_url: MoltbookSubmoltUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_url: Option<MoltbookPostUrl>,
}

/// Public challenge detail response with spec and Markdown statement.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeDetailResponse {
    pub challenge_id: ChallengeId,
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub spec: PublicChallengeBundleSpec,
    pub statement_markdown: String,
    pub moltbook: MoltbookCommunityDto,
}

/// Admin-facing challenge metadata response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChallengeAdminResponse {
    pub challenge_id: ChallengeId,
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[serde(default)]
    pub keywords: Vec<ChallengeKeyword>,
    pub status: ChallengeLifecycleStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// One row in the admin challenge list.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengeListItemDto {
    pub challenge_id: ChallengeId,
    pub challenge_name: ChallengeName,
    pub title: String,
    pub summary: LocalizedText,
    #[serde(default)]
    pub keywords: Vec<ChallengeKeyword>,
    pub status: ChallengeLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<ChallengeTargetSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<ChallengeEligibilitySpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ChallengeVisibilitySpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution_publication: Option<ChallengeSolutionPublicationPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_benchmark_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moltbook_discussion_url: Option<MoltbookPostUrl>,
    pub created_at: String,
    pub updated_at: String,
}

/// Admin challenge list response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminChallengeListResponse {
    pub items: Vec<AdminChallengeListItemDto>,
}

/// Admin response returned after publishing a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublishChallengeResponse {
    pub challenge_id: ChallengeId,
    pub challenge_name: ChallengeName,
    pub title: String,
    pub bundle_path: ManagedBundlePath,
    pub public_bundle_path: ManagedBundlePath,
    pub statement_path: ManagedStatementPath,
}
