//! Environment-backed runtime configuration.

use figment::{Figment, providers::Env};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::models::urls::{
    GithubApiUserUrl, GithubOauthAuthorizeUrl, GithubOauthRedirectUrl, GithubOauthTokenUrl,
};

const CONFIG_ENV_PREFIX: &str = "AGENTICS_";
const DEFAULT_ADMIN_USERNAME: &str = "admin";
const DEFAULT_ADMIN_PASSWORD: &str = "agentics-admin";
const DEFAULT_POSTGRES_PORT: u16 = 5432;
const DEFAULT_API_PORT: u16 = 3100;
const DEFAULT_WEB_PORT: u16 = 3001;
const DEFAULT_AGENT_REGISTRATION_MODE: &str = "pioneer_code";
const DEFAULT_RUNNER_WRITABLE_STORAGE_MODE: &str = "unbounded";
const DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB: &str = "64,256,1024,4096";
const DEFAULT_RUNNER_MAX_OUTPUT_FILES: u64 = 8192;
const DEFAULT_RUNNER_MAX_OUTPUT_DIRS: u64 = 1024;
const DEFAULT_RUNNER_MAX_OUTPUT_DEPTH: u64 = 32;
const DEFAULT_RUNNER_MAX_RUNS: u64 = crate::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION;
const DEFAULT_RUNNER_MAX_RESULT_JSON_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_RUNNER_MAX_PUBLIC_RESULTS: u64 = 1024;
const DEFAULT_RUNNER_MAX_RESULT_LOG_BYTES: u64 = 256 * 1024;
const DEFAULT_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION: u64 = 16 * 1024 * 1024;
const DEFAULT_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS: u64 = 2;

/// Application configuration loaded from `AGENTICS_*` environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_database_url")]
    pub database_url: SecretString,
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
    pub admin_password: SecretString,
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
    #[serde(default = "default_challenge_draft_validation_timeout_minutes")]
    pub challenge_draft_validation_timeout_minutes: i32,
    #[serde(default = "default_challenge_private_asset_pending_timeout_minutes")]
    pub challenge_private_asset_pending_timeout_minutes: i32,
    #[serde(default = "default_challenge_draft_publish_timeout_minutes")]
    pub challenge_draft_publish_timeout_minutes: i32,
    #[serde(default = "default_challenge_draft_ttl_days")]
    pub challenge_draft_ttl_days: i64,
    #[serde(default = "default_unpublished_challenge_asset_grace_days")]
    pub unpublished_challenge_asset_grace_days: i64,
    #[serde(default)]
    pub github_oauth_client_id: Option<String>,
    #[serde(default)]
    pub github_oauth_client_secret: Option<SecretString>,
    #[serde(default)]
    pub github_oauth_redirect_url: Option<GithubOauthRedirectUrl>,
    #[serde(default = "default_github_oauth_authorize_url")]
    pub github_oauth_authorize_url: GithubOauthAuthorizeUrl,
    #[serde(default = "default_github_oauth_token_url")]
    pub github_oauth_token_url: GithubOauthTokenUrl,
    #[serde(default = "default_github_api_user_url")]
    pub github_api_user_url: GithubApiUserUrl,
    #[serde(default = "default_web_session_cookie_name")]
    pub web_session_cookie_name: String,
    #[serde(default = "default_web_csrf_cookie_name")]
    pub web_csrf_cookie_name: String,
    #[serde(default = "default_web_session_ttl_hours")]
    pub web_session_ttl_hours: i64,
    #[serde(default)]
    pub web_session_cookie_secure: bool,
    #[serde(default = "default_agent_registration_mode")]
    pub agent_registration_mode: String,
    /// Optional Docker host URI used by CI or remote Docker setups.
    #[serde(default)]
    pub docker_host: Option<String>,
    #[serde(default = "default_host_probe_mode")]
    pub host_probe_mode: HostProbeMode,
    #[serde(default)]
    pub require_digest_pinned_images: bool,
    #[serde(default = "default_runner_writable_storage_mode")]
    pub runner_writable_storage_mode: String,
    #[serde(default)]
    pub runner_runtime_root: Option<String>,
    #[serde(default)]
    pub runner_phase_mount_root: Option<String>,
    #[serde(default = "default_runner_writable_slot_classes_mb")]
    pub runner_writable_slot_classes_mb: String,
    #[serde(default)]
    pub runner_docker_layer_quota: bool,
    #[serde(default = "default_runner_max_output_files")]
    pub runner_max_output_files: u64,
    #[serde(default = "default_runner_max_output_dirs")]
    pub runner_max_output_dirs: u64,
    #[serde(default = "default_runner_max_output_depth")]
    pub runner_max_output_depth: u64,
    #[serde(default = "default_runner_max_runs")]
    pub runner_max_runs: u64,
    #[serde(default = "default_runner_max_result_json_bytes")]
    pub runner_max_result_json_bytes: u64,
    #[serde(default = "default_runner_max_public_results")]
    pub runner_max_public_results: u64,
    #[serde(default = "default_runner_max_result_log_bytes")]
    pub runner_max_result_log_bytes: u64,
    #[serde(default = "default_runner_max_interaction_bytes_per_direction")]
    pub runner_max_interaction_bytes_per_direction: u64,
    #[serde(default = "default_runner_interaction_shutdown_grace_secs")]
    pub runner_interaction_shutdown_grace_secs: u64,
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

