use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

const DEFAULT_PRODUCTION_API_BASE_URL: &str = "https://agentics.reify.ing";
const LOCAL_API_HOST: &str = "127.0.0.1";

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Carries cli config data across this module boundary.
pub(crate) struct CliConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_api_token: Option<String>,
}

impl fmt::Debug for CliConfig {
    /// Redacts the persisted bearer token when tests or logs format config values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CliConfig")
            .field("api_base_url", &self.api_base_url)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field(
                "creator_api_token",
                &self.creator_api_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
/// Raw CLI environment values.
struct RawCliEnv {
    api_base_url: Option<String>,
    api_port: Option<u16>,
    token: Option<String>,
    pioneer_code: Option<String>,
    admin_service_token: Option<String>,
    creator_api_token: Option<String>,
    allow_insecure_remote_http: Option<bool>,
}

#[derive(Debug, Clone, Default)]
/// Carries environment data across this module boundary.
pub(crate) struct Environment {
    pub api_base_url: Option<String>,
    pub api_port: Option<u16>,
    pub token: Option<SecretString>,
    pub pioneer_code: Option<SecretString>,
    pub admin_service_token: Option<SecretString>,
    pub creator_api_token: Option<SecretString>,
    pub allow_insecure_remote_http: bool,
}

impl Environment {
    /// Handles from process for this module.
    pub(crate) fn from_process() -> Result<Self> {
        let raw = envy::prefixed("AGENTICS_")
            .from_env::<RawCliEnv>()
            .context("failed to load AGENTICS_* CLI environment")?;
        Ok(Self {
            api_base_url: non_empty_owned(raw.api_base_url),
            api_port: raw.api_port,
            token: non_empty_owned(raw.token).map(SecretString::from),
            pioneer_code: non_empty_owned(raw.pioneer_code).map(SecretString::from),
            admin_service_token: non_empty_owned(raw.admin_service_token).map(SecretString::from),
            creator_api_token: non_empty_owned(raw.creator_api_token).map(SecretString::from),
            allow_insecure_remote_http: raw.allow_insecure_remote_http.unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates setting source variants supported by this module.
pub(crate) enum SettingSource {
    Flag,
    Environment,
    ConfigFile,
    Default,
    Missing,
}

impl fmt::Display for SettingSource {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Flag => "flag",
            Self::Environment => "environment",
            Self::ConfigFile => "config",
            Self::Default => "default",
            Self::Missing => "missing",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone)]
/// Carries resolved settings data across this module boundary.
pub(crate) struct ResolvedSettings {
    pub api_base_url: ApiBaseUrl,
    pub api_base_url_source: SettingSource,
    pub token: Option<SecretString>,
    pub token_source: SettingSource,
    pub pioneer_code: Option<SecretString>,
    pub admin_service_token: Option<SecretString>,
    pub creator_api_token: Option<SecretString>,
    pub creator_api_token_source: SettingSource,
    pub allow_insecure_remote_http: bool,
    pub config_path: PathBuf,
}

impl ResolvedSettings {
    /// Handles resolve for this module.
    pub(crate) fn resolve(
        flag_api_base_url: Option<&str>,
        flag_token: Option<&str>,
        env: &Environment,
        file: &CliConfig,
        config_path: PathBuf,
    ) -> Result<Self> {
        let fallback_api_base_url = default_api_base_url_from_env(env.api_port);
        let (api_base_url, api_base_url_source) = first_value_with_default(
            flag_api_base_url,
            env.api_base_url.as_deref(),
            file.api_base_url.as_deref(),
            fallback_api_base_url.as_str(),
        );
        let (token, token_source) = first_optional_value(
            flag_token,
            env.token.as_ref().map(ExposeSecret::expose_secret),
            file.token.as_deref(),
        );
        let (creator_api_token, creator_api_token_source) = first_optional_value(
            None,
            env.creator_api_token
                .as_ref()
                .map(ExposeSecret::expose_secret),
            file.creator_api_token.as_deref(),
        );

        Ok(Self {
            api_base_url: ApiBaseUrl::try_new_with_policy(
                api_base_url,
                env.allow_insecure_remote_http,
            )?,
            api_base_url_source,
            token: token.map(|value| SecretString::from(value.to_string())),
            token_source,
            pioneer_code: env.pioneer_code.clone(),
            admin_service_token: env.admin_service_token.clone(),
            creator_api_token: creator_api_token.map(|value| SecretString::from(value.to_string())),
            creator_api_token_source,
            allow_insecure_remote_http: env.allow_insecure_remote_http,
            config_path,
        })
    }

    /// Handles token configured for this module.
    pub(crate) fn token_configured(&self) -> bool {
        self.token.is_some()
    }

    /// Returns whether a creator API token is available for creator commands.
    pub(crate) fn creator_api_token_configured(&self) -> bool {
        self.creator_api_token.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Carries api base url data across this module boundary.
pub(crate) struct ApiBaseUrl(Url);

impl ApiBaseUrl {
    /// Handles try new for this module.
    #[cfg(test)]
    pub(crate) fn try_new(value: &str) -> Result<Self> {
        Self::try_new_with_policy(value, false)
    }

    /// Parse and validate an API base URL with an explicit insecure-HTTP override.
    pub(crate) fn try_new_with_policy(
        value: &str,
        allow_insecure_remote_http: bool,
    ) -> Result<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            bail!("API base URL must not be empty");
        }

        let mut url =
            Url::parse(trimmed).with_context(|| format!("invalid API base URL `{trimmed}`"))?;
        match url.scheme() {
            "http" | "https" => {}
            scheme => bail!("API base URL must use http or https, got `{scheme}`"),
        }
        if url.scheme() == "http" && !allow_insecure_remote_http && !is_loopback_or_localhost(&url)
        {
            bail!(
                "HTTP API base URLs are allowed only for localhost/loopback; use HTTPS or set AGENTICS_ALLOW_INSECURE_REMOTE_HTTP=true"
            );
        }
        if url.query().is_some() || url.fragment().is_some() {
            bail!("API base URL must not include a query string or fragment");
        }
        if !url.path().ends_with('/') {
            let mut path = url.path().to_string();
            path.push('/');
            url.set_path(&path);
        }

        Ok(Self(url))
    }

    /// Returns url in the representation required by callers.
    pub(crate) fn as_url(&self) -> &Url {
        &self.0
    }
}

fn is_loopback_or_localhost(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|addr| addr.is_loopback())
}

impl fmt::Display for ApiBaseUrl {
    /// Handles fmt for this module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.0.as_str();
        f.write_str(value.strip_suffix('/').unwrap_or(value))
    }
}

/// Build the API URL used when CLI configuration is absent.
fn default_api_base_url_from_env(env_api_port: Option<u16>) -> String {
    match env_api_port {
        Some(port) => format!("http://{LOCAL_API_HOST}:{port}"),
        None => DEFAULT_PRODUCTION_API_BASE_URL.to_string(),
    }
}

#[derive(Debug, Clone)]
/// Carries config store data across this module boundary.
pub(crate) struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// Handles new for this module.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Return the standard per-user CLI config path.
    pub(crate) fn standard_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow!("could not determine a user config directory"))?;
        Ok(config_dir.join("agentics").join("config.toml"))
    }

