use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetResponse, ChallengeReviewRecordCleanupResponse,
    ChallengeReviewRecordResponse, CreatorChallengeReviewRecordResponse,
};
use agentics_domain::models::names::ChallengeName;
use anyhow::Result;

use crate::cli::OutputFormat;

use super::format::{pretty_json, status_label};

/// Renders challenge review record for user-facing output.
pub(crate) fn render_challenge_review_record(
    response: &ChallengeReviewRecordResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "challenge_review_record: {}\nchallenge: {}\nrequest: {}\nstatus: {}\nrepo: {}#{}\npath: {}\ncommit: {}\nmanifest_sha256: {}\npublished_challenge: {}\nprivate_assets: {}\nvalidation_records: {}",
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

/// Renders a creator-facing challenge review record.
pub(crate) fn render_creator_challenge_review_record(
    response: &CreatorChallengeReviewRecordResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "challenge_review_record: {}\nchallenge: {}\nrequest: {}\nstatus: {}\nrepo: {}#{}\npath: {}\ncommit: {}\nmanifest_sha256: {}\npublished_challenge: {}\nprivate_assets: {}\nvalidation_records: {}",
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

/// Renders a creator private asset upload response.
pub(crate) fn render_challenge_private_asset(
    response: &ChallengePrivateAssetResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "private_asset: {}\nreview_record: {}\nasset_name: {}\nkind: {}\nrequired: {}\nsize_bytes: {}\nsha256: {}\nstorage_key: {}",
            response.id,
            response.review_record_id,
            response.asset_name,
            status_label(&response.kind),
            response.required,
            response.size_bytes,
            response.sha256,
            response.storage_key
        )),
    }
}

/// Renders challenge review record cleanup for user-facing output.
pub(crate) fn render_challenge_review_record_cleanup(
    response: &ChallengeReviewRecordCleanupResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(format!(
            "abandoned_review_records: {}\npurged_private_assets: {}\npurged_temporary_storage_objects: {}",
            response.abandoned_review_records,
            response.purged_private_assets,
            response.purged_temporary_storage_objects
        )),
    }
}
