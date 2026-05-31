use serde::{Deserialize, Serialize};

use super::super::names::RunName;
use super::super::paths::{BundleRelativePath, RunInputPath, RunOutputPath};

/// Evaluator entrypoint and output-file contract for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct EvaluatorSpec {
    #[garde(
        length(min = 1),
        inner(
            custom(crate::validation::trimmed_non_empty),
            custom(crate::validation::no_nul)
        )
    )]
    pub command: Vec<String>,
    pub result_file: BundleRelativePath,
}

/// Supported challenge execution topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeExecutionMode {
    SeparatedEvaluator,
    PipedStdio,
    CoexecutedBenchmark,
}

impl ChallengeExecutionMode {
    /// Return the stable runtime name used for container labels and bundle script directories.
    pub fn runtime_name(self) -> &'static str {
        match self {
            Self::SeparatedEvaluator => "separated-evaluator",
            Self::PipedStdio => "interactive-evaluator",
            Self::CoexecutedBenchmark => "coexecuted-evaluator",
        }
    }
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

    /// Return whether the official evaluator declares setup-generated official inputs.
    pub fn has_official_evaluation_setup(&self) -> bool {
        match self {
            Self::SeparatedEvaluator(spec) => spec.official_evaluation_setup.is_some(),
            Self::PipedStdio(spec) => spec.official_evaluation_setup.is_some(),
            Self::CoexecutedBenchmark(spec) => spec.official_evaluation_setup.is_some(),
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
    pub acknowledge_stdio_protocol_framing: bool,
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
                    acknowledge_stdio_protocol_framing: spec.acknowledge_stdio_protocol_framing,
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
    pub acknowledge_stdio_protocol_framing: bool,
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
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct ChallengeSetupSpec {
    #[garde(
        length(min = 1),
        inner(
            custom(crate::validation::trimmed_non_empty),
            custom(crate::validation::no_nul)
        )
    )]
    pub command: Vec<String>,
    /// Relative path, under the setup workspace, to the generated run manifest.
    pub result_runs_file: BundleRelativePath,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub reproducibility_notes: Option<String>,
}

/// Optional interactive-evaluator command that sets up one generated interactive session.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct PipedStdioSetupSpec {
    #[garde(
        length(min = 1),
        inner(
            custom(crate::validation::trimmed_non_empty),
            custom(crate::validation::no_nul)
        )
    )]
    pub command: Vec<String>,
    /// Relative path, under the setup workspace, to the generated session manifest.
    pub result_session_file: BundleRelativePath,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
    pub reproducibility_notes: Option<String>,
}

/// Optional coexecuted-evaluator command that sets up files for a coexecuted run.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct CoexecutedBenchmarkSetupSpec {
    #[garde(
        length(min = 1),
        inner(
            custom(crate::validation::trimmed_non_empty),
            custom(crate::validation::no_nul)
        )
    )]
    pub command: Vec<String>,
    /// Challenge-owner notes about seeds, versions, or external data provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(custom(crate::validation::optional_trimmed_non_empty))]
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