/// Policy for unauthenticated agent-account registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRegistrationMode {
    /// Require a valid pioneer code for every new agent account.
    PioneerCode,
    /// Allow code-free registration for local testing and development only.
    Public,
}

/// Worker startup host-profile probe policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostProbeMode {
    /// Do not run hosted profile checks.
    Off,
    /// Run hosted profile checks and log failures without blocking startup.
    Warn,
    /// Run hosted profile checks and fail worker startup if they fail or are skipped.
    Require,
}

impl HostProbeMode {
    /// Stable environment string for this policy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Require => "require",
        }
    }
}

impl FromStr for AgentRegistrationMode {
    type Err = anyhow::Error;

    /// Parse the configured agent-registration mode.
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "pioneer_code" => Ok(Self::PioneerCode),
            "public" => Ok(Self::Public),
            other => anyhow::bail!(
                "AGENTICS_AGENT_REGISTRATION_MODE must be `pioneer_code` or `public`, got `{other}`"
            ),
        }
    }
}

impl FromStr for RunnerWritableStorageMode {
    type Err = anyhow::Error;

    /// Handles from str for this module.
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

/// Build the default local database URL without exposing it through Debug output.
fn default_database_url() -> SecretString {
    SecretString::from(format!(
        "postgres://agentics:agentics@127.0.0.1:{}/agentics",
        env_port("AGENTICS_POSTGRES_PORT", DEFAULT_POSTGRES_PORT)
    ))
}

/// Validate one configured CORS origin before the router accepts it.
fn validate_cors_origin(origin: &str) -> anyhow::Result<()> {
    origin.parse::<http::HeaderValue>().map_err(|e| {
        anyhow::anyhow!("AGENTICS_CORS_ALLOWED_ORIGINS contains invalid origin `{origin}`: {e}")
    })?;
    let parsed = url::Url::parse(origin).map_err(|e| {
        anyhow::anyhow!("AGENTICS_CORS_ALLOWED_ORIGINS contains invalid origin `{origin}`: {e}")
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        anyhow::bail!(
            "AGENTICS_CORS_ALLOWED_ORIGINS contains invalid origin `{origin}`: expected an http(s) origin without path, query, or fragment"
        );
    }
    Ok(())
}

/// Handles default api host for this module.
fn default_api_host() -> String {
    "127.0.0.1".to_string()
}

/// Handles default api port for this module.
fn default_api_port() -> u16 {
    DEFAULT_API_PORT
}

/// Handles default storage root for this module.
fn default_storage_root() -> String {
    "storage".to_string()
}

/// Handles default challenges root for this module.
fn default_challenges_root() -> String {
    "examples/challenges".to_string()
}

/// Handles default admin username for this module.
fn default_admin_username() -> String {
    DEFAULT_ADMIN_USERNAME.to_string()
}

/// Handles default admin password for this module.
fn default_admin_password() -> SecretString {
    SecretString::from(DEFAULT_ADMIN_PASSWORD)
}

/// Handles default cors allowed origins for this module.
fn default_cors_allowed_origins() -> String {
    let web_port = env_port("AGENTICS_WEB_PORT", DEFAULT_WEB_PORT);
    format!("http://127.0.0.1:{web_port},http://localhost:{web_port}")
}

/// Handles env port for this module.
fn env_port(name: &str, default: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(default)
}

/// Handles default worker poll interval ms for this module.
fn default_worker_poll_interval_ms() -> u64 {
    3000
}

/// Handles default worker stale job minutes for this module.
fn default_worker_stale_job_minutes() -> i32 {
    1
}

/// Handles default validation runs per agent challenge day for this module.
fn default_validation_runs_per_agent_challenge_day() -> u32 {
    20
}

/// Handles default official runs per agent challenge day for this module.
fn default_official_runs_per_agent_challenge_day() -> u32 {
    5
}

/// Handles default max active official jobs for this module.
fn default_max_active_official_jobs() -> u32 {
    20
}

/// Handles default max active agents for this module.
fn default_max_active_agents() -> u32 {
    1_000
}

/// Handles default max active challenge drafts per agent for this module.
fn default_max_active_challenge_drafts_per_agent() -> u32 {
    10
}

/// Handles default challenge private asset bytes per draft for this module.
fn default_challenge_private_asset_bytes_per_draft() -> u64 {
    250 * 1024 * 1024
}

/// Handles default challenge draft validations per day for this module.
fn default_challenge_draft_validations_per_day() -> u32 {
    10
}

/// Handles default challenge draft validation timeout minutes for this module.
fn default_challenge_draft_validation_timeout_minutes() -> i32 {
    30
}

/// Handles default private asset pending timeout minutes for this module.
fn default_challenge_private_asset_pending_timeout_minutes() -> i32 {
    30
}

/// Handles default challenge draft publish timeout minutes for this module.
fn default_challenge_draft_publish_timeout_minutes() -> i32 {
    30
}

/// Handles default challenge draft ttl days for this module.
fn default_challenge_draft_ttl_days() -> i64 {
    14
}

/// Handles default unpublished challenge asset grace days for this module.
fn default_unpublished_challenge_asset_grace_days() -> i64 {
    7
}

#[allow(
    clippy::expect_used,
    reason = "static default URLs are validated by type constructors and have no runtime fallback"
)]
/// Handles default github oauth authorize url for this module.
fn default_github_oauth_authorize_url() -> GithubOauthAuthorizeUrl {
    GithubOauthAuthorizeUrl::try_new("https://github.com/login/oauth/authorize")
        .expect("default GitHub OAuth authorize URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "static default URLs are validated by type constructors and have no runtime fallback"
)]
/// Handles default github oauth token url for this module.
fn default_github_oauth_token_url() -> GithubOauthTokenUrl {
    GithubOauthTokenUrl::try_new("https://github.com/login/oauth/access_token")
        .expect("default GitHub OAuth token URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "static default URLs are validated by type constructors and have no runtime fallback"
)]
/// Handles default github api user url for this module.
fn default_github_api_user_url() -> GithubApiUserUrl {
    GithubApiUserUrl::try_new("https://api.github.com/user")
        .expect("default GitHub API user URL must be valid")
}

