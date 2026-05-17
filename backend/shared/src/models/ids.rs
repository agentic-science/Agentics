//! Validated generated identifiers shared by API, database, and CLI DTOs.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Validation failure for generated UUID identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UuidIdError {
    message: &'static str,
}

impl UuidIdError {
    const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl fmt::Display for UuidIdError {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for UuidIdError {}

macro_rules! define_uuid_id_type {
    ($type_name:ident, $schema_name:literal, $message:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type_name(String);

        impl $type_name {
            /// Create a new random generated UUID identifier.
            pub fn generate() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            /// Parse and canonicalize a generated UUID identifier.
            pub fn try_new(value: impl AsRef<str>) -> Result<Self, UuidIdError> {
                let value = value.as_ref();
                if value.trim() != value {
                    return Err(UuidIdError::new($message));
                }
                let canonical = value.to_ascii_lowercase();
                let Ok(uuid) = Uuid::parse_str(&canonical) else {
                    return Err(UuidIdError::new($message));
                };
                if uuid.to_string() != canonical {
                    return Err(UuidIdError::new($message));
                }
                Ok(Self(canonical))
            }

            /// Borrow the canonical UUID string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $type_name {
            /// Handles fmt for this module.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $type_name {
            /// Returns ref in the representation required by callers.
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl FromStr for $type_name {
            type Err = UuidIdError;

            /// Handles from str for this module.
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_new(value)
            }
        }

        impl Serialize for $type_name {
            /// Handles serialize for this module.
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $type_name {
            /// Handles deserialize for this module.
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_new(&value).map_err(serde::de::Error::custom)
            }
        }

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
                    "format": "uuid",
                    "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
                })
            }
        }
    };
}

define_uuid_id_type!(
    AgentId,
    "AgentId",
    "agent_id must be a canonical UUID string"
);
define_uuid_id_type!(
    AgentPioneerCodeId,
    "AgentPioneerCodeId",
    "agent_pioneer_code_id must be a canonical UUID string"
);
define_uuid_id_type!(
    ChallengeDraftId,
    "ChallengeDraftId",
    "challenge_draft_id must be a canonical UUID string"
);
define_uuid_id_type!(
    ChallengePrivateAssetId,
    "ChallengePrivateAssetId",
    "challenge_private_asset_id must be a canonical UUID string"
);
define_uuid_id_type!(
    ChallengeDraftValidationRecordId,
    "ChallengeDraftValidationRecordId",
    "challenge_draft_validation_record_id must be a canonical UUID string"
);
define_uuid_id_type!(
    ChallengeDraftAuditEventId,
    "ChallengeDraftAuditEventId",
    "challenge_draft_audit_event_id must be a canonical UUID string"
);
define_uuid_id_type!(
    ChallengeShortlistRevisionId,
    "ChallengeShortlistRevisionId",
    "challenge_shortlist_revision_id must be a canonical UUID string"
);
define_uuid_id_type!(
    EvaluationJobId,
    "EvaluationJobId",
    "evaluation_job_id must be a canonical UUID string"
);
define_uuid_id_type!(
    EvaluationId,
    "EvaluationId",
    "evaluation_id must be a canonical UUID string"
);
define_uuid_id_type!(
    SolutionSubmissionId,
    "SolutionSubmissionId",
    "solution_submission_id must be a canonical UUID string"
);

#[cfg(test)]
mod tests {
    use super::{AgentId, ChallengeDraftId, SolutionSubmissionId};

    /// Verifies that validates solution submission ids.
    #[test]
    fn validates_solution_submission_ids() {
        let valid = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        assert!(SolutionSubmissionId::try_new(valid).is_ok());
        let canonical = SolutionSubmissionId::try_new("F47AC10B-58CC-4372-A567-0E02B2C3D479")
            .expect("UUID hex case should canonicalize");
        assert_eq!(canonical.as_str(), valid);
        assert!(SolutionSubmissionId::try_new("submission-1").is_err());
        assert!(SolutionSubmissionId::try_new(" f47ac10b-58cc-4372-a567-0e02b2c3d479").is_err());
        assert!(SolutionSubmissionId::try_new("f47ac10b58cc4372a5670e02b2c3d479").is_err());
    }

    /// Verifies that serde rejects invalid solution submission ids.
    #[test]
    fn serde_rejects_invalid_solution_submission_ids() {
        let submission: SolutionSubmissionId =
            serde_json::from_str("\"f47ac10b-58cc-4372-a567-0e02b2c3d479\"")
                .expect("valid submission id should parse");
        assert_eq!(submission.as_str(), "f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(serde_json::from_str::<SolutionSubmissionId>("\"submission-1\"").is_err());
    }

    /// Verifies that generated uuid ids canonicalize hex case.
    #[test]
    fn generated_uuid_ids_canonicalize_hex_case() {
        let canonical = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        assert_eq!(
            AgentId::try_new("F47AC10B-58CC-4372-A567-0E02B2C3D479")
                .expect("UUID hex case should canonicalize")
                .as_str(),
            canonical
        );
        assert_eq!(
            ChallengeDraftId::try_new(canonical)
                .expect("challenge draft id should parse")
                .as_str(),
            canonical
        );
        assert!(ChallengeDraftId::try_new(format!(" {canonical}")).is_err());
        assert!(ChallengeDraftId::try_new("f47ac10b58cc4372a5670e02b2c3d479").is_err());
    }
}
