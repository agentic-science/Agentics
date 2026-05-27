//! Published challenge lifecycle values.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Persistent lifecycle state for a challenge shell or published benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeLifecycleStatus {
    Draft,
    Active,
    Archived,
}

impl ChallengeLifecycleStatus {
    /// Stable database string for a challenge lifecycle state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    /// Parse a stable database string for a challenge lifecycle state.
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

impl fmt::Display for ChallengeLifecycleStatus {
    /// Format the challenge status as its stable persisted and wire value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