/// Handles default web session cookie name for this module.
fn default_web_session_cookie_name() -> String {
    "agentics_session".to_string()
}

/// Handles default web csrf cookie name for this module.
fn default_web_csrf_cookie_name() -> String {
    "agentics_csrf".to_string()
}

/// Handles default web session ttl hours for this module.
fn default_web_session_ttl_hours() -> i64 {
    24
}

/// Default MVP registration mode that requires pioneer codes.
fn default_agent_registration_mode() -> String {
    DEFAULT_AGENT_REGISTRATION_MODE.to_string()
}

/// Handles default log level for this module.
fn default_log_level() -> String {
    "info".to_string()
}

/// Handles default runner writable storage mode for this module.
fn default_runner_writable_storage_mode() -> String {
    DEFAULT_RUNNER_WRITABLE_STORAGE_MODE.to_string()
}

/// Handles default runner writable slot classes mb for this module.
fn default_runner_writable_slot_classes_mb() -> String {
    DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB.to_string()
}

/// Default maximum regular files accepted in one evaluator-visible run tree.
fn default_runner_max_output_files() -> u64 {
    DEFAULT_RUNNER_MAX_OUTPUT_FILES
}

/// Default maximum directories accepted in one evaluator-visible run tree.
fn default_runner_max_output_dirs() -> u64 {
    DEFAULT_RUNNER_MAX_OUTPUT_DIRS
}

