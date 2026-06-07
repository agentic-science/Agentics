//! Environment-backed runtime configuration.

use anyhow::Context as _;
use secrecy::{ExposeSecret, SecretString};
use std::path::{Path, PathBuf};

use agentics_domain::models::names::MoltbookSubmoltName;
use agentics_domain::models::urls::{
    GithubApiUserUrl, GithubAppAuthorizeUrl, GithubAppRedirectUrl, GithubAppTokenUrl,
    MoltbookSubmoltUrl,
};
use agentics_storage::{LocalStorageOptions, S3StorageOptions, StorageFactoryOptions};
pub use local_urls::{local_api_base_url, local_web_base_url};
pub use runtime_modes::{
    AgentRegistrationMode, HostProbeMode, OfficialLogRedactionMode, RunnerNamespace,
    RunnerSecurityProfile, RunnerWritableStorageMode, WorkerAccelerators,
};
pub use storage_config::{
    DEFAULT_S3_BUCKET, DEFAULT_S3_ENDPOINT_URL, DEFAULT_S3_FORCE_PATH_STYLE, DEFAULT_S3_REGION,
    DEFAULT_STORAGE_BACKEND, DEFAULT_STORAGE_ROOT, ENV_AGENTICS_S3_BUCKET,
    ENV_AGENTICS_S3_ENDPOINT_URL, ENV_AGENTICS_S3_FORCE_PATH_STYLE, ENV_AGENTICS_S3_PREFIX,
    ENV_AGENTICS_S3_REGION, ENV_AGENTICS_STORAGE_BACKEND, ENV_AGENTICS_STORAGE_ROOT,
    ENV_AGENTICS_STORAGE_WORK_ROOT, StorageBackend,
};

mod env;
mod env_policy;
mod groups;
mod local_urls;
mod runtime_modes;
mod storage_config;
mod validation;
pub use env::{
    RawApiWebEnv, RawAppEnv, RawAuthEnv, RawDatabaseEnv, RawGithubAppEnv, RawLoggingEnv,
    RawMoltbookEnv, RawQuotaEnv, RawRunnerEnv, RawStorageEnv, RawWorkerEnv,
};
pub use env_policy::{
    DeploymentStage, ENV_AGENTICS_DEPLOYMENT_STAGE, ENV_AGENTICS_REHEARSAL_ENVIRONMENT,
    ENV_AGENTICS_WEB_HOST, ENV_REVIEW_RECORD_LIMIT, ENV_RUST_LOG, ENV_STALE_REVIEW_RECORD_LIMIT,
    EnvPolicyReport, EnvPolicyWarning, EnvServiceRole, deployment_stage_from_env_map,
    known_stage_env_names, process_env_map, validate_current_env_policy, validate_env_policy,
};
pub use groups::{
    ApiWebConfig, AuthConfig, Config, DatabaseConfig, GithubAppConfig, LoggingConfig,
    MoltbookConfig, QuotaConfig, RunnerConfig, StorageConfig, WorkerConfig,
};

/// Environment variable that configures the API listen port.
pub const ENV_AGENTICS_API_PORT: &str = "AGENTICS_API_PORT";
/// Environment variable that configures the API base URL for clients and tools.
pub const ENV_AGENTICS_API_BASE_URL: &str = "AGENTICS_API_BASE_URL";
/// Environment variable that configures the web frontend base URL for checks.
pub const ENV_AGENTICS_WEB_BASE_URL: &str = "AGENTICS_WEB_BASE_URL";
/// Environment variable that configures GitHub users allowed to bootstrap the first admin.
pub const ENV_AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS: &str =
    "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS";
/// Environment variable that overrides the hosted runner profile probe command.
pub const ENV_AGENTICS_HOST_PROBE_COMMAND: &str = "AGENTICS_HOST_PROBE_COMMAND";
/// Environment variable used to derive the default local Postgres URL.
pub const ENV_AGENTICS_POSTGRES_PORT: &str = "AGENTICS_POSTGRES_PORT";
/// Environment variable used to derive the default local CORS origins.
pub const ENV_AGENTICS_WEB_PORT: &str = "AGENTICS_WEB_PORT";
/// Environment variable that separates runner containers sharing one Docker daemon.
pub const ENV_AGENTICS_RUNNER_NAMESPACE: &str = "AGENTICS_RUNNER_NAMESPACE";
/// Environment variable that configures the shared Moltbook Submolt name.
pub const ENV_AGENTICS_MOLTBOOK_SUBMOLT_NAME: &str = "AGENTICS_MOLTBOOK_SUBMOLT_NAME";
/// Environment variable that configures the shared Moltbook Submolt URL.
pub const ENV_AGENTICS_MOLTBOOK_SUBMOLT_URL: &str = "AGENTICS_MOLTBOOK_SUBMOLT_URL";
/// Environment variable that controls official-evaluation runner log redaction.
pub const ENV_AGENTICS_OFFICIAL_LOG_REDACTION: &str = "AGENTICS_OFFICIAL_LOG_REDACTION";

