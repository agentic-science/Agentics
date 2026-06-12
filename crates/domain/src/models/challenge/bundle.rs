//! Challenge bundle contracts and public bundle projections.

use serde::{Deserialize, Serialize};

use crate::models::localization::LocalizedText;
use crate::models::names::{ChallengeKeyword, ChallengeName, TargetName};
use crate::models::paths::BundleRelativePath;

use super::datasets::{DatasetsSpec, PublicDatasetsSpec};
use super::execution::{ChallengeExecutionSpec, PublicChallengeExecutionSpec};
use super::metrics::MetricSchemaSpec;
use super::serde_helpers::{required_nullable, required_nullable_schema};
use super::targets::ChallengeTargetSpec;

/// Minimum public keywords that a challenge must declare.
pub const MIN_CHALLENGE_KEYWORDS: usize = 1;

/// Maximum public keywords that a challenge may declare.
pub const MAX_CHALLENGE_KEYWORDS: usize = 6;

/// Parsed `spec.json` contract for a challenge bundle.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate, schemars::JsonSchema)]
#[garde(allow_unvalidated)]
#[serde(deny_unknown_fields)]
pub struct ChallengeBundleSpec {
    pub schema_version: i32,
    pub challenge_name: ChallengeName,
    pub challenge_title: String,
    /// Localized summary used in compact challenge catalog surfaces.
    pub summary: LocalizedText,
    /// Required public keywords used by catalog search and filtering.
    #[garde(length(min = MIN_CHALLENGE_KEYWORDS, max = MAX_CHALLENGE_KEYWORDS))]
    #[schemars(length(min = 1, max = 6))]
    pub keywords: Vec<ChallengeKeyword>,
    pub solution: SolutionSpec,
    pub targets: Vec<ChallengeTargetSpec>,
    pub starts_at: String,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    pub closes_at: Option<String>,
    pub eligibility: ChallengeEligibilitySpec,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<i64>")]
    pub validation_submission_limit: Option<i64>,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<i64>")]
    pub official_submission_limit: Option<i64>,
    pub visibility: ChallengeVisibilitySpec,
    pub solution_publication: ChallengeSolutionPublicationPolicy,
    pub execution: ChallengeExecutionSpec,
    pub datasets: DatasetsSpec,
    /// Metric definitions and ranking metadata used to interpret evaluator output.
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

    /// Return whether official runner diagnostics may contain private benchmark material.
    pub fn official_evaluation_may_expose_private_material(&self) -> bool {
        if self.datasets.private_benchmark_enabled || self.execution.has_official_evaluation_setup()
        {
            return true;
        }

        match &self.execution {
            ChallengeExecutionSpec::SeparatedEvaluator(spec) => {
                spec.official_runs.as_ref().is_none_or(|path| {
                    !path
                        .as_path()
                        .starts_with(self.datasets.public_dir.as_path())
                })
            }
            ChallengeExecutionSpec::PipedStdio(spec) => {
                spec.official_session.as_ref().is_none_or(|path| {
                    !path
                        .as_path()
                        .starts_with(self.datasets.public_dir.as_path())
                })
            }
            ChallengeExecutionSpec::CoexecutedBenchmark(_) => false,
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
#[serde(deny_unknown_fields)]
pub struct SolutionSpec {
    pub protocol: String,
    pub manifest_file: BundleRelativePath,
}
