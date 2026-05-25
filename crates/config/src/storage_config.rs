use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;

use crate::{Config, validate_required_trimmed};

/// Environment variable that selects durable object storage backend.
pub const ENV_AGENTICS_STORAGE_BACKEND: &str = "AGENTICS_STORAGE_BACKEND";
/// Environment variable that configures local filesystem durable object storage.
pub const ENV_AGENTICS_STORAGE_ROOT: &str = "AGENTICS_STORAGE_ROOT";
/// Environment variable that configures local storage staging and downloads.
pub const ENV_AGENTICS_STORAGE_WORK_ROOT: &str = "AGENTICS_STORAGE_WORK_ROOT";
/// Environment variable that configures the S3-compatible bucket name.
pub const ENV_AGENTICS_S3_BUCKET: &str = "AGENTICS_S3_BUCKET";
/// Environment variable that configures the S3-compatible object key prefix.
pub const ENV_AGENTICS_S3_PREFIX: &str = "AGENTICS_S3_PREFIX";
/// Environment variable that configures the S3-compatible region.
pub const ENV_AGENTICS_S3_REGION: &str = "AGENTICS_S3_REGION";
/// Environment variable that configures the S3-compatible endpoint URL.
pub const ENV_AGENTICS_S3_ENDPOINT_URL: &str = "AGENTICS_S3_ENDPOINT_URL";
/// Environment variable that enables S3 path-style access.
pub const ENV_AGENTICS_S3_FORCE_PATH_STYLE: &str = "AGENTICS_S3_FORCE_PATH_STYLE";

/// Default durable object storage backend.
pub const DEFAULT_STORAGE_BACKEND: StorageBackend = StorageBackend::S3;
/// Default local filesystem durable object storage root for explicit local mode.
pub const DEFAULT_STORAGE_ROOT: &str = "storage";
/// Default S3-compatible bucket for local, test, and single-host deployments.
pub const DEFAULT_S3_BUCKET: &str = "agentics";
/// Default S3-compatible region for RustFS and local-compatible services.
pub const DEFAULT_S3_REGION: &str = "us-east-1";
/// Default local RustFS endpoint for non-Compose S3-backed development.
pub const DEFAULT_S3_ENDPOINT_URL: &str = "http://127.0.0.1:9000";
/// Default S3 path-style setting for RustFS-compatible object storage.
pub const DEFAULT_S3_FORCE_PATH_STYLE: bool = true;

const DEFAULT_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const DEFAULT_STORAGE_MAX_STATEMENT_BYTES: u64 = 1024 * 1024;
const DEFAULT_STORAGE_MAX_JSON_ARTIFACT_BYTES: u64 = 1024 * 1024;
const DEFAULT_STORAGE_TMP_OBJECT_GRACE_HOURS: u64 = 24;

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

impl FromStr for StorageBackend {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            other => anyhow::bail!(
                "{ENV_AGENTICS_STORAGE_BACKEND} must be either `local` or `s3`; got `{other}`"
            ),
        }
    }
}

/// Default durable storage backend.
pub(crate) fn default_storage_backend() -> StorageBackend {
    DEFAULT_STORAGE_BACKEND
}

/// Default local filesystem durable object storage root for explicit local mode.
pub(crate) fn default_storage_root() -> String {
    DEFAULT_STORAGE_ROOT.to_string()
}

/// Default S3-compatible bucket.
pub(crate) fn default_s3_bucket() -> Option<String> {
    Some(DEFAULT_S3_BUCKET.to_string())
}

/// Default S3 region used by AWS-compatible local test services.
pub(crate) fn default_s3_region() -> String {
    DEFAULT_S3_REGION.to_string()
}

#[allow(
    clippy::expect_used,
    reason = "hard-coded default S3 endpoint is validated at compile-time by tests and has no runtime fallback"
)]
/// Default local RustFS endpoint for non-Compose S3-backed development.
pub(crate) fn default_s3_endpoint_url() -> Option<url::Url> {
    Some(
        DEFAULT_S3_ENDPOINT_URL
            .parse()
            .expect("default S3 endpoint URL must be valid"),
    )
}

/// Default S3 path-style access setting.
pub(crate) fn default_s3_force_path_style() -> bool {
    DEFAULT_S3_FORCE_PATH_STYLE
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

/// Default age after which temporary storage objects are eligible for cleanup.
pub(crate) fn default_storage_tmp_object_grace_hours() -> u64 {
    DEFAULT_STORAGE_TMP_OBJECT_GRACE_HOURS
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
    if config.storage_tmp_object_grace_hours == 0 {
        anyhow::bail!("AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS must be greater than zero");
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