/// Default API listen host for local development.
pub const DEFAULT_API_HOST: &str = "127.0.0.1";
/// Default API listen port for local development.
pub const DEFAULT_API_PORT: u16 = 3100;
/// Default web listen port for local development.
pub const DEFAULT_WEB_PORT: u16 = 3001;
/// Default hosted runner profile probe command in packaged deployments.
pub const DEFAULT_HOST_PROBE_COMMAND: &str = "bin/agentics-check-dgx-spark-profile";
/// Default local Postgres port used to derive the local database URL.
pub const DEFAULT_POSTGRES_PORT: u16 = 5432;
/// Default challenge bundle root for local development.
pub const DEFAULT_CHALLENGES_ROOT: &str = "challenge-repos/agentics-challenges/challenges";
/// Default web session cookie name.
pub const DEFAULT_WEB_SESSION_COOKIE_NAME: &str = "agentics_session";
/// Default web CSRF cookie name.
pub const DEFAULT_WEB_CSRF_COOKIE_NAME: &str = "agentics_csrf";
/// Default web session lifetime in hours.
pub const DEFAULT_WEB_SESSION_TTL_HOURS: i64 = 24;
/// Default secure-cookie requirement for local development.
pub const DEFAULT_WEB_SESSION_COOKIE_SECURE: bool = false;
/// Default unauthenticated agent registration policy.
pub const DEFAULT_AGENT_REGISTRATION_MODE: AgentRegistrationMode =
    AgentRegistrationMode::PioneerCode;
/// Default worker poll interval in milliseconds.
pub const DEFAULT_WORKER_POLL_INTERVAL_MS: u64 = 3000;
/// Default stale job lease threshold in minutes.
pub const DEFAULT_WORKER_STALE_JOB_MINUTES: i32 = 1;
/// Default worker accelerator capability.
pub const DEFAULT_WORKER_ACCELERATORS: WorkerAccelerators = WorkerAccelerators::None;
/// Default validation runs allowed per agent, challenge, and day.
pub const DEFAULT_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY: u32 = 20;
/// Default official runs allowed per agent, challenge, and day.
pub const DEFAULT_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY: u32 = 5;
/// Default global active official job limit.
pub const DEFAULT_MAX_ACTIVE_OFFICIAL_JOBS: u32 = 20;
/// Default active agent limit.
pub const DEFAULT_MAX_ACTIVE_AGENTS: u32 = 1_000;
/// Default active challenge review record limit per human creator.
pub const DEFAULT_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_HUMAN: u32 = 10;
/// Default private asset byte budget per challenge review record.
pub const DEFAULT_CHALLENGE_PRIVATE_ASSET_BYTES_PER_REVIEW_RECORD: u64 = 250 * 1024 * 1024;
/// Default challenge review record validation count per day.
pub const DEFAULT_CHALLENGE_REVIEW_RECORD_VALIDATIONS_PER_DAY: u32 = 10;
/// Default challenge review record validation timeout in minutes.
pub const DEFAULT_CHALLENGE_REVIEW_RECORD_VALIDATION_TIMEOUT_MINUTES: i32 = 30;
/// Default pending private asset timeout in minutes.
pub const DEFAULT_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES: i32 = 30;
/// Default review record publish timeout in minutes.
pub const DEFAULT_CHALLENGE_REVIEW_RECORD_PUBLISH_TIMEOUT_MINUTES: i32 = 30;
/// Default unpublished challenge review record TTL in days.
pub const DEFAULT_CHALLENGE_REVIEW_RECORD_TTL_DAYS: i64 = 14;
/// Default grace period for unpublished challenge assets in days.
pub const DEFAULT_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS: i64 = 7;
/// Default worker host-probe mode.
pub const DEFAULT_HOST_PROBE_MODE: HostProbeMode = HostProbeMode::Off;
/// Default requirement for digest-pinned runner images.
pub const DEFAULT_REQUIRE_DIGEST_PINNED_IMAGES: bool = false;
const DEFAULT_MOLTBOOK_SUBMOLT_NAME: &str = "agentics-platform";
const DEFAULT_MOLTBOOK_SUBMOLT_URL: &str = "https://www.moltbook.com/m/agentics-platform";
/// Default runner security profile.
pub const DEFAULT_RUNNER_SECURITY_PROFILE: RunnerSecurityProfile =
    RunnerSecurityProfile::Development;
