//! GitHub-specific typed scalar values shared across creation flows.

use std::borrow::Cow;
use std::fmt;

use nutype::nutype;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// User-facing validation message for GitHub pull request numbers.
pub const GITHUB_PULL_REQUEST_NUMBER_ERROR_MESSAGE: &str = "pr_number must be a positive integer";

/// Validation failure for [`GithubPullRequestNumber`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GithubPullRequestNumberError;

impl fmt::Display for GithubPullRequestNumberError {
    /// Formats the validation error as the public contract message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(GITHUB_PULL_REQUEST_NUMBER_ERROR_MESSAGE)
    }
}

impl std::error::Error for GithubPullRequestNumberError {}

#[nutype(
    sanitize(trim),
    validate(
        with = validate_github_pull_request_number,
        error = GithubPullRequestNumberError
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
        FromStr,
        TryFrom,
    ),
)]
/// Validated GitHub pull request number.
pub struct GithubPullRequestNumber(String);

impl GithubPullRequestNumber {
    /// Borrow the canonical decimal pull request number.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Return the numeric pull request number for database/API boundaries.
    pub fn as_i32(&self) -> Result<i32, GithubPullRequestNumberError> {
        self.as_str()
            .parse::<i32>()
            .map_err(|_| GithubPullRequestNumberError)
    }
}

impl Serialize for GithubPullRequestNumber {
    /// Serializes the validated pull request number as a JSON number.
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = self.as_i32().map_err(serde::ser::Error::custom)?;
        serializer.serialize_i32(value)
    }
}

impl<'de> Deserialize<'de> for GithubPullRequestNumber {
    /// Deserializes a JSON number into the validated pull request number type.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        let value = i32::try_from(value).map_err(serde::de::Error::custom)?;
        Self::try_new(value.to_string()).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for GithubPullRequestNumber {
    /// Keeps this scalar inline in generated JSON schemas.
    fn inline_schema() -> bool {
        true
    }

    /// Names the generated schema for this GitHub scalar.
    fn schema_name() -> Cow<'static, str> {
        "GithubPullRequestNumber".into()
    }

    /// Describes the public JSON contract for pull request numbers.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "integer",
            "minimum": 1,
            "maximum": i32::MAX
        })
    }
}

/// Validates that a pull request number is a canonical positive decimal integer.
fn validate_github_pull_request_number(value: &str) -> Result<(), GithubPullRequestNumberError> {
    if value.is_empty() || value.starts_with('+') || value.starts_with('0') && value != "0" {
        return Err(GithubPullRequestNumberError);
    }
    let Ok(number) = value.parse::<i32>() else {
        return Err(GithubPullRequestNumberError);
    };
    if number > 0 {
        Ok(())
    } else {
        Err(GithubPullRequestNumberError)
    }
}

#[cfg(test)]
mod tests {
    use super::GithubPullRequestNumber;

    /// Verifies that pull request numbers trim and validate CLI strings.
    #[test]
    fn validates_pull_request_numbers() {
        let number = GithubPullRequestNumber::try_new(" 42 ".to_string())
            .expect("positive PR number should parse");
        assert_eq!(number.as_str(), "42");
        assert_eq!(number.as_i32().expect("number should fit i32"), 42);
        for value in ["", "0", "-1", "42abc", "42.9", "01"] {
            assert!(GithubPullRequestNumber::try_new(value.to_string()).is_err());
        }
    }

    /// Verifies that JSON remains numeric for pull request numbers.
    #[test]
    fn serde_uses_numeric_json() {
        let number: GithubPullRequestNumber =
            serde_json::from_str("42").expect("numeric PR number should deserialize");
        assert_eq!(number.as_str(), "42");
        assert_eq!(
            serde_json::to_string(&number).expect("PR number should serialize"),
            "42"
        );
        assert!(serde_json::from_str::<GithubPullRequestNumber>("\"42\"").is_err());
    }
}
