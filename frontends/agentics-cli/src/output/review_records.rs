use std::fmt::Display;

use agentics_domain::models::challenge_creation::{
    ChallengeCreationRequestKind, ChallengePrivateAssetResponse,
    ChallengeReviewRecordCleanupResponse, ChallengeReviewRecordResponse,
    ChallengeReviewRecordStatus, ChallengeReviewValidationRecordResponse,
    CreatorChallengeReviewRecordResponse, CreatorChallengeReviewValidationRecordResponse,
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
        OutputFormat::Table => Ok(render_admin_review_record_table(response)),
    }
}

/// Renders a creator-facing challenge review record.
pub(crate) fn render_creator_challenge_review_record(
    response: &CreatorChallengeReviewRecordResponse,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Json => pretty_json(response),
        OutputFormat::Table => Ok(render_creator_review_record_table(response)),
    }
}

fn render_admin_review_record_table(response: &ChallengeReviewRecordResponse) -> String {
    let mut output = render_common_review_record_table(ReviewRecordTableFields {
        id: response.id.to_string(),
        challenge_name: response.challenge_name.to_string(),
        request: &response.request,
        status: &response.status,
        repo_url: response.repo_url.to_string(),
        pr_number: response.pr_number.to_string(),
        challenge_path: response.challenge_path.to_string(),
        commit_sha: response.commit_sha.to_string(),
        manifest_sha256: response.manifest_sha256.to_string(),
        published_challenge_name: response.published_challenge_name.as_ref(),
        private_asset_count: response.private_assets.len(),
        validation_record_count: response.validation_records.len(),
    });
    append_optional_line(
        &mut output,
        "validation_bundle_sha256",
        response.validation_bundle_sha256.as_ref(),
    );
    append_optional_line(
        &mut output,
        "approved_bundle_sha256",
        response.approved_bundle_sha256.as_ref(),
    );
    append_optional_str_line(
        &mut output,
        "validation_message",
        response.validation_message.as_deref(),
    );
    if let Some(record) = response.validation_records.last() {
        append_admin_validation_record(&mut output, record);
    }
    output
}

fn render_creator_review_record_table(response: &CreatorChallengeReviewRecordResponse) -> String {
    let mut output = render_common_review_record_table(ReviewRecordTableFields {
        id: response.id.to_string(),
        challenge_name: response.challenge_name.to_string(),
        request: &response.request,
        status: &response.status,
        repo_url: response.repo_url.to_string(),
        pr_number: response.pr_number.to_string(),
        challenge_path: response.challenge_path.to_string(),
        commit_sha: response.commit_sha.to_string(),
        manifest_sha256: response.manifest_sha256.to_string(),
        published_challenge_name: response.published_challenge_name.as_ref(),
        private_asset_count: response.private_assets.len(),
        validation_record_count: response.validation_records.len(),
    });
    append_optional_line(
        &mut output,
        "validation_bundle_sha256",
        response.validation_bundle_sha256.as_ref(),
    );
    append_optional_line(
        &mut output,
        "approved_bundle_sha256",
        response.approved_bundle_sha256.as_ref(),
    );
    append_optional_str_line(
        &mut output,
        "validation_message",
        response.validation_message.as_deref(),
    );
    if let Some(record) = response.validation_records.last() {
        append_creator_validation_record(&mut output, record);
    }
    output
}

struct ReviewRecordTableFields<'a> {
    id: String,
    challenge_name: String,
    request: &'a ChallengeCreationRequestKind,
    status: &'a ChallengeReviewRecordStatus,
    repo_url: String,
    pr_number: String,
    challenge_path: String,
    commit_sha: String,
    manifest_sha256: String,
    published_challenge_name: Option<&'a ChallengeName>,
    private_asset_count: usize,
    validation_record_count: usize,
}

fn render_common_review_record_table(fields: ReviewRecordTableFields<'_>) -> String {
    format!(
        "challenge_review_record: {}\nchallenge: {}\nrequest: {}\nstatus: {}\nrepo: {}#{}\npath: {}\ncommit: {}\nmanifest_sha256: {}\npublished_challenge: {}\nprivate_assets: {}\nvalidation_records: {}",
        fields.id,
        fields.challenge_name,
        status_label(fields.request),
        status_label(fields.status),
        fields.repo_url,
        fields.pr_number,
        fields.challenge_path,
        fields.commit_sha,
        fields.manifest_sha256,
        fields
            .published_challenge_name
            .map_or("none", ChallengeName::as_str),
        fields.private_asset_count,
        fields.validation_record_count,
    )
}

fn append_admin_validation_record(
    output: &mut String,
    record: &ChallengeReviewValidationRecordResponse,
) {
    output.push_str(&format!(
        "\nlatest_validation_status: {}",
        status_label(&record.status)
    ));
    append_optional_line(
        output,
        "latest_validation_bundle_sha256",
        record.bundle_sha256.as_ref(),
    );
    append_optional_str_line(output, "latest_validation_message", Some(&record.message));
}

fn append_creator_validation_record(
    output: &mut String,
    record: &CreatorChallengeReviewValidationRecordResponse,
) {
    output.push_str(&format!(
        "\nlatest_validation_status: {}",
        status_label(&record.status)
    ));
    append_optional_line(
        output,
        "latest_validation_bundle_sha256",
        record.bundle_sha256.as_ref(),
    );
    append_optional_str_line(output, "latest_validation_message", Some(&record.message));
}

fn append_optional_line<T: Display>(output: &mut String, key: &str, value: Option<&T>) {
    if let Some(value) = value {
        output.push_str(&format!("\n{key}: {value}"));
    }
}

fn append_optional_str_line(output: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        output.push_str(&format!("\n{key}: {value}"));
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
