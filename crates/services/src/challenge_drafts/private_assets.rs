//! Private asset ZIP validation and extraction helpers.

use std::path::Path;

use agentics_contracts::validation::archive::{
    ArchiveEnvelopePolicy, extract_zip_bytes_to_dir, inspect_zip_bytes,
};
use agentics_domain::error::{Result, ServiceError};

const MAX_PRIVATE_ASSET_FILE_COUNT: usize = 1024;

/// Validate a private asset ZIP before the bytes become durable storage state.
pub(super) async fn validate_private_asset_zip_upload(
    bytes: &[u8],
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        validate_private_asset_zip_upload_blocking(&bytes, &asset_name, max_uncompressed_bytes)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("private asset validation task failed: {e}")))?
}

/// Inspect a private asset ZIP for envelope safety without extracting it.
fn validate_private_asset_zip_upload_blocking(
    bytes: &[u8],
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let policy = ArchiveEnvelopePolicy::new(
        format!("private asset `{asset_name}`"),
        max_uncompressed_bytes,
        MAX_PRIVATE_ASSET_FILE_COUNT,
        max_uncompressed_bytes,
    );
    inspect_zip_bytes(bytes, &policy)?;
    Ok(())
}

/// Extracts one private asset ZIP overlay on a blocking worker thread.
pub(super) async fn extract_private_asset_overlay(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    let asset_name = asset_name.to_string();
    tokio::task::spawn_blocking(move || {
        extract_private_asset_overlay_blocking(
            &bytes,
            &target_dir,
            &asset_name,
            max_uncompressed_bytes,
        )
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("private asset extraction task failed: {e}")))?
}

/// Expands a private asset ZIP while enforcing containment, size, and no-overwrite rules.
pub(super) fn extract_private_asset_overlay_blocking(
    bytes: &[u8],
    target_dir: &Path,
    asset_name: &str,
    max_uncompressed_bytes: u64,
) -> Result<()> {
    let policy = ArchiveEnvelopePolicy::new(
        format!("private asset `{asset_name}`"),
        max_uncompressed_bytes,
        MAX_PRIVATE_ASSET_FILE_COUNT,
        max_uncompressed_bytes,
    );
    extract_zip_bytes_to_dir(bytes, target_dir, &policy)
}