    /// Handles path for this module.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Handles load for this module.
    pub(crate) fn load(&self) -> Result<CliConfig> {
        if !fs::exists(&self.path)
            .with_context(|| format!("failed to inspect config file {}", self.path.display()))?
        {
            return Ok(CliConfig::default());
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read config file {}", self.path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file {}", self.path.display()))
    }

    /// Handles save for this module.
    pub(crate) fn save(&self, config: &CliConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let raw = toml::to_string_pretty(config).context("failed to serialize CLI config")?;
        write_private_file(&self.path, raw.as_bytes())
            .with_context(|| format!("failed to write config file {}", self.path.display()))
    }
}

fn non_empty_owned(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Handles first value with default for this module.
fn first_value_with_default<'a>(
    flag: Option<&'a str>,
    env: Option<&'a str>,
    file: Option<&'a str>,
    default: &'a str,
) -> (&'a str, SettingSource) {
    if let Some(value) = non_empty(flag) {
        return (value, SettingSource::Flag);
    }
    if let Some(value) = non_empty(env) {
        return (value, SettingSource::Environment);
    }
    if let Some(value) = non_empty(file) {
        return (value, SettingSource::ConfigFile);
    }
    (default, SettingSource::Default)
}

/// Handles first optional value for this module.
fn first_optional_value<'a>(
    flag: Option<&'a str>,
    env: Option<&'a str>,
    file: Option<&'a str>,
) -> (Option<&'a str>, SettingSource) {
    if let Some(value) = non_empty(flag) {
        return (Some(value), SettingSource::Flag);
    }
    if let Some(value) = non_empty(env) {
        return (Some(value), SettingSource::Environment);
    }
    if let Some(value) = non_empty(file) {
        return (Some(value), SettingSource::ConfigFile);
    }
    (None, SettingSource::Missing)
}

/// Handles non empty for this module.
fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Writes private file to the target path.
fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = private_temp_path(path)?;
    let write_result = write_private_temp_file(&temp_path, bytes)
        .and_then(|()| fs::rename(&temp_path, path))
        .and_then(|()| set_private_file_permissions(path));

    if write_result.is_err() {
        drop(fs::remove_file(&temp_path));
    }

    write_result
}

/// Handles private temp path for this module.
fn private_temp_path(path: &Path) -> io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "config path has no file name")
        })?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    Ok(parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        unique
    )))
}

