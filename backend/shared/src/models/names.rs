//! Validated human-authored names shared by API, database, and CLI DTOs.

use std::borrow::Cow;
use std::fmt;

use nutype::nutype;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

/// User-facing validation message for challenge names.
pub const CHALLENGE_NAME_ERROR_MESSAGE: &str = "challenge_name must be 3-63 lowercase ASCII letters, digits, or single hyphens, and must start and end with a letter or digit";

/// Validation failure for [`ChallengeName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeNameError;

impl fmt::Display for ChallengeNameError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(CHALLENGE_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for ChallengeNameError {}

/// User-facing validation message for target names.
pub const TARGET_NAME_ERROR_MESSAGE: &str = "target must be non-empty and contain only ASCII letters, digits, underscores, hyphens, or dots";

/// Validation failure for [`TargetName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetNameError;

impl fmt::Display for TargetNameError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(TARGET_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for TargetNameError {}

/// User-facing validation message for metric names.
pub const METRIC_NAME_ERROR_MESSAGE: &str = "metric_name must be non-empty and contain only ASCII letters, digits, underscores, hyphens, or dots";

/// Validation failure for [`MetricName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricNameError;

impl fmt::Display for MetricNameError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(METRIC_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for MetricNameError {}

/// User-facing validation message for private asset names.
pub const ASSET_NAME_ERROR_MESSAGE: &str = "asset_name must be non-empty and contain only ASCII letters, digits, underscores, hyphens, or dots";

/// Validation failure for [`AssetName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetNameError;

impl fmt::Display for AssetNameError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(ASSET_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for AssetNameError {}

/// User-facing validation message for challenge run names.
pub const RUN_NAME_ERROR_MESSAGE: &str = "run_name must be non-empty and contain only ASCII letters, digits, underscores, hyphens, or dots";

/// Validation failure for [`RunName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunNameError;

impl fmt::Display for RunNameError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(RUN_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for RunNameError {}

/// User-facing validation message for resource profile names.
pub const RESOURCE_PROFILE_NAME_ERROR_MESSAGE: &str = "resource_profile.name must be non-empty and contain only ASCII letters, digits, underscores, hyphens, or dots";

/// Validation failure for [`ResourceProfileName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceProfileNameError;

impl fmt::Display for ResourceProfileNameError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(RESOURCE_PROFILE_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for ResourceProfileNameError {}

#[nutype(
    sanitize(trim, lowercase),
    validate(with = validate_challenge_name, error = ChallengeNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        Serialize,
        Deserialize,
        FromStr,
        TryFrom,
    ),
)]
/// Carries challenge name data across this module boundary.
pub struct ChallengeName(String);

impl ChallengeName {
    /// Borrow the canonical challenge name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    validate(with = validate_target_name, error = TargetNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        Serialize,
        Deserialize,
        FromStr,
        TryFrom,
    ),
)]
/// Carries target name data across this module boundary.
pub struct TargetName(String);

impl TargetName {
    /// Borrow the canonical target name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    validate(with = validate_asset_name, error = AssetNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        Serialize,
        Deserialize,
        FromStr,
        TryFrom,
    ),
)]
/// Carries asset name data across this module boundary.
pub struct AssetName(String);

impl AssetName {
    /// Borrow the canonical private asset name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    validate(with = validate_run_name, error = RunNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        Serialize,
        Deserialize,
        FromStr,
        TryFrom,
    ),
)]
/// Carries run name data across this module boundary.
pub struct RunName(String);

impl RunName {
    /// Borrow the canonical scorer run name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    validate(
        with = validate_resource_profile_name,
        error = ResourceProfileNameError
    ),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        Serialize,
        Deserialize,
        FromStr,
        TryFrom,
    ),
)]
/// Carries resource profile name data across this module boundary.
pub struct ResourceProfileName(String);

impl ResourceProfileName {
    /// Borrow the canonical resource profile name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    sanitize(trim),
    validate(with = validate_metric_name, error = MetricNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        Serialize,
        Deserialize,
        FromStr,
        TryFrom,
    ),
)]
/// Carries metric name data across this module boundary.
pub struct MetricName(String);

impl MetricName {
    /// Borrow the canonical metric name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Built-in compatibility metric used by legacy scorers.
    #[allow(
        clippy::panic,
        reason = "the built-in `score` metric name is a hard-coded valid literal"
    )]
    /// Handles score for this module.
    pub fn score() -> Self {
        match Self::try_new("score".to_string()) {
            Ok(metric_name) => metric_name,
            Err(_) => panic!("built-in metric name `score` must be valid"),
        }
    }
}

impl JsonSchema for ChallengeName {
    /// Handles inline schema for this module.
    fn inline_schema() -> bool {
        true
    }

    /// Handles schema name for this module.
    fn schema_name() -> Cow<'static, str> {
        "ChallengeName".into()
    }

    /// Handles json schema for this module.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 3,
            "maxLength": 63,
            "pattern": "^[a-z0-9](?:[a-z0-9]|-(?!-)){1,61}[a-z0-9]$"
        })
    }
}

macro_rules! impl_token_json_schema {
    ($type_name:ident, $schema_name:literal) => {
        impl JsonSchema for $type_name {
            /// Handles inline schema for this module.
            fn inline_schema() -> bool {
                true
            }

            /// Handles schema name for this module.
            fn schema_name() -> Cow<'static, str> {
                $schema_name.into()
            }

            /// Handles json schema for this module.
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({
                    "type": "string",
                    "minLength": 1,
                    "pattern": "^[A-Za-z0-9_.-]+$"
                })
            }
        }
    };
}