/// Default official-evaluation runner log redaction policy.
pub const DEFAULT_OFFICIAL_LOG_REDACTION_MODE: OfficialLogRedactionMode =
    OfficialLogRedactionMode::ContractBased;
/// Default runner writable-storage mode.
pub const DEFAULT_RUNNER_WRITABLE_STORAGE_MODE: RunnerWritableStorageMode =
    RunnerWritableStorageMode::Unbounded;
const DEFAULT_RUNNER_NAMESPACE: &str = "default";
const DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB: &str = "64,256,1024,4096";
const DEFAULT_RUNNER_MAX_OUTPUT_FILES: u64 = 8192;
const DEFAULT_RUNNER_MAX_OUTPUT_DIRS: u64 = 1024;
const DEFAULT_RUNNER_MAX_OUTPUT_DEPTH: u64 = 32;
const DEFAULT_RUNNER_MAX_RUNS: u64 =
    agentics_contracts::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION;
const DEFAULT_RUNNER_MAX_RESULT_JSON_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_RUNNER_MAX_PUBLIC_RESULTS: u64 = 1024;
const DEFAULT_RUNNER_MAX_RESULT_LOG_BYTES: u64 = 256 * 1024;
const DEFAULT_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION: u64 = 256 * 1024 * 1024;
const DEFAULT_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS: u64 = 2;
/// Default runner Docker writable-layer quota enforcement flag.
pub const DEFAULT_RUNNER_DOCKER_LAYER_QUOTA: bool = false;
/// Default runtime log level.
pub const DEFAULT_LOG_LEVEL: &str = "info";

