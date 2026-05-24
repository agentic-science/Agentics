use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use agentics_config::ENV_AGENTICS_ADMIN_PASSWORD;
use secrecy::SecretString;
use url::Url;
use uuid::Uuid;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdminPasswordSource {
    ExistingFile,
    Environment,
    Generated,
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

pub(super) fn parse_port(
    name: &str,
    value: Option<&str>,
    default: u16,
) -> Result<u16, LocalDemoError> {
    match value {
        Some(value) => value.parse::<u16>().map_err(|error| {
            LocalDemoError::InvalidConfig(format!("invalid {name} value {value:?}: {error}"))
        }),
        None => Ok(default),
    }
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

pub(super) fn resolve_admin_password(
    path: &Path,
) -> Result<(SecretString, AdminPasswordSource), LocalDemoError> {
    if path.exists() {
        reject_symlink(path, "admin password file")?;
        let value = std::fs::read_to_string(path)?.trim().to_string();
        if value.is_empty() {
            return Err(LocalDemoError::InvalidConfig(format!(
                "{} is empty",
                path.display()
            )));
        }
        return Ok((SecretString::from(value), AdminPasswordSource::ExistingFile));
    }
    if let Some(value) = env_non_empty(ENV_AGENTICS_ADMIN_PASSWORD)
        .filter(|value| !matches!(value.as_str(), "agentics-admin" | "change-me"))
    {
        return Ok((SecretString::from(value), AdminPasswordSource::Environment));
    }
    Ok((
        SecretString::from(generate_demo_admin_password()),
        AdminPasswordSource::Generated,
    ))
}

pub(super) fn create_secret_file(path: &Path, secret: &str) -> Result<(), LocalDemoError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(secret.as_bytes())?;
    secure_admin_password_file(path)?;
    Ok(())
}

pub(super) fn secure_admin_password_file(path: &Path) -> Result<(), LocalDemoError> {
    reject_symlink(path, "admin password file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

pub(super) fn reject_symlink(path: &Path, label: &str) -> Result<(), LocalDemoError> {
    if std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(LocalDemoError::InvalidConfig(format!(
            "refusing symlink {label} {}",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn detect_lan_host() -> Option<String> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    let addr = socket.local_addr().ok()?;
    let host = addr.ip().to_string();
    (!host_is_loopback(&host)).then_some(host)
}

pub(super) fn demo_cors_allowed_origins(
    web_port: u16,
    public_host: Option<&str>,
    web_host: &str,
) -> String {
    let mut origins = vec![
        format!("http://127.0.0.1:{web_port}"),
        format!("http://localhost:{web_port}"),
    ];
    if !host_is_loopback(web_host)
        && let Some(host) = public_host
        && !host_is_loopback(host)
    {
        origins.push(format!("http://{host}:{web_port}"));
    }
    origins.join(",")
}

pub(super) fn demo_allowed_dev_origins(public_host: Option<&str>, web_host: &str) -> String {
    let mut origins = vec!["127.0.0.1".to_string(), "localhost".to_string()];
    if !host_is_loopback(web_host)
        && let Some(host) = public_host
        && !host_is_loopback(host)
    {
        origins.push(host.to_string());
    }
    origins.join(",")
}

pub(super) fn host_is_loopback(host: &str) -> bool {
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

pub(super) fn generate_demo_admin_password() -> String {
    format!(
        "local-demo-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub(super) fn repo_root() -> Result<PathBuf, LocalDemoError> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| LocalDemoError::InvalidConfig("cannot determine repo root".to_string()))
}

pub(super) fn require_tool(tool: &str) -> Result<(), LocalDemoError> {
    let Some(path) = std::env::var_os("PATH") else {
        return Err(LocalDemoError::MissingTool(tool.to_string()));
    };
    if std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()) {
        Ok(())
    } else {
        Err(LocalDemoError::MissingTool(tool.to_string()))
    }
}