impl_token_json_schema!(TargetName, "TargetName");
impl_token_json_schema!(MetricName, "MetricName");
impl_token_json_schema!(AssetName, "AssetName");
impl_token_json_schema!(RunName, "RunName");
impl_token_json_schema!(ResourceProfileName, "ResourceProfileName");

/// Check whether a challenge name is valid in the public repository namespace.
pub fn is_valid_challenge_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(3..=63).contains(&bytes.len()) {
        return false;
    }
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return false;
    }
    if value.contains("--") {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Returns whether name token syntax is present.
fn has_name_token_syntax(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Validates challenge name invariants for this contract.
fn validate_challenge_name(value: &str) -> Result<(), ChallengeNameError> {
    if is_valid_challenge_name(value) {
        Ok(())
    } else {
        Err(ChallengeNameError)
    }
}

/// Validates target name invariants for this contract.
fn validate_target_name(value: &str) -> Result<(), TargetNameError> {
    if has_name_token_syntax(value) {
        Ok(())
    } else {
        Err(TargetNameError)
    }
}

/// Validates metric name invariants for this contract.
fn validate_metric_name(value: &str) -> Result<(), MetricNameError> {
    if has_name_token_syntax(value) {
        Ok(())
    } else {
        Err(MetricNameError)
    }
}

/// Validates asset name invariants for this contract.
fn validate_asset_name(value: &str) -> Result<(), AssetNameError> {
    if has_name_token_syntax(value) {
        Ok(())
    } else {
        Err(AssetNameError)
    }
}

/// Validates run name invariants for this contract.
fn validate_run_name(value: &str) -> Result<(), RunNameError> {
    if has_name_token_syntax(value) {
        Ok(())
    } else {
        Err(RunNameError)
    }
}

/// Validates resource profile name invariants for this contract.
fn validate_resource_profile_name(value: &str) -> Result<(), ResourceProfileNameError> {
    if has_name_token_syntax(value) {
        Ok(())
    } else {
        Err(ResourceProfileNameError)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssetName, ChallengeName, MetricName, ResourceProfileName, RunName, TargetName,
        is_valid_challenge_name,
    };

    /// Verifies that validates challenge names.
    #[test]
    fn validates_challenge_names() {
        assert!(is_valid_challenge_name("sample-sum"));
        assert!(ChallengeName::try_new("matrix-multiplication").is_ok());
        let canonical = ChallengeName::try_new(" Matrix-Multiplication ")
            .expect("challenge names should be lowercased and trimmed");
        assert_eq!(canonical.as_str(), "matrix-multiplication");
        assert!(ChallengeName::try_new("Bad_ID").is_err());
        assert!(ChallengeName::try_new("-bad").is_err());
        assert!(ChallengeName::try_new("bad-").is_err());
        assert!(ChallengeName::try_new("bad--id").is_err());
        assert!(ChallengeName::try_new("ab").is_err());
        assert!(ChallengeName::try_new("matrix mult").is_err());
    }

    /// Verifies that validates token names.
    #[test]
    fn validates_token_names() {
        for value in ["linux-arm64-cpu", "score.v1", "cuda_12"] {
            assert!(TargetName::try_new(value).is_ok());
            assert!(MetricName::try_new(value).is_ok());
            assert!(AssetName::try_new(value).is_ok());
            assert!(RunName::try_new(value).is_ok());
            assert!(ResourceProfileName::try_new(value).is_ok());
        }
        for value in ["", "linux arm64", "linux/arm64", "bad\ntarget"] {
            assert!(TargetName::try_new(value).is_err());
            assert!(MetricName::try_new(value).is_err());
            assert!(AssetName::try_new(value).is_err());
            assert!(RunName::try_new(value).is_err());
            assert!(ResourceProfileName::try_new(value).is_err());
        }
        let metric = MetricName::try_new(" runtime_ms ").expect("metric names trim edge spaces");
        assert_eq!(metric.as_str(), "runtime_ms");
        assert!(MetricName::try_new("runtime ms").is_err());
    }

    /// Verifies that serde rejects invalid names.
    #[test]
    fn serde_rejects_invalid_names() {
        let challenge: ChallengeName =
            serde_json::from_str("\"sample-sum\"").expect("valid challenge name should parse");
        assert_eq!(challenge.as_str(), "sample-sum");
        let challenge: ChallengeName =
            serde_json::from_str("\" Sample-Sum \"").expect("challenge name should canonicalize");
        assert_eq!(challenge.as_str(), "sample-sum");
        assert!(serde_json::from_str::<ChallengeName>("\"sample sum\"").is_err());

        let target: TargetName =
            serde_json::from_str("\"linux-arm64-cpu\"").expect("valid target should parse");
        assert_eq!(target.as_str(), "linux-arm64-cpu");
        assert!(serde_json::from_str::<TargetName>("\"linux arm64\"").is_err());

        let metric: MetricName =
            serde_json::from_str("\"runtime_ms\"").expect("valid metric name should parse");
        assert_eq!(metric.as_str(), "runtime_ms");
        let metric: MetricName =
            serde_json::from_str("\" runtime_ms \"").expect("metric name should trim");
        assert_eq!(metric.as_str(), "runtime_ms");
        assert!(serde_json::from_str::<MetricName>("\"runtime ms\"").is_err());
    }
}
