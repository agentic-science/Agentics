//! Environment-backed runtime configuration.

use figment::{Figment, providers::Env};
use serde::Deserialize;
use std::str::FromStr;

const CONFIG_ENV_PREFIX: &str = "AGENTICS_";
const DEFAULT_ADMIN_USERNAME: &str = "admin";
const DEFAULT_ADMIN_PASSWORD: &str = "agentics-admin";
const DEFAULT_POSTGRES_PORT: u16 = 5432;
const DEFAULT_API_PORT: u16 = 3100;
const DEFAULT_WEB_PORT: u16 = 3001;
const DEFAULT_RUNNER_WRITABLE_STORAGE_MODE: &str = "unbounded";
const DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB: &str = "64,256,1024,4096";

/// Application configuration loaded from `AGENTICS_*` environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_api_host")]
    pub api_host: String,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_storage_root")]
    pub storage_root: String,
    #[serde(default = "default_challenges_root")]
    pub challenges_root: String,
    #[serde(default = "default_admin_username")]
    pub admin_username: String,
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
    #[serde(default)]
    pub allow_insecure_default_admin_credentials: bool,
    #[serde(default = "default_cors_allowed_origins")]
    pub cors_allowed_origins: String,
    #[serde(default = "default_worker_poll_interval_ms")]
    pub worker_poll_interval_ms: u64,
    #[serde(default = "default_worker_stale_job_minutes")]
    pub worker_stale_job_minutes: i32,
    #[serde(default = "default_validation_runs_per_agent_challenge_day")]
    pub validation_runs_per_agent_challenge_day: u32,
    #[serde(default = "default_official_runs_per_agent_challenge_day")]
    pub official_runs_per_agent_challenge_day: u32,
    #[serde(default = "default_max_active_official_jobs")]
    pub max_active_official_jobs: u32,
    #[serde(default = "default_max_active_agents")]
    pub max_active_agents: u32,
    #[serde(default = "default_max_active_challenge_drafts_per_agent")]
    pub max_active_challenge_drafts_per_agent: u32,
    #[serde(default = "default_challenge_private_asset_bytes_per_draft")]
    pub challenge_private_asset_bytes_per_draft: u64,
    #[serde(default = "default_challenge_draft_validations_per_day")]
    pub challenge_draft_validations_per_day: u32,
    #[serde(default = "default_challenge_draft_ttl_days")]
    pub challenge_draft_ttl_days: i64,
    #[serde(default = "default_unpublished_challenge_asset_grace_days")]
    pub unpublished_challenge_asset_grace_days: i64,
    #[serde(default)]
    pub github_oauth_client_id: Option<String>,
    #[serde(default)]
    pub github_oauth_client_secret: Option<String>,
    #[serde(default)]
    pub github_oauth_redirect_url: Option<String>,
    #[serde(default = "default_github_oauth_authorize_url")]
    pub github_oauth_authorize_url: String,
    #[serde(default = "default_github_oauth_token_url")]
    pub github_oauth_token_url: String,
    #[serde(default = "default_github_api_user_url")]
    pub github_api_user_url: String,
    #[serde(default = "default_web_session_cookie_name")]
    pub web_session_cookie_name: String,
    #[serde(default = "default_web_csrf_cookie_name")]
    pub web_csrf_cookie_name: String,
    #[serde(default = "default_web_session_ttl_hours")]
    pub web_session_ttl_hours: i64,
    #[serde(default)]
    pub web_session_cookie_secure: bool,
    #[serde(default)]
    pub allow_public_agent_registration_on_non_loopback: bool,
    /// Optional Docker host URI used by CI or remote Docker setups.
    #[serde(default)]
    pub docker_host: Option<String>,
    #[serde(default)]
    pub require_digest_pinned_images: bool,
    #[serde(default = "default_runner_writable_storage_mode")]
    pub runner_writable_storage_mode: String,
    #[serde(default)]
    pub runner_phase_mount_root: Option<String>,
    #[serde(default = "default_runner_writable_slot_classes_mb")]
    pub runner_writable_slot_classes_mb: String,
    #[serde(default)]
    pub runner_docker_layer_quota: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

/// Runner strategy for Docker bind-mounted writable paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerWritableStorageMode {
    /// Keep writable paths under `AGENTICS_STORAGE_ROOT`.
    Unbounded,
    /// Lease root-prepared XFS project-quota slots for writable container paths.
    XfsProjectQuotaSlots,
}

