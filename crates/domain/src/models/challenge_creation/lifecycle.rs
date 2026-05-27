//! Challenge draft lifecycle values.

use serde::{Deserialize, Serialize};

/// Draft status used by the review lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDraftStatus {
    Draft,
    Validated,
    Approved,
    Publishing,
    Rejected,
    Published,
    Abandoned,
}

impl ChallengeDraftStatus {
    /// Stable database string for this draft status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Validated => "validated",
            Self::Approved => "approved",
            Self::Publishing => "publishing",
            Self::Rejected => "rejected",
            Self::Published => "published",
            Self::Abandoned => "abandoned",
        }
    }

    /// Parse a stable database string for this draft status.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "validated" => Some(Self::Validated),
            "approved" => Some(Self::Approved),
            "publishing" => Some(Self::Publishing),
            "rejected" => Some(Self::Rejected),
            "published" => Some(Self::Published),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

/// Validation record status for a challenge draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDraftValidationStatus {
    Running,
    Passed,
    Failed,
}

impl ChallengeDraftValidationStatus {
    /// Stable database string for this validation outcome.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    /// Parse a stable database string for this validation outcome.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
