use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:3000";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    pub api_base_url: Option<String>,
    pub token: Option<String>,
}

impl Environment {
    pub fn from_process() -> Self {
        Self {
            api_base_url: read_non_empty_env("AGENTICS_API_BASE_URL"),
            token: read_non_empty_env("AGENTICS_TOKEN"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingSource {
    Flag,
    Environment,
    ConfigFile,
    Default,
    Missing,
}

impl fmt::Display for SettingSource {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSettings {
    pub api_base_url: String,
    pub api_base_url_source: SettingSource,
    pub token: Option<String>,
    pub token_source: SettingSource,
    pub config_path: PathBuf,
}

impl ResolvedSettings {
    pub fn resolve(
        flag_api_base_url: Option<&str>,
        flag_token: Option<&str>,
        env: &Environment,
        file: &CliConfig,
        config_path: PathBuf,
    ) -> Result<Self> {
        let (api_base_url, api_base_url_source) = first_value_with_default(
            flag_api_base_url,
            env.api_base_url.as_deref(),
            file.api_base_url.as_deref(),
            DEFAULT_API_BASE_URL,
        );
        let (token, token_source) =
            first_optional_value(flag_token, env.token.as_deref(), file.token.as_deref());

        Ok(Self {
            api_base_url: normalize_api_base_url(api_base_url)?,
            api_base_url_source,
            token: token.map(ToOwned::to_owned),
            token_source,
            config_path,
        })
    }

    pub fn token_configured(&self) -> bool {
        self.token.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow!("could not determine a user config directory"))?;
        Ok(config_dir.join("agentics").join("config.toml"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<CliConfig> {
        if !self.path.exists() {
            return Ok(CliConfig::default());
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read config file {}", self.path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file {}", self.path.display()))
    }

    pub fn save(&self, config: &CliConfig) -> Result<()> {
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

pub fn normalize_api_base_url(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("API base URL must not be empty");
    }

    let url = reqwest::Url::parse(trimmed)
        .with_context(|| format!("invalid API base URL `{trimmed}`"))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => bail!("API base URL must use http or https, got `{scheme}`"),
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("API base URL must not include a query string or fragment");
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

fn read_non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

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

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)
    }

    #[cfg(not(unix))]
    {
        fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_config_precedence() {
        let file = CliConfig {
            api_base_url: Some("http://file.example".to_string()),
            token: Some("file-token".to_string()),
        };
        let env = Environment {
            api_base_url: Some("http://env.example".to_string()),
            token: Some("env-token".to_string()),
        };

        let settings = ResolvedSettings::resolve(
            Some("http://flag.example"),
            None,
            &env,
            &file,
            PathBuf::from("config.toml"),
        )
        .expect("settings should resolve");

        assert_eq!(settings.api_base_url, "http://flag.example");
        assert_eq!(settings.api_base_url_source, SettingSource::Flag);
        assert_eq!(settings.token.as_deref(), Some("env-token"));
        assert_eq!(settings.token_source, SettingSource::Environment);
    }

    #[test]
    fn saves_and_loads_config_without_null_fields() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("agentics.toml");
        let store = ConfigStore::new(path.clone());

        store
            .save(&CliConfig {
                api_base_url: Some("http://127.0.0.1:3000".to_string()),
                token: None,
            })
            .expect("config should save");

        let raw = fs::read_to_string(path).expect("config should be readable");
        assert!(!raw.contains("token"));
        assert_eq!(
            store.load().expect("config should load"),
            CliConfig {
                api_base_url: Some("http://127.0.0.1:3000".to_string()),
                token: None,
            }
        );
    }

    #[test]
    fn rejects_invalid_api_base_url() {
        let error = normalize_api_base_url("file:///tmp/api").expect_err("must reject file URL");
        assert!(error.to_string().contains("http or https"));
    }
}
