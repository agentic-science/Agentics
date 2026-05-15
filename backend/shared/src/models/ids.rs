//! Validated domain identifiers shared by API, database, and CLI DTOs.

use std::borrow::Cow;
use std::fmt;

use nutype::nutype;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};

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

fn validate_challenge_id(value: &str) -> Result<(), ChallengeIdError> {
    if is_valid_challenge_id(value) {
        Ok(())
    } else {
        Err(ChallengeIdError)
    }
}

#[cfg(test)]
mod tests {
    use super::{ChallengeId, is_valid_challenge_id};

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
}
