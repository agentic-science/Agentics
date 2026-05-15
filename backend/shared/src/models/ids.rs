//! Validated domain identifiers shared by API, database, and CLI DTOs.

use std::borrow::Cow;
use std::fmt;

use nutype::nutype;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use uuid::Uuid;

/// User-facing validation message for challenge ids.
pub const CHALLENGE_ID_ERROR_MESSAGE: &str = "challenge_id must be 3-63 lowercase ASCII letters, digits, or single hyphens, and must start and end with a letter or digit";

/// Validation failure for [`ChallengeId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeIdError;

impl fmt::Display for ChallengeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(CHALLENGE_ID_ERROR_MESSAGE)
    }
}

impl std::error::Error for ChallengeIdError {}

/// User-facing validation message for target name syntax.
///
/// This newtype only rejects malformed external strings. Whether a target is
/// supported for a challenge is checked against the published challenge spec.
pub const TARGET_NAME_ERROR_MESSAGE: &str = "target must be non-empty and contain only ASCII letters, digits, underscores, hyphens, or dots";

/// Validation failure for [`TargetName`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetNameError;

impl fmt::Display for TargetNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(TARGET_NAME_ERROR_MESSAGE)
    }
}

impl std::error::Error for TargetNameError {}

/// User-facing validation message for solution submission ids.
pub const SOLUTION_SUBMISSION_ID_ERROR_MESSAGE: &str =
    "solution_submission_id must be a canonical UUID string";

/// Validation failure for [`SolutionSubmissionId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolutionSubmissionIdError;

impl fmt::Display for SolutionSubmissionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SOLUTION_SUBMISSION_ID_ERROR_MESSAGE)
    }
}

impl std::error::Error for SolutionSubmissionIdError {}

#[nutype(
    validate(with = validate_challenge_id, error = ChallengeIdError),
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
pub struct ChallengeId(String);

impl ChallengeId {
    /// Borrow the canonical challenge id string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    validate(with = validate_target, error = TargetNameError),
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
pub struct TargetName(String);

impl TargetName {
    /// Borrow the canonical target name string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[nutype(
    validate(with = validate_solution_submission_id, error = SolutionSubmissionIdError),
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
pub struct SolutionSubmissionId(String);

impl SolutionSubmissionId {
    /// Borrow the canonical solution submission id string.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

impl JsonSchema for ChallengeId {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "ChallengeId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 3,
            "maxLength": 63,
            "pattern": "^[a-z0-9](?:[a-z0-9]|-(?!-)){1,61}[a-z0-9]$"
        })
    }
}

impl JsonSchema for TargetName {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "TargetName".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "minLength": 1,
            "pattern": "^[A-Za-z0-9_.-]+$"
        })
    }
}

impl JsonSchema for SolutionSubmissionId {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "SolutionSubmissionId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "format": "uuid",
            "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
        })
    }
}

/// Check whether a challenge id is valid in the public repository namespace.
pub fn is_valid_challenge_id(value: &str) -> bool {
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

fn has_target_syntax(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Check whether a solution submission id is a canonical hyphenated UUID.
pub fn is_valid_solution_submission_id(value: &str) -> bool {
    let Ok(uuid) = Uuid::parse_str(value) else {
        return false;
    };
    uuid.to_string() == value
}

fn validate_challenge_id(value: &str) -> Result<(), ChallengeIdError> {
    if is_valid_challenge_id(value) {
        Ok(())
    } else {
        Err(ChallengeIdError)
    }
}

fn validate_target(value: &str) -> Result<(), TargetNameError> {
    if has_target_syntax(value) {
        Ok(())
    } else {
        Err(TargetNameError)
    }
}

fn validate_solution_submission_id(value: &str) -> Result<(), SolutionSubmissionIdError> {
    if is_valid_solution_submission_id(value) {
        Ok(())
    } else {
        Err(SolutionSubmissionIdError)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChallengeId, SolutionSubmissionId, TargetName, is_valid_challenge_id,
        is_valid_solution_submission_id,
    };

    #[test]
    fn validates_challenge_ids() {
        assert!(is_valid_challenge_id("sample-sum"));
        assert!(ChallengeId::try_new("matrix-multiplication").is_ok());
        assert!(ChallengeId::try_new("Bad_ID").is_err());
        assert!(ChallengeId::try_new("-bad").is_err());
        assert!(ChallengeId::try_new("bad-").is_err());
        assert!(ChallengeId::try_new("bad--id").is_err());
        assert!(ChallengeId::try_new("ab").is_err());
        assert!(ChallengeId::try_new(" matrix").is_err());
        assert!(ChallengeId::try_new("matrix ").is_err());
        assert!(ChallengeId::try_new("matrix mult").is_err());
    }

    #[test]
    fn serde_rejects_invalid_challenge_ids() {
        let parsed: ChallengeId =
            serde_json::from_str("\"sample-sum\"").expect("valid challenge id should deserialize");
        assert_eq!(parsed.as_str(), "sample-sum");
        assert!(serde_json::from_str::<ChallengeId>("\"sample sum\"").is_err());
    }

    #[test]
    fn validates_targets() {
        assert!(TargetName::try_new("linux-arm64-cpu").is_ok());
        assert!(TargetName::try_new("score.v1").is_ok());
        assert!(TargetName::try_new("cuda_12").is_ok());
        assert!(TargetName::try_new("").is_err());
        assert!(TargetName::try_new("linux arm64").is_err());
        assert!(TargetName::try_new("linux/arm64").is_err());
        assert!(TargetName::try_new("bad\ntarget").is_err());
    }

    #[test]
    fn validates_solution_submission_ids() {
        let valid = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        assert!(is_valid_solution_submission_id(valid));
        assert!(SolutionSubmissionId::try_new(valid).is_ok());
        assert!(SolutionSubmissionId::try_new("submission-1").is_err());
        assert!(SolutionSubmissionId::try_new("F47AC10B-58CC-4372-A567-0E02B2C3D479").is_err());
        assert!(SolutionSubmissionId::try_new("f47ac10b58cc4372a5670e02b2c3d479").is_err());
    }

    #[test]
    fn serde_rejects_invalid_target_and_submission_ids() {
        let target: TargetName =
            serde_json::from_str("\"linux-arm64-cpu\"").expect("valid target should parse");
        assert_eq!(target.as_str(), "linux-arm64-cpu");
        assert!(serde_json::from_str::<TargetName>("\"linux arm64\"").is_err());

        let submission: SolutionSubmissionId =
            serde_json::from_str("\"f47ac10b-58cc-4372-a567-0e02b2c3d479\"")
                .expect("valid submission id should parse");
        assert_eq!(submission.as_str(), "f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(serde_json::from_str::<SolutionSubmissionId>("\"submission-1\"").is_err());
    }
}