/// Default maximum path depth accepted in one evaluator-visible run tree.
fn default_runner_max_output_depth() -> u64 {
    DEFAULT_RUNNER_MAX_OUTPUT_DEPTH
}

/// Default maximum solution invocations accepted in one evaluation.
fn default_runner_max_runs() -> u64 {
    DEFAULT_RUNNER_MAX_RUNS
}

/// Default maximum raw evaluator result JSON bytes accepted before parsing.
fn default_runner_max_result_json_bytes() -> u64 {
    DEFAULT_RUNNER_MAX_RESULT_JSON_BYTES
}

/// Default maximum public case result entries accepted in evaluator output.
fn default_runner_max_public_results() -> u64 {
    DEFAULT_RUNNER_MAX_PUBLIC_RESULTS
}

/// Default maximum embedded evaluator log bytes accepted in evaluator output.
fn default_runner_max_result_log_bytes() -> u64 {
    DEFAULT_RUNNER_MAX_RESULT_LOG_BYTES
}

/// Default maximum bytes relayed in each direction during a piped-stdio interaction.
fn default_runner_max_interaction_bytes_per_direction() -> u64 {
    DEFAULT_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION
}

/// Default grace period for attached stdio pumps after interactive containers exit.
fn default_runner_interaction_shutdown_grace_secs() -> u64 {
    DEFAULT_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS
}

