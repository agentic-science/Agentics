use agentics_domain::models::challenge_creation::{
    ChallengeDraftCleanupResponse, ChallengeDraftResponse,
};
use agentics_domain::models::names::ChallengeName;
use anyhow::Result;

use crate::cli::OutputFormat;

use super::format::{pretty_json, status_label};

/// Renders challenge draft for user-facing output.
pub(crate) fn render_challenge_draft(
    response: &ChallengeDraftResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "challenge_draft: {}\nchallenge: {}\nrequest: {}\nstatus: {}\nrepo: {}#{}\npath: {}\ncommit: {}\nmanifest_sha256: {}\npublished_challenge: {}\nprivate_assets: {}\nvalidation_records: {}",
            response.id,
            response.challenge_name,
            status_label(&response.request),
            status_label(&response.status),
            response.repo_url,
            response.pr_number,
            response.challenge_path,
            response.commit_sha,
            response.manifest_sha256,
            response
                .published_challenge_name
                .as_ref()
                .map_or("none", ChallengeName::as_str),
            response.private_assets.len(),
            response.validation_records.len()
        )),
    }
}

/// Renders challenge draft cleanup for user-facing output.
pub(crate) fn render_challenge_draft_cleanup(
    response: &ChallengeDraftCleanupResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "abandoned_drafts: {}\npurged_private_assets: {}\npurged_temporary_storage_objects: {}",
            response.abandoned_drafts,
            response.purged_private_assets,
            response.purged_temporary_storage_objects
        )),
    }
}