/// Writes private temp file to the target path.
fn write_private_temp_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);

    cfg_select! {
        unix => {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        _ => {}
    }

    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Sets private file permissions after applying domain validation.
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    cfg_select! {
        unix => {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        }
        _ => {
            let _ = path;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that resolves config precedence.
    #[test]
    fn resolves_config_precedence() {
        let file = CliConfig {
            api_base_url: Some("https://file.example".to_string()),
            token: Some("file-token".to_string()),
            ..CliConfig::default()
        };
        let env = Environment {
            api_base_url: Some("https://env.example".to_string()),
            api_port: None,
            token: Some(SecretString::from("env-token")),
            pioneer_code: None,
            admin_service_token: None,
            creator_api_token: None,
            allow_insecure_remote_http: false,
        };

        let settings = ResolvedSettings::resolve(
            Some("https://flag.example"),
            None,
            &env,
            &file,
            PathBuf::from("config.toml"),
        )
        .expect("settings should resolve");

        assert_eq!(settings.api_base_url.to_string(), "https://flag.example");
        assert_eq!(settings.api_base_url_source, SettingSource::Flag);
        assert_eq!(
            settings.token.as_ref().map(ExposeSecret::expose_secret),
            Some("env-token")
        );
        assert_eq!(settings.token_source, SettingSource::Environment);
    }

    /// Verifies the CLI defaults to the production API when no override is supplied.
    #[test]
    fn defaults_to_production_api_base_url() {
        let settings = ResolvedSettings::resolve(
            None,
            None,
            &Environment::default(),
            &CliConfig::default(),
            PathBuf::from("config.toml"),
        )
        .expect("settings should resolve");

        assert_eq!(
            settings.api_base_url.to_string(),
            DEFAULT_PRODUCTION_API_BASE_URL
        );
        assert_eq!(settings.api_base_url_source, SettingSource::Default);
    }

    /// Verifies an explicit local API port still requests a loopback default.
    #[test]
    fn api_port_env_selects_local_api_base_url() {
        let env = Environment {
            api_port: Some(3210),
            ..Environment::default()
        };
        let settings = ResolvedSettings::resolve(
            None,
            None,
            &env,
            &CliConfig::default(),
            PathBuf::from("config.toml"),
        )
        .expect("settings should resolve");

        assert_eq!(settings.api_base_url.to_string(), "http://127.0.0.1:3210");
        assert_eq!(settings.api_base_url_source, SettingSource::Default);
    }

    /// Verifies that saves and loads config without null fields.
    #[test]
    fn saves_and_loads_config_without_null_fields() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("agentics.toml");
        let store = ConfigStore::new(path.clone());

        store
            .save(&CliConfig {
                api_base_url: Some("http://127.0.0.1:3100".to_string()),
                token: None,
                ..CliConfig::default()
            })
            .expect("config should save");

        let raw = fs::read_to_string(path).expect("config should be readable");
        assert!(!raw.contains("token"));
        assert_eq!(
            store.load().expect("config should load"),
            CliConfig {
                api_base_url: Some("http://127.0.0.1:3100".to_string()),
                token: None,
                ..CliConfig::default()
            }
        );
    }

    /// Verifies that debug output does not expose persisted CLI bearer tokens.
    #[test]
    fn debug_redacts_saved_agent_token() {
        let config = CliConfig {
            api_base_url: Some("http://127.0.0.1:3100".to_string()),
            token: Some("secret-agent-token".to_string()),
            creator_api_token: Some("secret-creator-token".to_string()),
        };

        let debug = format!("{config:?}");

        assert!(!debug.contains("secret-agent-token"));
        assert!(debug.contains("<redacted>"));
    }

    #[cfg(unix)]
    /// Verifies that save restricts existing config file permissions.
    #[test]
    fn save_restricts_existing_config_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("agentics.toml");
        std::fs::write(&path, "token = \"old\"\n").expect("seed config");
        std::fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
            .expect("loose permissions should be settable");

        let store = ConfigStore::new(path.clone());
        store
            .save(&CliConfig {
                api_base_url: None,
                token: Some("secret-token".to_string()),
                ..CliConfig::default()
            })
            .expect("config should save");

        let mode = std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    /// Verifies that rejects invalid api base url.
    #[test]
    fn rejects_invalid_api_base_url() {
        let error = ApiBaseUrl::try_new("file:///tmp/api").expect_err("must reject file URL");
        assert!(error.to_string().contains("http or https"));
    }

    /// Verifies remote HTTP requires an explicit unsafe override while loopback remains usable.
    #[test]
    fn rejects_remote_http_without_insecure_override() {
        let error = ApiBaseUrl::try_new("http://agentics.example")
            .expect_err("remote HTTP should require explicit opt-in");
        assert!(error.to_string().contains("localhost/loopback"));
        ApiBaseUrl::try_new("http://127.0.0.1:3100").expect("loopback HTTP should be allowed");
        ApiBaseUrl::try_new_with_policy("http://agentics.example", true)
            .expect("explicit insecure override should allow remote HTTP");
    }

    /// Verifies malformed API port env fails instead of falling back.
    #[test]
    fn rejects_invalid_env_api_port_for_default_url() {
        let error = envy::prefixed("AGENTICS_")
            .from_iter::<_, RawCliEnv>([(
                "AGENTICS_API_PORT".to_string(),
                "not-a-port".to_string(),
            )])
            .expect_err("invalid env port should fail");
        assert!(error.to_string().contains("invalid digit"));
    }
}
