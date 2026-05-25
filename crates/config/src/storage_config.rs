use std::path::Path;

use serde::Deserialize;

use crate::{Config, validate_required_trimmed};

const DEFAULT_STORAGE_BACKEND: StorageBackend = StorageBackend::Local;
const DEFAULT_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const DEFAULT_STORAGE_MAX_STATEMENT_BYTES: u64 = 1024 * 1024;
const DEFAULT_STORAGE_MAX_JSON_ARTIFACT_BYTES: u64 = 1024 * 1024;

/// Durable storage backend for platform objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    /// Store durable objects under `AGENTICS_STORAGE_ROOT`.
    Local,
    /// Store durable objects in an S3-compatible bucket.
    S3,
}

impl StorageBackend {
    /// Stable environment string for this durable storage backend.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
        }
    }
}

/// Default durable storage backend.
pub(crate) fn default_storage_backend() -> StorageBackend {
    DEFAULT_STORAGE_BACKEND
}

/// Default S3 region used by AWS-compatible local test services.
pub(crate) fn default_s3_region() -> String {
    "us-east-1".to_string()
}

/// Default maximum stored challenge bundle archive bytes.
pub(crate) fn default_storage_max_bundle_archive_bytes() -> u64 {
    DEFAULT_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES
}

/// Default maximum stored statement bytes.
pub(crate) fn default_storage_max_statement_bytes() -> u64 {
    DEFAULT_STORAGE_MAX_STATEMENT_BYTES
}

/// Default maximum stored creator/admin JSON artifact bytes.
pub(crate) fn default_storage_max_json_artifact_bytes() -> u64 {
    DEFAULT_STORAGE_MAX_JSON_ARTIFACT_BYTES
}

/// Validate durable object storage configuration.
pub(crate) fn validate_object_storage_config(config: &Config) -> anyhow::Result<()> {
    if config.storage_max_bundle_archive_bytes == 0 {
        anyhow::bail!("AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES must be greater than zero");
    }
    if config.storage_max_statement_bytes == 0 {
        anyhow::bail!("AGENTICS_STORAGE_MAX_STATEMENT_BYTES must be greater than zero");
    }
    if config.storage_max_json_artifact_bytes == 0 {
        anyhow::bail!("AGENTICS_STORAGE_MAX_JSON_ARTIFACT_BYTES must be greater than zero");
    }
    if let Some(work_root) = config
        .storage_work_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && !Path::new(work_root).is_absolute()
    {
        anyhow::bail!("AGENTICS_STORAGE_WORK_ROOT must be an absolute path");
    }
    match config.storage_backend {
        StorageBackend::Local => Ok(()),
        StorageBackend::S3 => {
            validate_required_trimmed(config.s3_bucket.as_deref(), "AGENTICS_S3_BUCKET")?;
            validate_s3_prefix(config.s3_prefix.as_deref())?;
            validate_required_trimmed(Some(&config.s3_region), "AGENTICS_S3_REGION")?;
            if let Some(endpoint) = config.s3_endpoint_url.as_ref()
                && !matches!(endpoint.scheme(), "http" | "https")
            {
                anyhow::bail!("AGENTICS_S3_ENDPOINT_URL must start with http:// or https://");
            }
            Ok(())
        }
    }
}

/// Validate an optional S3 key prefix.
fn validate_s3_prefix(value: Option<&str>) -> anyhow::Result<()> {
    let Some(prefix) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if prefix.starts_with('/') || prefix.ends_with('/') || prefix.contains('\\') {
        anyhow::bail!(
            "AGENTICS_S3_PREFIX must be a relative slash-separated storage prefix without leading or trailing slash"
        );
    }
    for component in prefix.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.bytes().any(|byte| {
                byte.is_ascii_whitespace()
                    || byte.is_ascii_control()
                    || !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
            })
        {
            anyhow::bail!("AGENTICS_S3_PREFIX contains an unsafe path component");
        }
    }
    Ok(())
}
