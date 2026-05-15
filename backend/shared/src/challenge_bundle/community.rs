//! Community integration validation for challenge bundles.

use crate::error::{AppError, Result};
use crate::models::challenge::ChallengeBundleSpec;

use super::require_non_empty;

/// Validate optional Moltbook community metadata for a challenge bundle.
pub(super) fn validate_community(spec: &ChallengeBundleSpec) -> Result<()> {
    let Some(community) = &spec.community else {
        return Ok(());
    };

    let has_name = community
        .moltbook_submolt_name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_url = community.moltbook_submolt_url.as_ref().is_some();
    if !has_name && !has_url {
        return Err(AppError::Validation(
            "community must declare moltbook_submolt_name or moltbook_submolt_url".to_string(),
        ));
    }

    if let Some(name) = &community.moltbook_submolt_name {
        validate_moltbook_submolt_name(name)?;
    }
    Ok(())
}

/// Validate the short Moltbook Submolt handle stored in challenge metadata.
fn validate_moltbook_submolt_name(value: &str) -> Result<()> {
    require_non_empty(value, "community.moltbook_submolt_name")?;
    if value.chars().count() > 80 {
        return Err(AppError::Validation(
            "community.moltbook_submolt_name must be at most 80 characters".to_string(),
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::Validation(
            "community.moltbook_submolt_name must contain only ASCII letters, digits, underscores, hyphens, or dots"
                .to_string(),
        ));
    }

    Ok(())
}