impl FromStr for RunnerWritableStorageMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "unbounded" => Ok(Self::Unbounded),
            "xfs-project-quota-slots" => Ok(Self::XfsProjectQuotaSlots),
            other => anyhow::bail!(
                "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE must be `unbounded` or `xfs-project-quota-slots`, got `{other}`"
            ),
        }
    }
}

fn default_database_url() -> String {
    format!(
        "postgres://agentics:agentics@127.0.0.1:{}/agentics",
        env_port("AGENTICS_POSTGRES_PORT", DEFAULT_POSTGRES_PORT)
    )
}

fn default_api_host() -> String {
    "127.0.0.1".to_string()
}

fn default_api_port() -> u16 {
    DEFAULT_API_PORT
}

fn default_storage_root() -> String {
    "storage".to_string()
}

fn default_challenges_root() -> String {
    "examples/challenges".to_string()
}

fn default_admin_username() -> String {
    DEFAULT_ADMIN_USERNAME.to_string()
}

fn default_admin_password() -> String {
    DEFAULT_ADMIN_PASSWORD.to_string()
}

fn default_cors_allowed_origins() -> String {
    let web_port = env_port("AGENTICS_WEB_PORT", DEFAULT_WEB_PORT);
    format!("http://127.0.0.1:{web_port},http://localhost:{web_port}")
}

fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default)
}

fn default_worker_poll_interval_ms() -> u64 {
    3000
}

fn default_worker_stale_job_minutes() -> i32 {
    1
}

fn default_validation_runs_per_agent_challenge_day() -> u32 {
    20
}

fn default_official_runs_per_agent_challenge_day() -> u32 {
    5
}

fn default_max_active_official_jobs() -> u32 {
    20
}

fn default_max_active_agents() -> u32 {
    1_000
}

fn default_max_active_challenge_drafts_per_agent() -> u32 {
    10
}

fn default_challenge_private_asset_bytes_per_draft() -> u64 {
    250 * 1024 * 1024
}

fn default_challenge_draft_validations_per_day() -> u32 {
    10
}

fn default_challenge_draft_ttl_days() -> i64 {
    14
}

fn default_unpublished_challenge_asset_grace_days() -> i64 {
    7
}

fn default_github_oauth_authorize_url() -> String {
    "https://github.com/login/oauth/authorize".to_string()
}

fn default_github_oauth_token_url() -> String {
    "https://github.com/login/oauth/access_token".to_string()
}

fn default_github_api_user_url() -> String {
    "https://api.github.com/user".to_string()
}

fn default_web_session_cookie_name() -> String {
    "agentics_session".to_string()
}

fn default_web_csrf_cookie_name() -> String {
    "agentics_csrf".to_string()
}