/// Default hosted profile probe mode for local development.
fn default_host_probe_mode() -> HostProbeMode {
    HostProbeMode::Off
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
            && self.agent_registration_mode()? == AgentRegistrationMode::Public
        {
            anyhow::bail!(
                "refusing to bind API to `{}` with AGENTICS_AGENT_REGISTRATION_MODE=public; public registration is local-development only",
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
        if self.challenge_draft_validation_timeout_minutes <= 0 {
            anyhow::bail!(
                "AGENTICS_CHALLENGE_DRAFT_VALIDATION_TIMEOUT_MINUTES must be greater than zero"
            );
        }
        if self.challenge_private_asset_pending_timeout_minutes <= 0 {
            anyhow::bail!(
                "AGENTICS_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES must be greater than zero"
            );
        }
        if self.challenge_draft_publish_timeout_minutes <= 0 {
            anyhow::bail!(
                "AGENTICS_CHALLENGE_DRAFT_PUBLISH_TIMEOUT_MINUTES must be greater than zero"
            );
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
        for origin in self.cors_allowed_origin_values() {
            validate_cors_origin(&origin)?;
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
                self.github_oauth_client_secret
                    .as_ref()
                    .map(ExposeSecret::expose_secret),
                "AGENTICS_GITHUB_OAUTH_CLIENT_SECRET",
            )?;
            validate_required_trimmed(
                self.github_oauth_redirect_url
                    .as_ref()
                    .map(GithubOauthRedirectUrl::as_str),
                "AGENTICS_GITHUB_OAUTH_REDIRECT_URL",
            )?;
        }
        self.validate_hosted_image_policy()?;

        Ok(())
    }

    /// Validate worker-only storage settings before claiming evaluation jobs.
    pub fn validate_runner_storage(&self) -> anyhow::Result<()> {
        self.validate_runner_output_limits()?;
        self.validate_hosted_image_policy()?;

        match self.runner_writable_storage_mode()? {
            RunnerWritableStorageMode::Unbounded => {
                if !is_loopback_host(&self.api_host) {
                    anyhow::bail!(
                        "unbounded runner writable storage is allowed only for loopback development; set AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots for hosted workers"
                    );
                }
            }
            RunnerWritableStorageMode::XfsProjectQuotaSlots => {
                if !cfg!(target_os = "linux") {
                    anyhow::bail!(
                        "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots is Linux-only"
                    );
                }
                if !self.runner_docker_layer_quota {
                    anyhow::bail!(
                        "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true is required alongside AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots"
                    );
                }
                self.validate_required_runner_runtime_root(
                    "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots",
                )?;
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
        if self.host_probe_mode == HostProbeMode::Require && !self.runner_docker_layer_quota {
            anyhow::bail!(
                "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true is required when AGENTICS_HOST_PROBE_MODE=require"
            );
        }
        if self.host_probe_mode != HostProbeMode::Off && !cfg!(target_os = "linux") {
            anyhow::bail!(
                "AGENTICS_HOST_PROBE_MODE={} is Linux-only",
                self.host_probe_mode.as_str()
            );
        }
        if self.host_probe_mode != HostProbeMode::Off {
            self.validate_required_runner_runtime_root("AGENTICS_HOST_PROBE_MODE is enabled")?;
        }
        if let Some(runtime_root) = self
            .runner_runtime_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            && !Path::new(runtime_root).is_absolute()
        {
            anyhow::bail!("AGENTICS_RUNNER_RUNTIME_ROOT must be an absolute path");
        }

        Ok(())
    }

    /// Return whether this configuration must enforce immutable hosted images.
    pub fn requires_digest_pinned_images(&self) -> bool {
        self.require_digest_pinned_images || self.host_probe_mode == HostProbeMode::Require
    }

    /// Reject hosted profiles that try to opt out of immutable image references.
    fn validate_hosted_image_policy(&self) -> anyhow::Result<()> {
        if self.requires_digest_pinned_images() && !self.require_digest_pinned_images {
            anyhow::bail!(
                "AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES must be true for hosted profiles using AGENTICS_HOST_PROBE_MODE=require"
            );
        }
        Ok(())
    }

    /// Validate platform-owned output tree limits.
    fn validate_runner_output_limits(&self) -> anyhow::Result<()> {
        if self.runner_max_output_files == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_OUTPUT_FILES must be greater than zero");
        }
        if self.runner_max_output_dirs == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_OUTPUT_DIRS must be greater than zero");
        }
        if self.runner_max_output_depth == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_OUTPUT_DEPTH must be greater than zero");
        }
        if self.runner_max_runs == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_RUNS must be greater than zero");
        }
        if self.runner_max_runs > crate::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION {
            anyhow::bail!(
                "AGENTICS_RUNNER_MAX_RUNS must be at most {}",
                crate::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION
            );
        }
        if self.runner_max_result_json_bytes == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES must be greater than zero");
        }
        if self.runner_max_public_results == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_PUBLIC_RESULTS must be greater than zero");
        }
        if self.runner_max_result_log_bytes == 0 {
            anyhow::bail!("AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES must be greater than zero");
        }
        if self.runner_max_interaction_bytes_per_direction == 0 {
            anyhow::bail!(
                "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION must be greater than zero"
            );
        }
        if self.runner_interaction_shutdown_grace_secs == 0 {
            anyhow::bail!(
                "AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS must be greater than zero"
            );
        }
        Ok(())
    }

    /// Handles runner writable storage mode for this module.
    pub fn runner_writable_storage_mode(&self) -> anyhow::Result<RunnerWritableStorageMode> {
        self.runner_writable_storage_mode.parse()
    }

    /// Return the host-visible root for transient runner artifacts.
    pub fn runner_runtime_root(&self) -> anyhow::Result<PathBuf> {
        let Some(runtime_root) = self
            .runner_runtime_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(std::env::temp_dir());
        };
        if !Path::new(runtime_root).is_absolute() {
            anyhow::bail!("AGENTICS_RUNNER_RUNTIME_ROOT must be an absolute path");
        }
        Ok(PathBuf::from(runtime_root))
    }

    /// Require a Docker-daemon-visible runner runtime root for hosted paths.
    fn validate_required_runner_runtime_root(&self, reason: &str) -> anyhow::Result<()> {
        let runtime_root = self
            .runner_runtime_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("AGENTICS_RUNNER_RUNTIME_ROOT must be set when {reason}")
            })?;
        if !Path::new(runtime_root).is_absolute() {
            anyhow::bail!("AGENTICS_RUNNER_RUNTIME_ROOT must be an absolute path");
        }
        Ok(())
    }

    /// Return the configured agent-registration mode.
    pub fn agent_registration_mode(&self) -> anyhow::Result<AgentRegistrationMode> {
        self.agent_registration_mode.parse()
    }

    /// Return whether local-only testing knobs such as unlimited pioneer codes may be used.
    pub fn allows_local_registration_testing_knobs(&self) -> bool {
        is_loopback_host(&self.api_host)
    }

    /// Handles runner writable slot classes mb for this module.
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

    /// Handles uses default admin credentials for this module.
    fn uses_default_admin_credentials(&self) -> bool {
        self.admin_username == DEFAULT_ADMIN_USERNAME
            && self.admin_password.expose_secret() == DEFAULT_ADMIN_PASSWORD
    }

    /// Compare a candidate admin password against the configured secret.
    pub fn admin_password_matches(&self, candidate: &str) -> bool {
        self.admin_password.expose_secret() == candidate
    }

    /// Expose the admin password for integration-test Basic auth construction.
    ///
    /// Production callers should prefer `admin_password_matches`; this accessor
    /// exists for test clients that must send the configured password over the
    /// same HTTP boundary as real clients.
    pub fn expose_admin_password_for_http_basic(&self) -> &str {
        self.admin_password.expose_secret()
    }

    /// Return whether GitHub OAuth is fully configured for creator login.
    pub fn github_oauth_enabled(&self) -> bool {
        self.github_oauth_client_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self
                .github_oauth_client_secret
                .as_ref()
                .map(ExposeSecret::expose_secret)
                .is_some_and(|value| !value.trim().is_empty())
            && self.github_oauth_redirect_url.is_some()
    }
}

