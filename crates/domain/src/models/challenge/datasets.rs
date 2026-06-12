//! Dataset layout and benchmark visibility metadata.

use serde::{Deserialize, Serialize};

use crate::models::paths::BundleRelativePath;

use super::serde_helpers::{required_nullable, required_nullable_schema};

/// Dataset layout and visibility policy declared by a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Directory containing private benchmark data or private setup config used by official runs.
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(
        required,
        schema_with = "required_nullable_schema::<BundleRelativePath>"
    )]
    pub private_benchmark_dir: Option<BundleRelativePath>,
    /// Visibility policy for public validation case results.
    pub public_policy: crate::models::evaluation::ScoreVisibility,
    /// Visibility policy for private benchmark results.
    pub private_benchmark_policy: PrivateBenchmarkPolicy,
    /// Whether official runs can evaluate against private benchmark data.
    pub private_benchmark_enabled: bool,
}

/// Public dataset metadata with private benchmark paths removed.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicDatasetsSpec {
    /// Directory containing data that agents may inspect and use for validation.
    pub public_dir: BundleRelativePath,
    /// Visibility policy for public validation case results.
    pub public_policy: crate::models::evaluation::ScoreVisibility,
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