impl Default for Config {
    /// Build local-development configuration used when env/config fields are absent.
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                url: local_database_url(DEFAULT_POSTGRES_PORT),
            },
            api_web: ApiWebConfig {
                api_host: DEFAULT_API_HOST.to_string(),
                api_port: DEFAULT_API_PORT,
                cors_allowed_origins: local_cors_allowed_origins(DEFAULT_WEB_PORT),
                web_session_cookie_name: DEFAULT_WEB_SESSION_COOKIE_NAME.to_string(),
                web_csrf_cookie_name: DEFAULT_WEB_CSRF_COOKIE_NAME.to_string(),
                web_session_ttl_hours: DEFAULT_WEB_SESSION_TTL_HOURS,
                web_session_cookie_secure: DEFAULT_WEB_SESSION_COOKIE_SECURE,
            },
            storage: StorageConfig {
                root: DEFAULT_STORAGE_ROOT.to_string(),
                backend: DEFAULT_STORAGE_BACKEND,
                work_root: None,
                s3_bucket: Some(DEFAULT_S3_BUCKET.to_string()),
                s3_prefix: None,
                s3_region: DEFAULT_S3_REGION.to_string(),
                s3_endpoint_url: Some(builtin_s3_endpoint_url()),
                s3_force_path_style: DEFAULT_S3_FORCE_PATH_STYLE,
                max_bundle_archive_bytes: storage_config::DEFAULT_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES,
                max_statement_bytes: storage_config::DEFAULT_STORAGE_MAX_STATEMENT_BYTES,
                max_json_artifact_bytes: storage_config::DEFAULT_STORAGE_MAX_JSON_ARTIFACT_BYTES,
                tmp_object_grace_hours: storage_config::DEFAULT_STORAGE_TMP_OBJECT_GRACE_HOURS,
                challenges_root: DEFAULT_CHALLENGES_ROOT.to_string(),
            },
            auth: AuthConfig {
                bootstrap_admin_github_user_ids: Vec::new(),
                agent_registration_mode: DEFAULT_AGENT_REGISTRATION_MODE,
            },
            moltbook: MoltbookConfig {
                submolt_name: builtin_moltbook_submolt_name(),
                submolt_url: builtin_moltbook_submolt_url(),
            },
            worker: WorkerConfig {
                poll_interval_ms: DEFAULT_WORKER_POLL_INTERVAL_MS,
                stale_job_minutes: DEFAULT_WORKER_STALE_JOB_MINUTES,
                accelerators: DEFAULT_WORKER_ACCELERATORS,
                gpu_probe_image: None,
            },
            quotas: QuotaConfig {
                validation_runs_per_agent_challenge_day:
                    DEFAULT_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY,
                official_runs_per_agent_challenge_day:
                    DEFAULT_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY,
                max_active_official_jobs: DEFAULT_MAX_ACTIVE_OFFICIAL_JOBS,
                max_active_agents: DEFAULT_MAX_ACTIVE_AGENTS,
                max_active_challenge_review_records_per_human:
                    DEFAULT_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_HUMAN,
                challenge_private_asset_bytes_per_review_record:
                    DEFAULT_CHALLENGE_PRIVATE_ASSET_BYTES_PER_REVIEW_RECORD,
                challenge_review_record_validations_per_day:
                    DEFAULT_CHALLENGE_REVIEW_RECORD_VALIDATIONS_PER_DAY,
                challenge_review_record_validation_timeout_minutes:
                    DEFAULT_CHALLENGE_REVIEW_RECORD_VALIDATION_TIMEOUT_MINUTES,
                challenge_private_asset_pending_timeout_minutes:
                    DEFAULT_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES,
                challenge_review_record_publish_timeout_minutes:
                    DEFAULT_CHALLENGE_REVIEW_RECORD_PUBLISH_TIMEOUT_MINUTES,
                challenge_review_record_ttl_days: DEFAULT_CHALLENGE_REVIEW_RECORD_TTL_DAYS,
                unpublished_challenge_asset_grace_days:
                    DEFAULT_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS,
            },
            github_app: GithubAppConfig {
                client_id: None,
                client_secret: None,
                redirect_url: None,
                authorize_url: builtin_github_app_authorize_url(),
                token_url: builtin_github_app_token_url(),
                api_user_url: builtin_github_api_user_url(),
            },
            runner: RunnerConfig {
                docker_host: None,
                host_probe_mode: DEFAULT_HOST_PROBE_MODE,
                host_probe_command: DEFAULT_HOST_PROBE_COMMAND.to_string(),
                security_profile: DEFAULT_RUNNER_SECURITY_PROFILE,
                official_log_redaction: DEFAULT_OFFICIAL_LOG_REDACTION_MODE,
                require_digest_pinned_images: DEFAULT_REQUIRE_DIGEST_PINNED_IMAGES,
                writable_storage_mode: DEFAULT_RUNNER_WRITABLE_STORAGE_MODE,
                namespace: builtin_runner_namespace(),
                runtime_root: None,
                phase_mount_root: None,
                writable_slot_classes_mb: DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB.to_string(),
                docker_layer_quota: DEFAULT_RUNNER_DOCKER_LAYER_QUOTA,
                max_output_files: DEFAULT_RUNNER_MAX_OUTPUT_FILES,
                max_output_dirs: DEFAULT_RUNNER_MAX_OUTPUT_DIRS,
                max_output_depth: DEFAULT_RUNNER_MAX_OUTPUT_DEPTH,
                max_runs: DEFAULT_RUNNER_MAX_RUNS,
                max_result_json_bytes: DEFAULT_RUNNER_MAX_RESULT_JSON_BYTES,
                max_public_results: DEFAULT_RUNNER_MAX_PUBLIC_RESULTS,
                max_result_log_bytes: DEFAULT_RUNNER_MAX_RESULT_LOG_BYTES,
                max_interaction_bytes_per_direction:
                    DEFAULT_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION,
                interaction_shutdown_grace_secs: DEFAULT_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS,
            },
            logging: LoggingConfig {
                log_level: DEFAULT_LOG_LEVEL.to_string(),
            },
        }
    }
}

