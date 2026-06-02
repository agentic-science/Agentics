//! Challenge review record lifecycle values.

use serde::{Deserialize, Serialize};

/// Status used by the challenge review record lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeReviewRecordStatus {
    PendingReview,
    Validated,
    Approved,
    Publishing,
    Rejected,
    Published,
    Abandoned,
}

impl ChallengeReviewRecordStatus {
    /// Stable database string for this review record status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingReview => "pending_review",
            Self::Validated => "validated",
            Self::Approved => "approved",
            Self::Publishing => "publishing",
            Self::Rejected => "rejected",
            Self::Published => "published",
            Self::Abandoned => "abandoned",
        }
    }

    /// Parse a stable database string for this review record status.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "pending_review" => Some(Self::PendingReview),
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

/// Validation record status for a challenge review record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeReviewValidationStatus {
    Running,
    Passed,
    Failed,
}

impl ChallengeReviewValidationStatus {
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
