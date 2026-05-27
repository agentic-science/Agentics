use std::path::Path;

use tracing::warn;

use agentics_error::Result;
use agentics_storage::{Storage, StorageKey};

/// Deletes a storage object after a failed or repaired workflow path.
pub(super) async fn cleanup_storage_key(storage: &dyn Storage, storage_key: &StorageKey) {
    if let Err(error) = storage.delete(storage_key).await {
        warn!(
            storage_key = %storage_key,
            error = %error,
            "failed to clean up temporary storage object"
        );
    }
}

/// Best-effort cleanup for failed runtime bundle assembly or publish.
pub(super) async fn cleanup_runtime_bundle(path: &Path) {
    if let Err(error) = tokio::fs::remove_dir_all(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            path = %path.display(),
            error = %error,
            "failed to clean up challenge runtime bundle"
        );
    }
}

/// Best-effort cleanup for temporary archive files.
pub(super) async fn cleanup_file(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            path = %path.display(),
            error = %error,
            "failed to clean up temporary storage file"
        );
    }
}

pub(super) fn challenge_bundle_storage_key(
    prefix: &str,
    challenge_name: &str,
    draft_id: &str,
    publish_claim_id: &str,
) -> Result<StorageKey> {
    StorageKey::try_new(format!(
        "{prefix}/{challenge_name}/{draft_id}-{publish_claim_id}.tar"
    ))
    .map_err(Into::into)
}

/// Returns the trimmed message only when it carries non-whitespace content.
pub(super) fn non_empty_message(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Decodes user-provided base64 payloads after trimming transport whitespace.
pub(super) fn base64_decode(input: &str) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.decode(input.trim()).ok()
}
