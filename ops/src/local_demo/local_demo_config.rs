use std::collections::HashMap;
use std::path::{Path, PathBuf};

use url::Url;

use super::{
    ENV_DATABASE_URL, ENV_DEMO_DATABASE_NAME, ENV_DEMO_DATABASE_URL, ENV_DEMO_DATABASE_URL_CONFIRM,
    LocalDemoError, NON_LOOPBACK_DATABASE_CONFIRMATION,
};
use crate::support::env_non_empty;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DemoDatabaseName(String);

impl DemoDatabaseName {
    pub(super) fn parse(value: &str) -> Result<Self, LocalDemoError> {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err(LocalDemoError::InvalidConfig(format!(
                "{ENV_DEMO_DATABASE_NAME} must contain only letters, digits, and underscores"
            )));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn load_dotenv_file(path: &Path) -> Result<HashMap<String, String>, LocalDemoError> {
    if !path.exists() {
        return Err(LocalDemoError::InvalidConfig(format!(
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

pub(super) fn env_value(name: &str, file_env: &HashMap<String, String>) -> Option<String> {
    env_non_empty(name).or_else(|| file_env_non_empty(name, file_env))
}

pub(super) fn file_env_non_empty(name: &str, file_env: &HashMap<String, String>) -> Option<String> {
    file_env
        .get(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn parse_url(name: &str, value: &str) -> Result<Url, LocalDemoError> {
    Url::parse(value)
        .map_err(|error| LocalDemoError::InvalidConfig(format!("invalid {name}: {error}")))
}

pub(super) fn resolve_demo_database_url(
    process_value: Option<String>,
    file_value: Option<String>,
) -> Result<String, LocalDemoError> {
    process_value.or(file_value).ok_or_else(|| {
        LocalDemoError::InvalidConfig(format!(
            "{ENV_DEMO_DATABASE_URL} must be set; local demo refuses to use {ENV_DATABASE_URL} or generate an implicit database URL"
        ))
    })
}

pub(super) fn validate_demo_database_url(
    raw: &str,
    database_name: &DemoDatabaseName,
    confirmation: Option<&str>,
) -> Result<Url, LocalDemoError> {
    let url = parse_url(ENV_DEMO_DATABASE_URL, raw)?;
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return Err(LocalDemoError::InvalidConfig(format!(
            "{ENV_DEMO_DATABASE_URL} must use postgres or postgresql scheme"
        )));
    }
    let host = url.host_str().ok_or_else(|| {
        LocalDemoError::InvalidConfig(format!("{ENV_DEMO_DATABASE_URL} must include a host"))
    })?;
    if !host_is_loopback(host) && confirmation != Some(NON_LOOPBACK_DATABASE_CONFIRMATION) {
        return Err(LocalDemoError::InvalidConfig(format!(
            "refusing non-loopback {ENV_DEMO_DATABASE_URL} host {host:?} without {ENV_DEMO_DATABASE_URL_CONFIRM}={NON_LOOPBACK_DATABASE_CONFIRMATION}"
        )));
    }
    let path_database = url.path().trim_start_matches('/');
    if path_database != database_name.as_str() {
        return Err(LocalDemoError::InvalidConfig(format!(
            "{ENV_DEMO_DATABASE_URL} database path must be /{}; got {:?}",
            database_name.as_str(),
            url.path()
        )));
    }
    Ok(url)
}

pub(super) fn host_is_loopback(host: &str) -> bool {
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

pub(super) fn repo_root() -> Result<PathBuf, LocalDemoError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| LocalDemoError::InvalidConfig("cannot determine repo root".to_string()))
}
