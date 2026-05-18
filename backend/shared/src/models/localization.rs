//! Localized text contracts shared by API, bundle, and frontend DTOs.

use std::fmt;

use serde::{Deserialize, Serialize};

/// English and Chinese text for short public challenge copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalizedText {
    pub en: String,
    pub zh: String,
}

impl LocalizedText {
    /// Build localized text from required English and Chinese values.
    pub fn new(en: impl Into<String>, zh: impl Into<String>) -> Self {
        Self {
            en: en.into(),
            zh: zh.into(),
        }
    }
}

impl fmt::Display for LocalizedText {
    /// Formats both locales for validation diagnostics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "en={}, zh={}", self.en, self.zh)
    }
}