fn default_web_session_ttl_hours() -> i64 {
    24
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_runner_writable_storage_mode() -> String {
    DEFAULT_RUNNER_WRITABLE_STORAGE_MODE.to_string()
}

fn default_runner_writable_slot_classes_mb() -> String {
    DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB.to_string()
}

impl Config {
    /// Load configuration from `AGENTICS_*` environment variables with defaults.
    pub fn from_env() -> anyhow::Result<Self> {
        let config: Config = Figment::new()
            .merge(Env::prefixed(CONFIG_ENV_PREFIX))
            .extract()?;
        Ok(config)
    }

    /// Reject settings that are acceptable for local development but dangerous
    /// when the API is reachable from another machine.
    pub fn validate_api_security(&self) -> anyhow::Result<()> {
        if self.uses_default_admin_credentials()
            && !self.allow_insecure_default_admin_credentials
            && !is_loopback_host(&self.api_host)
        {
            anyhow::bail!(
                "refusing to bind API to `{}` with default admin credentials; set AGENTICS_ADMIN_PASSWORD or explicitly set AGENTICS_ALLOW_INSECURE_DEFAULT_ADMIN_CREDENTIALS=true for local-only development",
                self.api_host
            );
        }

        if !is_loopback_host(&self.api_host)
            && !self.allow_public_agent_registration_on_non_loopback
        {
            anyhow::bail!(
                "refusing to bind API to `{}` with public agent registration enabled; set AGENTICS_ALLOW_PUBLIC_AGENT_REGISTRATION_ON_NON_LOOPBACK=true only after adding deployment-level rate limits",
                self.api_host
            );
        }

        if self.max_active_agents == 0 {
            anyhow::bail!("AGENTICS_MAX_ACTIVE_AGENTS must be greater than zero");
        }
        if self.official_runs_per_agent_challenge_day == 0 {
            anyhow::bail!(
                "AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY must be greater than zero"
            );
        }
        if self.max_active_official_jobs == 0 {
            anyhow::bail!("AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS must be greater than zero");
        }
        if self.max_active_challenge_drafts_per_agent == 0 {
            anyhow::bail!(
                "AGENTICS_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT must be greater than zero"
            );
        }
        if self.challenge_private_asset_bytes_per_draft == 0 {
            anyhow::bail!(
                "AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT must be greater than zero"
            );
        }
        if self.challenge_draft_validations_per_day == 0 {
            anyhow::bail!("AGENTICS_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY must be greater than zero");
        }
        if self.challenge_draft_ttl_days <= 0 {
            anyhow::bail!("AGENTICS_CHALLENGE_DRAFT_TTL_DAYS must be greater than zero");
        }
        if self.unpublished_challenge_asset_grace_days <= 0 {
            anyhow::bail!(
                "AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS must be greater than zero"
            );
        }
        if self.web_session_ttl_hours <= 0 {
            anyhow::bail!("AGENTICS_WEB_SESSION_TTL_HOURS must be greater than zero");
        }
        validate_cookie_name(
            &self.web_session_cookie_name,
            "AGENTICS_WEB_SESSION_COOKIE_NAME",
        )?;
        validate_cookie_name(&self.web_csrf_cookie_name, "AGENTICS_WEB_CSRF_COOKIE_NAME")?;
        if self.web_session_cookie_name == self.web_csrf_cookie_name {
            anyhow::bail!(
                "AGENTICS_WEB_SESSION_COOKIE_NAME and AGENTICS_WEB_CSRF_COOKIE_NAME must differ"
            );
        }
        if !is_loopback_host(&self.api_host) && !self.web_session_cookie_secure {
            anyhow::bail!(
                "AGENTICS_WEB_SESSION_COOKIE_SECURE must be true when the API is reachable from another machine"
            );
        }
        if self.github_oauth_client_id.is_some()
            || self.github_oauth_client_secret.is_some()
            || self.github_oauth_redirect_url.is_some()
        {
            validate_required_trimmed(
                self.github_oauth_client_id.as_deref(),
                "AGENTICS_GITHUB_OAUTH_CLIENT_ID",
            )?;
            validate_required_trimmed(
                self.github_oauth_client_secret.as_deref(),
                "AGENTICS_GITHUB_OAUTH_CLIENT_SECRET",
            )?;
            validate_required_trimmed(
                self.github_oauth_redirect_url.as_deref(),
                "AGENTICS_GITHUB_OAUTH_REDIRECT_URL",
            )?;
        }

        Ok(())
    }

    /// Validate worker-only storage settings before claiming evaluation jobs.
    pub fn validate_runner_storage(&self) -> anyhow::Result<()> {
        match self.runner_writable_storage_mode()? {
            RunnerWritableStorageMode::Unbounded => {}
            RunnerWritableStorageMode::XfsProjectQuotaSlots => {
                if !cfg!(target_os = "linux") {
                    anyhow::bail!(
                        "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots is Linux-only"
                    );
                }
                let mount_root = self
                    .runner_phase_mount_root
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "AGENTICS_RUNNER_PHASE_MOUNT_ROOT must be set when AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots"
                        )
                    })?;
                if !std::path::Path::new(mount_root).is_absolute() {
                    anyhow::bail!("AGENTICS_RUNNER_PHASE_MOUNT_ROOT must be an absolute path");
                }
                if self.runner_writable_slot_classes_mb()?.is_empty() {
                    anyhow::bail!("AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB must not be empty");
                }
            }
        }

        if self.runner_docker_layer_quota && !cfg!(target_os = "linux") {
            anyhow::bail!("AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true is Linux-only");
        }

        Ok(())
    }

    pub fn runner_writable_storage_mode(&self) -> anyhow::Result<RunnerWritableStorageMode> {
        self.runner_writable_storage_mode.parse()
    }

    pub fn runner_writable_slot_classes_mb(&self) -> anyhow::Result<Vec<u64>> {
        let mut classes = Vec::new();
        for raw in self
            .runner_writable_slot_classes_mb
            .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        {
            let value = raw.trim();
            if value.is_empty() {
                continue;
            }
            let parsed = value.parse::<u64>().map_err(|e| {
                anyhow::anyhow!(
                    "invalid AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB entry `{value}`: {e}"
                )
            })?;
            if parsed == 0 {
                anyhow::bail!("AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB entries must be positive");
            }
            classes.push(parsed);
        }
        classes.sort_unstable();
        classes.dedup();
        Ok(classes)
    }

    /// Split the comma-separated CORS allowlist into trimmed origin strings.
    pub fn cors_allowed_origin_values(&self) -> Vec<String> {
        self.cors_allowed_origins
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    fn uses_default_admin_credentials(&self) -> bool {
        self.admin_username == DEFAULT_ADMIN_USERNAME
            && self.admin_password == DEFAULT_ADMIN_PASSWORD
    }

    /// Return whether GitHub OAuth is fully configured for creator login.
    pub fn github_oauth_enabled(&self) -> bool {
        self.github_oauth_client_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self
                .github_oauth_client_secret
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            && self
                .github_oauth_redirect_url
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.parse::<std::net::IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

fn validate_required_trimmed(value: Option<&str>, field: &str) -> anyhow::Result<()> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        anyhow::bail!("{field} must be set when GitHub OAuth is configured");
    }
    Ok(())
}