/// Build the local database URL without exposing it through Debug output.
fn local_database_url(postgres_port: u16) -> SecretString {
    SecretString::from(format!(
        "postgres://agentics:agentics@127.0.0.1:{postgres_port}/agentics"
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

/// Build local CORS origins from the configured local web port.
fn local_cors_allowed_origins(web_port: u16) -> String {
    format!("http://127.0.0.1:{web_port},http://localhost:{web_port}")
}

#[allow(
    clippy::expect_used,
    reason = "hard-coded S3 endpoint is validated at compile-time by tests and has no runtime fallback"
)]
/// Built-in local RustFS endpoint for non-Compose S3-backed development.
fn builtin_s3_endpoint_url() -> url::Url {
    DEFAULT_S3_ENDPOINT_URL
        .parse()
        .expect("built-in S3 endpoint URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "hard-coded Moltbook Submolt name must satisfy the domain parser"
)]
/// Built-in shared Moltbook Submolt name.
fn builtin_moltbook_submolt_name() -> MoltbookSubmoltName {
    MoltbookSubmoltName::try_new(DEFAULT_MOLTBOOK_SUBMOLT_NAME.to_string())
        .expect("built-in Moltbook Submolt name must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "hard-coded Moltbook Submolt URL must satisfy the domain parser"
)]
/// Built-in shared Moltbook Submolt URL.
fn builtin_moltbook_submolt_url() -> MoltbookSubmoltUrl {
    MoltbookSubmoltUrl::try_new(DEFAULT_MOLTBOOK_SUBMOLT_URL)
        .expect("built-in Moltbook Submolt URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "static URLs are validated by type constructors and have no runtime fallback"
)]
/// Built-in GitHub sign-in authorize URL.
fn builtin_github_app_authorize_url() -> GithubAppAuthorizeUrl {
    GithubAppAuthorizeUrl::try_new("https://github.com/login/oauth/authorize")
        .expect("built-in GitHub sign-in authorize URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "static URLs are validated by type constructors and have no runtime fallback"
)]
/// Built-in GitHub sign-in token URL.
fn builtin_github_app_token_url() -> GithubAppTokenUrl {
    GithubAppTokenUrl::try_new("https://github.com/login/oauth/access_token")
        .expect("built-in GitHub sign-in token URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "static URLs are validated by type constructors and have no runtime fallback"
)]
/// Built-in GitHub API user URL.
fn builtin_github_api_user_url() -> GithubApiUserUrl {
    GithubApiUserUrl::try_new("https://api.github.com/user")
        .expect("built-in GitHub API user URL must be valid")
}

#[allow(
    clippy::expect_used,
    reason = "hard-coded runner namespace must satisfy the domain parser"
)]
/// Built-in runner namespace for non-containerized local development.
fn builtin_runner_namespace() -> RunnerNamespace {
    RunnerNamespace::try_new(DEFAULT_RUNNER_NAMESPACE)
        .expect("built-in runner namespace must be valid")
}

impl Config {
    /// Load configuration from `AGENTICS_*` environment variables with defaults.
    pub fn from_env() -> anyhow::Result<Self> {
        let raw = RawAppEnv::from_env().context("failed to load AGENTICS_* environment")?;
        Self::try_from(raw)
    }

