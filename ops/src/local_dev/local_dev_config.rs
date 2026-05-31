use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agentics_config::StorageBackend;
use serde::Deserialize;
use url::Url;

use super::{
    ENV_DATABASE_URL, ENV_LOCAL_DEV_DATABASE_NAME, ENV_LOCAL_DEV_DATABASE_URL,
    ENV_LOCAL_DEV_DATABASE_URL_CONFIRM, LocalDevError, NON_LOOPBACK_DATABASE_CONFIRMATION,
};

const ENV_PREFIX: &str = "AGENTICS_";

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct RawLocalDevEnv {
    pub(super) local_dev_env_file: Option<String>,
    pub(super) local_dev_database_name: Option<String>,
    pub(super) local_dev_database_url: Option<String>,
    pub(super) local_dev_database_url_confirm: Option<String>,
    pub(super) local_dev_challenge_source_root: Option<String>,
    pub(super) local_dev_test_solutions_root: Option<String>,
    pub(super) api_base_url: Option<String>,
    pub(super) storage_root: Option<String>,
    pub(super) storage_backend: Option<StorageBackend>,
    pub(super) storage_work_root: Option<String>,
    pub(super) s3_bucket: Option<String>,
    pub(super) s3_prefix: Option<String>,
    pub(super) s3_region: Option<String>,
    pub(super) s3_endpoint_url: Option<String>,
    pub(super) s3_force_path_style: Option<bool>,
}

impl RawLocalDevEnv {
    pub(super) fn from_process() -> Result<Self, LocalDevError> {
        envy::prefixed(ENV_PREFIX)
            .from_env::<Self>()
            .map_err(|error| LocalDevError::InvalidConfig(error.to_string()))
    }

    pub(super) fn from_map(values: &HashMap<String, String>) -> Result<Self, LocalDevError> {
        envy::prefixed(ENV_PREFIX)
            .from_iter(values.clone())
            .map_err(|error| LocalDevError::InvalidConfig(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::RawLocalDevEnv;

    /// Verifies local dev bool envs use normal env deserialization.
    #[test]
    fn bool_env_values_use_generic_deserialization() {
        let values = [(
            "AGENTICS_S3_FORCE_PATH_STYLE".to_string(),
            "false".to_string(),
        )]
        .into_iter()
        .collect();
        let env = RawLocalDevEnv::from_map(&values).expect("bool literal should parse");
        assert_eq!(env.s3_force_path_style, Some(false));

        let values = [("AGENTICS_S3_FORCE_PATH_STYLE".to_string(), "1".to_string())]
            .into_iter()
            .collect();
        assert!(RawLocalDevEnv::from_map(&values).is_err());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalDevDatabaseName(String);

impl LocalDevDatabaseName {
    pub(super) fn parse(value: &str) -> Result<Self, LocalDevError> {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err(LocalDevError::InvalidConfig(format!(
                "{ENV_LOCAL_DEV_DATABASE_NAME} must contain only letters, digits, and underscores"
            )));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn load_dotenv_file(path: &Path) -> Result<HashMap<String, String>, LocalDevError> {
    if !path.exists() {
        return Err(LocalDevError::InvalidConfig(format!(
            "missing env file {}",
            path.display()
        )));
    }
    let mut values = HashMap::new();
    for item in dotenvy::from_path_iter(path)? {
        let (key, value) = item?;
        values.insert(key, value);
    }
    Ok(values)
}

pub(super) fn env_value(
    process_value: Option<&String>,
    file_value: Option<&String>,
) -> Option<String> {
    non_empty_value(process_value).or_else(|| non_empty_value(file_value))
}

pub(super) fn non_empty_value(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn parse_url(name: &str, value: &str) -> Result<Url, LocalDevError> {
    Url::parse(value)
        .map_err(|error| LocalDevError::InvalidConfig(format!("invalid {name}: {error}")))
}

pub(super) fn resolve_local_dev_database_url(
    process_value: Option<String>,
    file_value: Option<String>,
) -> Result<String, LocalDevError> {
    process_value.or(file_value).ok_or_else(|| {
        LocalDevError::InvalidConfig(format!(
            "{ENV_LOCAL_DEV_DATABASE_URL} must be set; local dev refuses to use {ENV_DATABASE_URL} or generate an implicit database URL"
        ))
    })
}

pub(super) fn validate_local_dev_database_url(
    raw: &str,
    database_name: &LocalDevDatabaseName,
    confirmation: Option<&str>,
) -> Result<Url, LocalDevError> {
    let url = parse_url(ENV_LOCAL_DEV_DATABASE_URL, raw)?;
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return Err(LocalDevError::InvalidConfig(format!(
            "{ENV_LOCAL_DEV_DATABASE_URL} must use postgres or postgresql scheme"
        )));
    }
    let host = url.host_str().ok_or_else(|| {
        LocalDevError::InvalidConfig(format!("{ENV_LOCAL_DEV_DATABASE_URL} must include a host"))
    })?;
    if !host_is_loopback(host) && confirmation != Some(NON_LOOPBACK_DATABASE_CONFIRMATION) {
        return Err(LocalDevError::InvalidConfig(format!(
            "refusing non-loopback {ENV_LOCAL_DEV_DATABASE_URL} host {host:?} without {ENV_LOCAL_DEV_DATABASE_URL_CONFIRM}={NON_LOOPBACK_DATABASE_CONFIRMATION}"
        )));
    }
    let path_database = url.path().trim_start_matches('/');
    if path_database != database_name.as_str() {
        return Err(LocalDevError::InvalidConfig(format!(
            "{ENV_LOCAL_DEV_DATABASE_URL} database path must be /{}; got {:?}",
            database_name.as_str(),
            url.path()
        )));
    }
    Ok(url)
}

pub(super) fn host_is_loopback(host: &str) -> bool {
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

pub(super) fn repo_root() -> Result<PathBuf, LocalDevError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| LocalDevError::InvalidConfig("cannot determine repo root".to_string()))
}