fn validate_cookie_name(value: &str, field: &str) -> anyhow::Result<()> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    if !value
        .bytes()
        .all(|byte| matches!(byte, b'!' | b'#'..=b'\'' | b'*' | b'+' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'))
    {
        anyhow::bail!("{field} contains characters that are not valid in a cookie name");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CONFIG_ENV_PREFIX, Config};

    #[test]
    fn uses_agentics_environment_prefix() {
        assert_eq!(CONFIG_ENV_PREFIX, "AGENTICS_");
    }

    #[test]
    fn default_api_host_is_loopback() {
        let config = Config {
            database_url: String::new(),
            api_host: super::default_api_host(),
            api_port: 3100,
            storage_root: String::new(),
            challenges_root: String::new(),
            admin_username: super::default_admin_username(),
            admin_password: super::default_admin_password(),
            allow_insecure_default_admin_credentials: false,
            cors_allowed_origins: super::default_cors_allowed_origins(),
            worker_poll_interval_ms: 3000,
            worker_stale_job_minutes: 1,
            validation_runs_per_agent_challenge_day: 20,
            official_runs_per_agent_challenge_day: 5,
            max_active_official_jobs: 20,
            max_active_agents: 1_000,
            max_active_challenge_drafts_per_agent: 10,
            challenge_private_asset_bytes_per_draft: 250 * 1024 * 1024,
            challenge_draft_validations_per_day: 10,
            challenge_draft_ttl_days: 14,
            unpublished_challenge_asset_grace_days: 7,
            github_oauth_client_id: None,
            github_oauth_client_secret: None,
            github_oauth_redirect_url: None,
            github_oauth_authorize_url: super::default_github_oauth_authorize_url(),
            github_oauth_token_url: super::default_github_oauth_token_url(),
            github_api_user_url: super::default_github_api_user_url(),
            web_session_cookie_name: super::default_web_session_cookie_name(),
            web_csrf_cookie_name: super::default_web_csrf_cookie_name(),
            web_session_ttl_hours: super::default_web_session_ttl_hours(),
            web_session_cookie_secure: false,
            allow_public_agent_registration_on_non_loopback: false,
            docker_host: None,
            require_digest_pinned_images: false,
            runner_writable_storage_mode: super::default_runner_writable_storage_mode(),
            runner_phase_mount_root: None,
            runner_writable_slot_classes_mb: super::default_runner_writable_slot_classes_mb(),
            runner_docker_layer_quota: false,
            log_level: "info".to_string(),
        };

        assert!(config.validate_api_security().is_ok());
    }

    #[test]
    fn default_api_port_avoids_common_frontend_port() {
        assert_eq!(super::default_api_port(), 3100);
    }

    #[test]
    fn default_admin_credentials_are_rejected_on_wildcard_bind() {
        let mut config = Config {
            database_url: String::new(),
            api_host: "0.0.0.0".to_string(),
            api_port: 3100,
            storage_root: String::new(),
            challenges_root: String::new(),
            admin_username: super::default_admin_username(),
            admin_password: super::default_admin_password(),
            allow_insecure_default_admin_credentials: false,
            cors_allowed_origins: super::default_cors_allowed_origins(),
            worker_poll_interval_ms: 3000,
            worker_stale_job_minutes: 1,
            validation_runs_per_agent_challenge_day: 20,
            official_runs_per_agent_challenge_day: 5,
            max_active_official_jobs: 20,
            max_active_agents: 1_000,
            max_active_challenge_drafts_per_agent: 10,
            challenge_private_asset_bytes_per_draft: 250 * 1024 * 1024,
            challenge_draft_validations_per_day: 10,
            challenge_draft_ttl_days: 14,
            unpublished_challenge_asset_grace_days: 7,
            github_oauth_client_id: None,
            github_oauth_client_secret: None,
            github_oauth_redirect_url: None,
            github_oauth_authorize_url: super::default_github_oauth_authorize_url(),
            github_oauth_token_url: super::default_github_oauth_token_url(),
            github_api_user_url: super::default_github_api_user_url(),
            web_session_cookie_name: super::default_web_session_cookie_name(),
            web_csrf_cookie_name: super::default_web_csrf_cookie_name(),
            web_session_ttl_hours: super::default_web_session_ttl_hours(),
            web_session_cookie_secure: false,
            allow_public_agent_registration_on_non_loopback: false,
            docker_host: None,
            require_digest_pinned_images: false,
            runner_writable_storage_mode: super::default_runner_writable_storage_mode(),
            runner_phase_mount_root: None,
            runner_writable_slot_classes_mb: super::default_runner_writable_slot_classes_mb(),
            runner_docker_layer_quota: false,
            log_level: "info".to_string(),
        };

        assert!(config.validate_api_security().is_err());

        config.admin_password = "changed".to_string();
        assert!(config.validate_api_security().is_err());

        config.allow_public_agent_registration_on_non_loopback = true;
        config.web_session_cookie_secure = true;
        assert!(config.validate_api_security().is_ok());
    }

    #[test]
    fn parses_runner_writable_slot_classes() {
        let config = Config {
            database_url: String::new(),
            api_host: super::default_api_host(),
            api_port: 3100,
            storage_root: String::new(),
            challenges_root: String::new(),
            admin_username: super::default_admin_username(),
            admin_password: super::default_admin_password(),
            allow_insecure_default_admin_credentials: false,
            cors_allowed_origins: super::default_cors_allowed_origins(),
            worker_poll_interval_ms: 3000,
            worker_stale_job_minutes: 1,
            validation_runs_per_agent_challenge_day: 20,
            official_runs_per_agent_challenge_day: 5,
            max_active_official_jobs: 20,
            max_active_agents: 1_000,
            max_active_challenge_drafts_per_agent: 10,
            challenge_private_asset_bytes_per_draft: 250 * 1024 * 1024,
            challenge_draft_validations_per_day: 10,
            challenge_draft_ttl_days: 14,
            unpublished_challenge_asset_grace_days: 7,
            github_oauth_client_id: None,
            github_oauth_client_secret: None,
            github_oauth_redirect_url: None,
            github_oauth_authorize_url: super::default_github_oauth_authorize_url(),
            github_oauth_token_url: super::default_github_oauth_token_url(),
            github_api_user_url: super::default_github_api_user_url(),
            web_session_cookie_name: super::default_web_session_cookie_name(),
            web_csrf_cookie_name: super::default_web_csrf_cookie_name(),
            web_session_ttl_hours: super::default_web_session_ttl_hours(),
            web_session_cookie_secure: false,
            allow_public_agent_registration_on_non_loopback: false,
            docker_host: None,
            require_digest_pinned_images: false,
            runner_writable_storage_mode: super::default_runner_writable_storage_mode(),
            runner_phase_mount_root: None,
            runner_writable_slot_classes_mb: "1024,64 256,1024".to_string(),
            runner_docker_layer_quota: false,
            log_level: "info".to_string(),
        };

        assert_eq!(
            config.runner_writable_slot_classes_mb().unwrap(),
            vec![64, 256, 1024]
        );
    }
}