    /// Reject settings that are acceptable for local development but dangerous
    /// when the API is reachable from another machine.
    pub fn validate_api_security(&self) -> anyhow::Result<()> {
        validation::validate_report(&self.auth)?;
        validation::validate_report(&self.api_web)?;
        validation::validate_report(&self.quotas)?;
        validation::validate_report(&self.github_app)?;
        if !local_urls::is_loopback_host(&self.api_web.api_host)
            && self.auth.agent_registration_mode == AgentRegistrationMode::Public
        {
            anyhow::bail!(
                "refusing to bind API to `{}` with AGENTICS_AGENT_REGISTRATION_MODE=public; public registration is local-development only",
                self.api_web.api_host
            );
        }

        if self.api_web.web_session_cookie_name == self.api_web.web_csrf_cookie_name {
            anyhow::bail!(
                "AGENTICS_WEB_SESSION_COOKIE_NAME and AGENTICS_WEB_CSRF_COOKIE_NAME must differ"
            );
        }
        self.validate_moltbook_config()?;
        self.validate_session_cookie_security()?;
        if self.github_app.client_id.is_some()
            || self.github_app.client_secret.is_some()
            || self.github_app.redirect_url.is_some()
        {
            validate_required_trimmed(
                self.github_app.client_id.as_deref(),
                "AGENTICS_GITHUB_APP_CLIENT_ID",
            )?;
            validate_required_trimmed(
                self.github_app
                    .client_secret
                    .as_ref()
                    .map(ExposeSecret::expose_secret),
                "AGENTICS_GITHUB_APP_CLIENT_SECRET",
            )?;
            validate_required_trimmed(
                self.github_app
                    .redirect_url
                    .as_ref()
                    .map(GithubAppRedirectUrl::as_str),
                "AGENTICS_GITHUB_APP_REDIRECT_URL",
            )?;
            self.validate_github_app_redirect_policy()?;
        }
        if (!local_urls::is_loopback_host(&self.api_web.api_host)
            || !self.auth.bootstrap_admin_github_user_ids.is_empty())
            && !self.github_app_enabled()
        {
            anyhow::bail!(
                "GitHub sign-in must be fully configured with AGENTICS_GITHUB_APP_CLIENT_ID, AGENTICS_GITHUB_APP_CLIENT_SECRET, and AGENTICS_GITHUB_APP_REDIRECT_URL before human admin login or bootstrap can work"
            );
        }
        self.validate_hosted_image_policy()?;
        self.validate_object_storage_config()?;

        Ok(())
    }

    fn validate_github_app_redirect_policy(&self) -> anyhow::Result<()> {
        let Some(redirect_url) = self.github_app.redirect_url.as_ref() else {
            return Ok(());
        };
        let url = redirect_url.to_url();
        if url.scheme() == "https" {
            return Ok(());
        }
        if url.scheme() == "http" && local_urls::is_loopback_url(&url) {
            return Ok(());
        }
        anyhow::bail!(
            "AGENTICS_GITHUB_APP_REDIRECT_URL must use HTTPS except for loopback local development callbacks"
        );
    }

    fn validate_session_cookie_security(&self) -> anyhow::Result<()> {
        if self.api_web.web_session_cookie_secure {
            return Ok(());
        }
        if let Some(redirect_url) = self.github_app.redirect_url.as_ref() {
            let url = redirect_url.to_url();
            if local_urls::is_loopback_url(&url) {
                return Ok(());
            }
        }
        if !local_urls::is_loopback_host(&self.api_web.api_host) {
            anyhow::bail!(
                "AGENTICS_WEB_SESSION_COOKIE_SECURE=false is allowed only for loopback GitHub sign-in callbacks"
            );
        }
        Ok(())
    }

    /// Validate durable object storage configuration.
    pub fn validate_object_storage_config(&self) -> anyhow::Result<()> {
        storage_config::validate_object_storage_config(self)
    }