/// Returns whether loopback host holds.
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.parse::<std::net::IpAddr>()
        .map(|addr| addr.is_loopback())
        .unwrap_or(false)
}

/// Validates required trimmed invariants for this contract.
fn validate_required_trimmed(value: Option<&str>, field: &str) -> anyhow::Result<()> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        anyhow::bail!("{field} must be set when GitHub OAuth is configured");
    }
    Ok(())
}

/// Validates cookie name invariants for this contract.
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
    use super::Config;
    use secrecy::SecretString;

    /// Verifies that loopback bind allows local default credentials.
    #[test]
    fn loopback_bind_allows_local_default_credentials() {
        assert!(test_config().validate_api_security().is_ok());
    }

    /// Verifies that derived debug output redacts configured secrets.
    #[test]
    fn config_debug_redacts_secrets() {
        let mut config = test_config();
        config.database_url = SecretString::from("postgres://agentics:secret@localhost/agentics");
        config.admin_password = SecretString::from("secret-admin-password");
        config.github_oauth_client_secret = Some(SecretString::from("secret-oauth-client"));

        let debug = format!("{config:?}");

        assert!(!debug.contains("secret@localhost"));
        assert!(!debug.contains("secret-admin-password"));
        assert!(!debug.contains("secret-oauth-client"));
        assert!(debug.contains("[REDACTED"));
    }

    /// Verifies that default admin credentials are rejected on wildcard bind.
    #[test]
    fn default_admin_credentials_are_rejected_on_wildcard_bind() {
        let mut config = test_config();
        config.api_host = "0.0.0.0".to_string();

        assert!(config.validate_api_security().is_err());

        config.admin_password = SecretString::from("changed");
        assert!(config.validate_api_security().is_err());

        config.agent_registration_mode = "pioneer_code".to_string();
        config.web_session_cookie_secure = true;
        assert!(config.validate_api_security().is_ok());

        config.agent_registration_mode = "public".to_string();
        assert!(config.validate_api_security().is_err());
    }

    /// Verifies that hosted API binds reject public registration mode.
    #[test]
    fn hosted_bind_rejects_public_agent_registration_mode() {
        let mut config = test_config();
        config.api_host = "0.0.0.0".to_string();
        config.admin_password = SecretString::from("changed");
        config.web_session_cookie_secure = true;
        config.agent_registration_mode = "public".to_string();

        let error = config
            .validate_api_security()
            .expect_err("public mode must stay local-only");
        assert!(
            error
                .to_string()
                .contains("AGENTICS_AGENT_REGISTRATION_MODE=public")
        );
    }

    /// Verifies invalid configured CORS origins fail startup validation.
    #[test]
    fn invalid_cors_origin_is_rejected() {
        let mut config = test_config();
        config.cors_allowed_origins = "http://localhost:3001,http://bad\nsite".to_string();

        let error = config
            .validate_api_security()
            .expect_err("invalid CORS origins should fail startup validation");

        assert!(
            error
                .to_string()
                .contains("AGENTICS_CORS_ALLOWED_ORIGINS contains invalid origin")
        );
    }

    /// Verifies that parses runner writable slot classes.
    #[test]
    fn parses_runner_writable_slot_classes() {
        let config = Config {
            runner_writable_slot_classes_mb: "1024,64 256,1024".to_string(),
            ..test_config()
        };

        assert_eq!(
            config.runner_writable_slot_classes_mb().unwrap(),
            vec![64, 256, 1024]
        );
    }

    /// Verifies invalid runner output and result limits are rejected.
    #[test]
    fn runner_output_and_result_limits_must_be_valid() {
        for (mut config, expected) in [
            (
                Config {
                    runner_max_output_files: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_OUTPUT_FILES",
            ),
            (
                Config {
                    runner_max_output_dirs: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_OUTPUT_DIRS",
            ),
            (
                Config {
                    runner_max_output_depth: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_OUTPUT_DEPTH",
            ),
            (
                Config {
                    runner_max_runs: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_RUNS",
            ),
            (
                Config {
                    runner_max_runs: 13,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_RUNS",
            ),
            (
                Config {
                    runner_max_result_json_bytes: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES",
            ),
            (
                Config {
                    runner_max_public_results: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_PUBLIC_RESULTS",
            ),
            (
                Config {
                    runner_max_result_log_bytes: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES",
            ),
            (
                Config {
                    runner_max_interaction_bytes_per_direction: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION",
            ),
            (
                Config {
                    runner_interaction_shutdown_grace_secs: 0,
                    ..test_config()
                },
                "AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS",
            ),
        ] {
            config.api_host = "127.0.0.1".to_string();
            let error = config
                .validate_runner_storage()
                .expect_err("zero limit should be rejected");
            assert!(error.to_string().contains(expected));
        }
    }

    /// Verifies that hosted workers must bound bind mounts and writable rootfs.
    #[test]
    fn hosted_runner_requires_bounded_mounts_and_layer_quota() {
        let mut config = test_config();
        config.api_host = "0.0.0.0".to_string();
        config.require_digest_pinned_images = true;

        let error = config
            .validate_runner_storage()
            .expect_err("hosted workers require a writable storage boundary");
        assert!(
            error
                .to_string()
                .contains("unbounded runner writable storage")
        );

        config.runner_docker_layer_quota = true;
        assert!(
            config.validate_runner_storage().is_err(),
            "Docker layer quota does not bound phase bind mounts"
        );

        config.runner_writable_storage_mode = "xfs-project-quota-slots".to_string();
        config.runner_docker_layer_quota = false;
        config.api_host = "127.0.0.1".to_string();
        config.runner_runtime_root = Some("/agentics-runtime".to_string());
        config.runner_phase_mount_root = Some("/agentics-runner-slots".to_string());
        let error = config
            .validate_runner_storage()
            .expect_err("quota-backed writable rootfs also needs Docker layer quota");
        assert!(error.to_string().contains("xfs-project-quota-slots"));

        config.runner_docker_layer_quota = true;
        assert_eq!(
            config.validate_runner_storage().is_ok(),
            cfg!(target_os = "linux")
        );

        config.runner_writable_storage_mode = "unbounded".to_string();
        config.runner_docker_layer_quota = false;
        config.host_probe_mode = super::HostProbeMode::Require;
        let error = config
            .validate_runner_storage()
            .expect_err("required host probes also need Docker layer quota");
        assert!(
            error
                .to_string()
                .contains("AGENTICS_HOST_PROBE_MODE=require")
        );
    }

    /// Verifies quota-backed runner storage requires a host-visible runtime root.
    #[test]
    fn quota_backed_runner_requires_runtime_root() {
        let config = Config {
            runner_writable_storage_mode: "xfs-project-quota-slots".to_string(),
            runner_docker_layer_quota: true,
            runner_phase_mount_root: Some("/agentics-runner-slots".to_string()),
            ..test_config()
        };
        let error = config
            .validate_runner_storage()
            .expect_err("quota-backed storage must require a runtime root");
        if cfg!(target_os = "linux") {
            assert!(error.to_string().contains("AGENTICS_RUNNER_RUNTIME_ROOT"));
        } else {
            assert!(error.to_string().contains("Linux-only"));
        }

        let config = Config {
            runner_writable_storage_mode: "xfs-project-quota-slots".to_string(),
            runner_docker_layer_quota: true,
            runner_runtime_root: Some("relative-runtime".to_string()),
            runner_phase_mount_root: Some("/agentics-runner-slots".to_string()),
            ..test_config()
        };
        let error = config
            .validate_runner_storage()
            .expect_err("runtime root must be absolute");
        if cfg!(target_os = "linux") {
            assert!(error.to_string().contains("absolute"));
        } else {
            assert!(error.to_string().contains("Linux-only"));
        }
    }

    /// Verifies hosted profiles cannot disable digest-pinned image enforcement.
    #[test]
    fn hosted_profiles_require_digest_pinned_images() {
        let mut probe_config = Config {
            host_probe_mode: super::HostProbeMode::Require,
            ..test_config()
        };
        let error = probe_config
            .validate_api_security()
            .expect_err("required hosted probes imply immutable images");
        assert!(
            error
                .to_string()
                .contains("AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES")
        );
        assert!(probe_config.requires_digest_pinned_images());

        probe_config.require_digest_pinned_images = true;
        assert!(probe_config.validate_api_security().is_ok());

        let local_quota_config = Config {
            runner_writable_storage_mode: "xfs-project-quota-slots".to_string(),
            ..test_config()
        };
        assert!(
            !local_quota_config.requires_digest_pinned_images(),
            "local quota-backed tests can still use local images when hosted probes are off"
        );
    }

    /// Handles test config for this module.
    fn test_config() -> Config {
        Config {
            database_url: SecretString::from(""),
            api_host: super::default_api_host(),
            api_port: super::default_api_port(),
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
            challenge_draft_validation_timeout_minutes: 30,
            challenge_private_asset_pending_timeout_minutes: 30,
            challenge_draft_publish_timeout_minutes: 30,
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
            agent_registration_mode: super::default_agent_registration_mode(),
            docker_host: None,
            host_probe_mode: super::default_host_probe_mode(),
            require_digest_pinned_images: false,
            runner_writable_storage_mode: super::default_runner_writable_storage_mode(),
            runner_runtime_root: None,
            runner_phase_mount_root: None,
            runner_writable_slot_classes_mb: super::default_runner_writable_slot_classes_mb(),
            runner_docker_layer_quota: false,
            runner_max_output_files: super::default_runner_max_output_files(),
            runner_max_output_dirs: super::default_runner_max_output_dirs(),
            runner_max_output_depth: super::default_runner_max_output_depth(),
            runner_max_runs: super::default_runner_max_runs(),
            runner_max_result_json_bytes: super::default_runner_max_result_json_bytes(),
            runner_max_public_results: super::default_runner_max_public_results(),
            runner_max_result_log_bytes: super::default_runner_max_result_log_bytes(),
            runner_max_interaction_bytes_per_direction:
                super::default_runner_max_interaction_bytes_per_direction(),
            runner_interaction_shutdown_grace_secs:
                super::default_runner_interaction_shutdown_grace_secs(),
            log_level: "info".to_string(),
        }
    }
}
