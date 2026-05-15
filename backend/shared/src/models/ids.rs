//! Validated generated identifiers shared by API, database, and CLI DTOs.

use std::borrow::Cow;
use std::fmt;

use nutype::nutype;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use uuid::Uuid;

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

/// Check whether a solution submission id is a canonical hyphenated UUID.
pub fn is_valid_solution_submission_id(value: &str) -> bool {
    let Ok(uuid) = Uuid::parse_str(value) else {
        return false;
    };
    uuid.to_string() == value
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
    use super::{SolutionSubmissionId, is_valid_solution_submission_id};

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
    fn serde_rejects_invalid_solution_submission_ids() {
        let submission: SolutionSubmissionId =
            serde_json::from_str("\"f47ac10b-58cc-4372-a567-0e02b2c3d479\"")
                .expect("valid submission id should parse");
        assert_eq!(submission.as_str(), "f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(serde_json::from_str::<SolutionSubmissionId>("\"submission-1\"").is_err());
    }
}