    /// Build backend-specific durable storage options from validated runtime config.
    pub fn storage_factory_options(&self) -> anyhow::Result<StorageFactoryOptions> {
        self.validate_object_storage_config()?;
        match self.storage.backend {
            StorageBackend::Local => Ok(StorageFactoryOptions::Local(LocalStorageOptions {
                root: PathBuf::from(&self.storage.root),
            })),
            StorageBackend::S3 => {
                let bucket = self
                    .storage
                    .s3_bucket
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("AGENTICS_S3_BUCKET must be set"))?
                    .to_string();
                Ok(StorageFactoryOptions::S3(S3StorageOptions {
                    bucket,
                    prefix: self.storage.s3_prefix.clone(),
                    region: self.storage.s3_region.clone(),
                    endpoint_url: self.storage.s3_endpoint_url.clone(),
                    force_path_style: self.storage.s3_force_path_style,
                    work_root: Some(self.storage_work_root()?),
                }))
            }
        }
    }

    /// Resolve the host-local work root for storage staging and materialization.
    pub fn storage_work_root(&self) -> agentics_storage::Result<PathBuf> {
        let work_root = self
            .storage
            .work_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        agentics_storage::storage_work_root(work_root.as_deref())
    }

    /// Validate Moltbook platform-community configuration.
    fn validate_moltbook_config(&self) -> anyhow::Result<()> {
        let url_name = self.moltbook.submolt_url.submolt_name().map_err(|e| {
            anyhow::anyhow!("{} is invalid: {e}", ENV_AGENTICS_MOLTBOOK_SUBMOLT_URL)
        })?;
        if url_name != self.moltbook.submolt_name {
            anyhow::bail!(
                "{} must match the Submolt name in {}",
                ENV_AGENTICS_MOLTBOOK_SUBMOLT_NAME,
                ENV_AGENTICS_MOLTBOOK_SUBMOLT_URL
            );
        }
        Ok(())
    }

    /// Validate worker-only storage settings before claiming evaluation jobs.
    pub fn validate_runner_storage(&self) -> anyhow::Result<()> {
        self.validate_object_storage_config()?;
        self.validate_runner_output_limits()?;
        self.validate_worker_accelerator_config()?;
        self.validate_hosted_image_policy()?;

        match self.runner.writable_storage_mode {
            RunnerWritableStorageMode::Unbounded => {
                if self.runner.security_profile == RunnerSecurityProfile::Production {
                    anyhow::bail!(
                        "AGENTICS_RUNNER_SECURITY_PROFILE=production requires AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots"
                    );
                }
            }
            RunnerWritableStorageMode::XfsProjectQuotaSlots => {
                if !cfg!(target_os = "linux") {
                    anyhow::bail!(
                        "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots is Linux-only"
                    );
                }
                if !self.runner.docker_layer_quota {
                    anyhow::bail!(
                        "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true is required alongside AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots"
                    );
                }
                self.validate_required_runner_runtime_root(
                    "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots",
                )?;
                let mount_root = self
                    .runner
                    .phase_mount_root
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

        if self.runner.docker_layer_quota && !cfg!(target_os = "linux") {
            anyhow::bail!("AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true is Linux-only");
        }
        if self.runner.security_profile == RunnerSecurityProfile::Production
            && self.runner.host_probe_mode != HostProbeMode::Require
        {
            anyhow::bail!(
                "AGENTICS_RUNNER_SECURITY_PROFILE=production requires AGENTICS_HOST_PROBE_MODE=require"
            );
        }
        if self.runner.host_probe_mode == HostProbeMode::Require && !self.runner.docker_layer_quota
        {
            anyhow::bail!(
                "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true is required when AGENTICS_HOST_PROBE_MODE=require"
            );
        }
        if self.runner.host_probe_mode != HostProbeMode::Off && !cfg!(target_os = "linux") {
            anyhow::bail!(
                "AGENTICS_HOST_PROBE_MODE={} is Linux-only",
                self.runner.host_probe_mode.as_str()
            );
        }
        if self.runner.host_probe_mode != HostProbeMode::Off {
            validate_required_trimmed(
                Some(&self.runner.host_probe_command),
                ENV_AGENTICS_HOST_PROBE_COMMAND,
            )?;
            self.validate_required_runner_runtime_root("AGENTICS_HOST_PROBE_MODE is enabled")?;
        }
        if let Some(runtime_root) = self
            .runner
            .runtime_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            && !Path::new(runtime_root).is_absolute()
        {
            anyhow::bail!("AGENTICS_RUNNER_RUNTIME_ROOT must be an absolute path");
        }
        if self.runner.security_profile == RunnerSecurityProfile::Production {
            self.validate_private_host_directory(
                "AGENTICS_RUNNER_RUNTIME_ROOT",
                self.runner.runtime_root.as_deref(),
            )?;
            if self.runner.writable_storage_mode == RunnerWritableStorageMode::XfsProjectQuotaSlots
            {
                self.validate_private_host_directory(
                    "AGENTICS_RUNNER_PHASE_MOUNT_ROOT",
                    self.runner.phase_mount_root.as_deref(),
                )?;
            }
        }

        Ok(())
    }

    /// Validate worker accelerator capability knobs before accepting jobs.
    fn validate_worker_accelerator_config(&self) -> anyhow::Result<()> {
        match self.worker.accelerators {
            WorkerAccelerators::None => {
                if let Some(image) = self.worker.gpu_probe_image.as_deref()
                    && image.trim().is_empty()
                {
                    anyhow::bail!("AGENTICS_WORKER_GPU_PROBE_IMAGE must not be empty");
                }
            }
            WorkerAccelerators::Gpu => {
                if !cfg!(target_os = "linux") {
                    anyhow::bail!("AGENTICS_WORKER_ACCELERATORS=gpu is Linux-only");
                }
                self.worker_gpu_probe_image()?;
            }
        }
        Ok(())
    }

    /// Return the validated GPU probe image for GPU-capable workers.
    pub fn worker_gpu_probe_image(&self) -> anyhow::Result<&str> {
        let image = self
            .worker
            .gpu_probe_image
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "AGENTICS_WORKER_GPU_PROBE_IMAGE must be set when AGENTICS_WORKER_ACCELERATORS=gpu"
                )
            })?;
        Ok(image)
    }

    /// Return whether this configuration must enforce immutable hosted images.
    pub fn requires_digest_pinned_images(&self) -> bool {
        self.runner.require_digest_pinned_images
            || self.runner.host_probe_mode == HostProbeMode::Require
            || self.runner.security_profile == RunnerSecurityProfile::Production
    }

    /// Reject hosted profiles that try to opt out of immutable image references.
    fn validate_hosted_image_policy(&self) -> anyhow::Result<()> {
        if self.requires_digest_pinned_images() && !self.runner.require_digest_pinned_images {
            anyhow::bail!(
                "AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES must be true for profiles using AGENTICS_HOST_PROBE_MODE=require or AGENTICS_RUNNER_SECURITY_PROFILE=production"
            );
        }
        Ok(())
    }

    /// Validate platform-owned output tree limits.
    fn validate_runner_output_limits(&self) -> anyhow::Result<()> {
        validation::validate_report(&self.runner)
    }

    /// Handles runner writable storage mode for this module.
    pub fn runner_writable_storage_mode(&self) -> RunnerWritableStorageMode {
        self.runner.writable_storage_mode
    }

    /// Return the host-visible root for transient runner artifacts.
    pub fn runner_runtime_root(&self) -> anyhow::Result<PathBuf> {
        let Some(runtime_root) = self
            .runner
            .runtime_root
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
            .runner
            .runtime_root
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

    /// Validate a production runner host directory cannot be traversed by other users.
    fn validate_private_host_directory(
        &self,
        env_name: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        let path = value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("{env_name} must be set for production runners"))?;
        let path = Path::new(path);
        if !path.is_absolute() {
            anyhow::bail!("{env_name} must be an absolute path");
        }
        validate_private_host_directory_path(env_name, path)
    }

    /// Return the configured agent-registration mode.
    pub fn agent_registration_mode(&self) -> AgentRegistrationMode {
        self.auth.agent_registration_mode
    }

    /// Return whether local-only testing knobs such as unlimited pioneer codes may be used.
    pub fn allows_local_registration_testing_knobs(&self) -> bool {
        local_urls::is_loopback_host(&self.api_web.api_host)
    }

    /// Handles runner writable slot classes mb for this module.
    pub fn runner_writable_slot_classes_mb(&self) -> anyhow::Result<Vec<u64>> {
        let mut classes = Vec::new();
        for raw in self
            .runner
            .writable_slot_classes_mb
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
        self.api_web
            .cors_allowed_origins
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Return whether GitHub sign-in is fully configured for creator login.
    pub fn github_app_enabled(&self) -> bool {
        self.github_app
            .client_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self
                .github_app
                .client_secret
                .as_ref()
                .map(ExposeSecret::expose_secret)
                .is_some_and(|value| !value.trim().is_empty())
            && self.github_app.redirect_url.is_some()
    }
}

/// Validates required trimmed invariants for this contract.
pub(crate) fn validate_required_trimmed(value: Option<&str>, field: &str) -> anyhow::Result<()> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        anyhow::bail!("{field} must be set");
    }
    Ok(())
}

/// Validate a production runner directory is owned by this worker user and non-traversable.
fn validate_private_host_directory_path(env_name: &str, path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let metadata = std::fs::metadata(path)
            .with_context(|| format!("{env_name} must exist for production runners"))?;
        if !metadata.is_dir() {
            anyhow::bail!("{env_name} must point to a directory");
        }
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            anyhow::bail!("{env_name} must be mode 0700 or stricter, got {mode:o}");
        }
        let effective_uid = nix::unistd::Uid::effective().as_raw();
        if metadata.uid() != effective_uid {
            anyhow::bail!(
                "{env_name} must be owned by the worker service user uid {effective_uid}, got uid {}",
                metadata.uid()
            );
        }
    }
    #[cfg(not(unix))]
    {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("{env_name} must exist for production runners"))?;
        if !metadata.is_dir() {
            anyhow::bail!("{env_name} must point to a directory");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
