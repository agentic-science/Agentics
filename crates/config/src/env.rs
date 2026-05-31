//! Raw `AGENTICS_*` environment loading and validation.

use super::{
    AgentRegistrationMode, Config, DEFAULT_ADMIN_USERNAME, DEFAULT_AGENT_REGISTRATION_MODE,
    DEFAULT_ALLOW_INSECURE_DEFAULT_ADMIN_CREDENTIALS, DEFAULT_API_HOST, DEFAULT_API_PORT,
    DEFAULT_CHALLENGE_DRAFT_PUBLISH_TIMEOUT_MINUTES, DEFAULT_CHALLENGE_DRAFT_TTL_DAYS,
    DEFAULT_CHALLENGE_DRAFT_VALIDATION_TIMEOUT_MINUTES,
    DEFAULT_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY, DEFAULT_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT,
    DEFAULT_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES, DEFAULT_HOST_PROBE_COMMAND,
    DEFAULT_HOST_PROBE_MODE, DEFAULT_LOG_LEVEL, DEFAULT_MAX_ACTIVE_AGENTS,
    DEFAULT_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT, DEFAULT_MAX_ACTIVE_OFFICIAL_JOBS,
    DEFAULT_OFFICIAL_LOG_REDACTION_MODE, DEFAULT_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY,
    DEFAULT_POSTGRES_PORT, DEFAULT_REQUIRE_DIGEST_PINNED_IMAGES, DEFAULT_RUNNER_DOCKER_LAYER_QUOTA,
    DEFAULT_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS,
    DEFAULT_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION, DEFAULT_RUNNER_MAX_OUTPUT_DEPTH,
    DEFAULT_RUNNER_MAX_OUTPUT_DIRS, DEFAULT_RUNNER_MAX_OUTPUT_FILES,
    DEFAULT_RUNNER_MAX_PUBLIC_RESULTS, DEFAULT_RUNNER_MAX_RESULT_JSON_BYTES,
    DEFAULT_RUNNER_MAX_RESULT_LOG_BYTES, DEFAULT_RUNNER_MAX_RUNS, DEFAULT_RUNNER_SECURITY_PROFILE,
    DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB, DEFAULT_RUNNER_WRITABLE_STORAGE_MODE,
    DEFAULT_S3_BUCKET, DEFAULT_S3_FORCE_PATH_STYLE, DEFAULT_S3_REGION, DEFAULT_STORAGE_BACKEND,
    DEFAULT_STORAGE_ROOT, DEFAULT_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS,
    DEFAULT_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY, DEFAULT_WEB_CSRF_COOKIE_NAME,
    DEFAULT_WEB_PORT, DEFAULT_WEB_SESSION_COOKIE_NAME, DEFAULT_WEB_SESSION_COOKIE_SECURE,
    DEFAULT_WEB_SESSION_TTL_HOURS, DEFAULT_WORKER_ACCELERATORS, DEFAULT_WORKER_POLL_INTERVAL_MS,
    DEFAULT_WORKER_STALE_JOB_MINUTES, ENV_AGENTICS_ADMIN_PASSWORD, ENV_AGENTICS_ADMIN_USERNAME,
    ENV_AGENTICS_HOST_PROBE_COMMAND, ENV_AGENTICS_MOLTBOOK_SUBMOLT_NAME,
    ENV_AGENTICS_MOLTBOOK_SUBMOLT_URL, ENV_AGENTICS_RUNNER_NAMESPACE, ENV_AGENTICS_S3_ENDPOINT_URL,
    ENV_AGENTICS_S3_REGION, ENV_AGENTICS_STORAGE_ROOT, GithubApiUserUrl, GithubOauthAuthorizeUrl,
    GithubOauthRedirectUrl, GithubOauthTokenUrl, HostProbeMode, INSECURE_DEFAULT_ADMIN_PASSWORD,
    MoltbookSubmoltName, MoltbookSubmoltUrl, OfficialLogRedactionMode, RunnerNamespace,
    RunnerSecurityProfile, RunnerWritableStorageMode, StorageBackend, WorkerAccelerators,
    builtin_github_api_user_url, builtin_github_oauth_authorize_url,
    builtin_github_oauth_token_url, builtin_moltbook_submolt_name, builtin_moltbook_submolt_url,
    builtin_runner_namespace, builtin_s3_endpoint_url, local_cors_allowed_origins,
    local_database_url, storage_config,
};
use secrecy::SecretString;
use serde::{Deserialize, de::DeserializeOwned};

const ENV_PREFIX: &str = "AGENTICS_";

/// Raw application environment grouped by runtime concern.
#[derive(Debug, Clone, Default)]
pub struct RawAppEnv {
    pub database: RawDatabaseEnv,
    pub api_web: RawApiWebEnv,
    pub storage: RawStorageEnv,
    pub auth: RawAuthEnv,
    pub moltbook: RawMoltbookEnv,
    pub worker: RawWorkerEnv,
    pub quotas: RawQuotaEnv,
    pub oauth: RawGithubOauthEnv,
    pub runner: RawRunnerEnv,
    pub logging: RawLoggingEnv,
}

impl RawAppEnv {
    /// Load grouped raw env structs from the current process.
    pub fn from_env() -> envy::Result<Self> {
        Self::from_env_iter(std::env::vars())
    }

    /// Load grouped raw env structs from one prefixed env snapshot.
    pub fn from_env_iter<Iter>(iter: Iter) -> envy::Result<Self>
    where
        Iter: IntoIterator<Item = (String, String)>,
    {
        let vars: Vec<_> = iter.into_iter().collect();
        Ok(Self {
            database: load_group(&vars)?,
            api_web: load_group(&vars)?,
            storage: load_group(&vars)?,
            auth: load_group(&vars)?,
            moltbook: load_group(&vars)?,
            worker: load_group(&vars)?,
            quotas: load_group(&vars)?,
            oauth: load_group(&vars)?,
            runner: load_group(&vars)?,
            logging: load_group(&vars)?,
        })
    }
}

fn load_group<T>(vars: &[(String, String)]) -> envy::Result<T>
where
    T: DeserializeOwned,
{
    envy::prefixed(ENV_PREFIX).from_iter(vars.iter().cloned())
}

/// Raw database environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawDatabaseEnv {
    pub database_url: Option<String>,
    pub postgres_port: Option<u16>,
}

/// Raw API and web environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawApiWebEnv {
    pub api_host: Option<String>,
    pub api_port: Option<u16>,
    pub web_port: Option<u16>,
    pub cors_allowed_origins: Option<String>,
    pub web_session_cookie_name: Option<String>,
    pub web_csrf_cookie_name: Option<String>,
    pub web_session_ttl_hours: Option<i64>,
    pub web_session_cookie_secure: Option<bool>,
}

/// Raw durable storage environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawStorageEnv {
    pub storage_root: Option<String>,
    pub storage_backend: Option<StorageBackend>,
    pub storage_work_root: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_prefix: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint_url: Option<String>,
    pub s3_force_path_style: Option<bool>,
    pub challenges_root: Option<String>,
    pub storage_max_bundle_archive_bytes: Option<u64>,
    pub storage_max_statement_bytes: Option<u64>,
    pub storage_max_json_artifact_bytes: Option<u64>,
    pub storage_tmp_object_grace_hours: Option<u64>,
}

/// Raw administrator and registration environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawAuthEnv {
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
    pub allow_insecure_default_admin_credentials: Option<bool>,
    pub agent_registration_mode: Option<AgentRegistrationMode>,
}

/// Raw Moltbook environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawMoltbookEnv {
    pub moltbook_submolt_name: Option<String>,
    pub moltbook_submolt_url: Option<String>,
}

/// Raw worker environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawWorkerEnv {
    pub worker_poll_interval_ms: Option<u64>,
    pub worker_stale_job_minutes: Option<i32>,
    pub worker_accelerators: Option<WorkerAccelerators>,
    pub worker_gpu_probe_image: Option<String>,
}

/// Raw platform quota and lifecycle environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawQuotaEnv {
    pub validation_runs_per_agent_challenge_day: Option<u32>,
    pub official_runs_per_agent_challenge_day: Option<u32>,
    pub max_active_official_jobs: Option<u32>,
    pub max_active_agents: Option<u32>,
    pub max_active_challenge_drafts_per_agent: Option<u32>,
    pub challenge_private_asset_bytes_per_draft: Option<u64>,
    pub challenge_draft_validations_per_day: Option<u32>,
    pub challenge_draft_validation_timeout_minutes: Option<i32>,
    pub challenge_private_asset_pending_timeout_minutes: Option<i32>,
    pub challenge_draft_publish_timeout_minutes: Option<i32>,
    pub challenge_draft_ttl_days: Option<i64>,
    pub unpublished_challenge_asset_grace_days: Option<i64>,
}

/// Raw GitHub OAuth environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawGithubOauthEnv {
    pub github_oauth_client_id: Option<String>,
    pub github_oauth_client_secret: Option<String>,
    pub github_oauth_redirect_url: Option<String>,
    pub github_oauth_authorize_url: Option<String>,
    pub github_oauth_token_url: Option<String>,
    pub github_api_user_url: Option<String>,
}

/// Raw runner environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawRunnerEnv {
    pub docker_host: Option<String>,
    pub host_probe_mode: Option<HostProbeMode>,
    pub host_probe_command: Option<String>,
    pub runner_security_profile: Option<RunnerSecurityProfile>,
    pub official_log_redaction: Option<OfficialLogRedactionMode>,
    pub require_digest_pinned_images: Option<bool>,
    pub runner_writable_storage_mode: Option<RunnerWritableStorageMode>,
    pub runner_namespace: Option<String>,
    pub runner_runtime_root: Option<String>,
    pub runner_phase_mount_root: Option<String>,
    pub runner_writable_slot_classes_mb: Option<String>,
    pub runner_docker_layer_quota: Option<bool>,
    pub runner_max_output_files: Option<u64>,
    pub runner_max_output_dirs: Option<u64>,
    pub runner_max_output_depth: Option<u64>,
    pub runner_max_runs: Option<u64>,
    pub runner_max_result_json_bytes: Option<u64>,
    pub runner_max_public_results: Option<u64>,
    pub runner_max_result_log_bytes: Option<u64>,
    pub runner_max_interaction_bytes_per_direction: Option<u64>,
    pub runner_interaction_shutdown_grace_secs: Option<u64>,
}

/// Raw logging environment values.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawLoggingEnv {
    pub log_level: Option<String>,
}

impl TryFrom<RawAppEnv> for Config {
    type Error = anyhow::Error;

    /// Convert raw environment strings into typed runtime configuration.
    fn try_from(raw: RawAppEnv) -> anyhow::Result<Self> {
        let mut config = Self::default();
        let postgres_port = raw.database.postgres_port.unwrap_or(DEFAULT_POSTGRES_PORT);
        let web_port = raw.api_web.web_port.unwrap_or(DEFAULT_WEB_PORT);

        config.database.url = match raw.database.database_url {
            Some(value) => {
                SecretString::from(required_trimmed_string("AGENTICS_DATABASE_URL", value)?)
            }
            None => local_database_url(postgres_port),
        };
        config.api_web.api_host =
            string_or_default("AGENTICS_API_HOST", raw.api_web.api_host, DEFAULT_API_HOST)?;
        config.api_web.api_port = raw.api_web.api_port.unwrap_or(DEFAULT_API_PORT);
        config.api_web.cors_allowed_origins = match raw.api_web.cors_allowed_origins {
            Some(value) => required_trimmed_string("AGENTICS_CORS_ALLOWED_ORIGINS", value)?,
            None => local_cors_allowed_origins(web_port),
        };
        config.api_web.web_session_cookie_name = string_or_default(
            "AGENTICS_WEB_SESSION_COOKIE_NAME",
            raw.api_web.web_session_cookie_name,
            DEFAULT_WEB_SESSION_COOKIE_NAME,
        )?;
        config.api_web.web_csrf_cookie_name = string_or_default(
            "AGENTICS_WEB_CSRF_COOKIE_NAME",
            raw.api_web.web_csrf_cookie_name,
            DEFAULT_WEB_CSRF_COOKIE_NAME,
        )?;
        config.api_web.web_session_ttl_hours = raw
            .api_web
            .web_session_ttl_hours
            .unwrap_or(DEFAULT_WEB_SESSION_TTL_HOURS);
        config.api_web.web_session_cookie_secure = raw
            .api_web
            .web_session_cookie_secure
            .unwrap_or(DEFAULT_WEB_SESSION_COOKIE_SECURE);

        apply_storage_env(&mut config, raw.storage)?;
        apply_auth_env(&mut config, raw.auth)?;
        apply_moltbook_env(&mut config, raw.moltbook)?;
        apply_worker_env(&mut config, raw.worker)?;
        apply_quota_env(&mut config, raw.quotas)?;
        apply_oauth_env(&mut config, raw.oauth)?;
        apply_runner_env(&mut config, raw.runner)?;
        config.logging.log_level = string_or_default(
            "AGENTICS_LOG_LEVEL",
            raw.logging.log_level,
            DEFAULT_LOG_LEVEL,
        )?;

        Ok(config)
    }
}

fn apply_storage_env(config: &mut Config, raw: RawStorageEnv) -> anyhow::Result<()> {
    config.storage.root = string_or_default(
        ENV_AGENTICS_STORAGE_ROOT,
        raw.storage_root,
        DEFAULT_STORAGE_ROOT,
    )?;
    config.storage.backend = raw.storage_backend.unwrap_or(DEFAULT_STORAGE_BACKEND);
    config.storage.work_root = optional_non_empty_string(raw.storage_work_root);
    config.storage.s3_bucket = raw
        .s3_bucket
        .map(trimmed_string)
        .or_else(|| Some(DEFAULT_S3_BUCKET.to_string()));
    config.storage.s3_prefix = optional_non_empty_string(raw.s3_prefix);
    config.storage.s3_region =
        string_or_default(ENV_AGENTICS_S3_REGION, raw.s3_region, DEFAULT_S3_REGION)?;
    config.storage.s3_endpoint_url = match raw.s3_endpoint_url {
        Some(value) => Some(parse_url_env(ENV_AGENTICS_S3_ENDPOINT_URL, value)?),
        None => Some(builtin_s3_endpoint_url()),
    };
    config.storage.s3_force_path_style = raw
        .s3_force_path_style
        .unwrap_or(DEFAULT_S3_FORCE_PATH_STYLE);
    if let Some(value) = raw.challenges_root {
        config.storage.challenges_root =
            required_trimmed_string("AGENTICS_CHALLENGES_ROOT", value)?;
    }
    config.storage.max_bundle_archive_bytes = raw
        .storage_max_bundle_archive_bytes
        .unwrap_or(storage_config::DEFAULT_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES);
    config.storage.max_statement_bytes = raw
        .storage_max_statement_bytes
        .unwrap_or(storage_config::DEFAULT_STORAGE_MAX_STATEMENT_BYTES);
    config.storage.max_json_artifact_bytes = raw
        .storage_max_json_artifact_bytes
        .unwrap_or(storage_config::DEFAULT_STORAGE_MAX_JSON_ARTIFACT_BYTES);
    config.storage.tmp_object_grace_hours = raw
        .storage_tmp_object_grace_hours
        .unwrap_or(storage_config::DEFAULT_STORAGE_TMP_OBJECT_GRACE_HOURS);
    Ok(())
}

fn apply_auth_env(config: &mut Config, raw: RawAuthEnv) -> anyhow::Result<()> {
    config.auth.admin_username = string_or_default(
        ENV_AGENTICS_ADMIN_USERNAME,
        raw.admin_username,
        DEFAULT_ADMIN_USERNAME,
    )?;
    config.auth.admin_password = match raw.admin_password {
        Some(value) => {
            SecretString::from(required_secret_string(ENV_AGENTICS_ADMIN_PASSWORD, value)?)
        }
        None => SecretString::from(INSECURE_DEFAULT_ADMIN_PASSWORD),
    };
    config.auth.allow_insecure_default_admin_credentials = raw
        .allow_insecure_default_admin_credentials
        .unwrap_or(DEFAULT_ALLOW_INSECURE_DEFAULT_ADMIN_CREDENTIALS);
    config.auth.agent_registration_mode = raw
        .agent_registration_mode
        .unwrap_or(DEFAULT_AGENT_REGISTRATION_MODE);
    Ok(())
}

fn apply_moltbook_env(config: &mut Config, raw: RawMoltbookEnv) -> anyhow::Result<()> {
    config.moltbook.submolt_name = match raw.moltbook_submolt_name {
        Some(value) => MoltbookSubmoltName::try_new(required_trimmed_string(
            ENV_AGENTICS_MOLTBOOK_SUBMOLT_NAME,
            value,
        )?)?,
        None => builtin_moltbook_submolt_name(),
    };
    config.moltbook.submolt_url = match raw.moltbook_submolt_url {
        Some(value) => MoltbookSubmoltUrl::try_new(required_trimmed_string(
            ENV_AGENTICS_MOLTBOOK_SUBMOLT_URL,
            value,
        )?)?,
        None => builtin_moltbook_submolt_url(),
    };
    Ok(())
}

fn apply_worker_env(config: &mut Config, raw: RawWorkerEnv) -> anyhow::Result<()> {
    config.worker.poll_interval_ms = raw
        .worker_poll_interval_ms
        .unwrap_or(DEFAULT_WORKER_POLL_INTERVAL_MS);
    config.worker.stale_job_minutes = raw
        .worker_stale_job_minutes
        .unwrap_or(DEFAULT_WORKER_STALE_JOB_MINUTES);
    config.worker.accelerators = raw
        .worker_accelerators
        .unwrap_or(DEFAULT_WORKER_ACCELERATORS);
    config.worker.gpu_probe_image = optional_non_empty_string(raw.worker_gpu_probe_image);
    Ok(())
}

fn apply_quota_env(config: &mut Config, raw: RawQuotaEnv) -> anyhow::Result<()> {
    config.quotas.validation_runs_per_agent_challenge_day = raw
        .validation_runs_per_agent_challenge_day
        .unwrap_or(DEFAULT_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY);
    config.quotas.official_runs_per_agent_challenge_day = raw
        .official_runs_per_agent_challenge_day
        .unwrap_or(DEFAULT_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY);
    config.quotas.max_active_official_jobs = raw
        .max_active_official_jobs
        .unwrap_or(DEFAULT_MAX_ACTIVE_OFFICIAL_JOBS);
    config.quotas.max_active_agents = raw.max_active_agents.unwrap_or(DEFAULT_MAX_ACTIVE_AGENTS);
    config.quotas.max_active_challenge_drafts_per_agent = raw
        .max_active_challenge_drafts_per_agent
        .unwrap_or(DEFAULT_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT);
    config.quotas.challenge_private_asset_bytes_per_draft = raw
        .challenge_private_asset_bytes_per_draft
        .unwrap_or(DEFAULT_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT);
    config.quotas.challenge_draft_validations_per_day = raw
        .challenge_draft_validations_per_day
        .unwrap_or(DEFAULT_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY);
    config.quotas.challenge_draft_validation_timeout_minutes = raw
        .challenge_draft_validation_timeout_minutes
        .unwrap_or(DEFAULT_CHALLENGE_DRAFT_VALIDATION_TIMEOUT_MINUTES);
    config
        .quotas
        .challenge_private_asset_pending_timeout_minutes = raw
        .challenge_private_asset_pending_timeout_minutes
        .unwrap_or(DEFAULT_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES);
    config.quotas.challenge_draft_publish_timeout_minutes = raw
        .challenge_draft_publish_timeout_minutes
        .unwrap_or(DEFAULT_CHALLENGE_DRAFT_PUBLISH_TIMEOUT_MINUTES);
    config.quotas.challenge_draft_ttl_days = raw
        .challenge_draft_ttl_days
        .unwrap_or(DEFAULT_CHALLENGE_DRAFT_TTL_DAYS);
    config.quotas.unpublished_challenge_asset_grace_days = raw
        .unpublished_challenge_asset_grace_days
        .unwrap_or(DEFAULT_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS);
    Ok(())
}

fn apply_oauth_env(config: &mut Config, raw: RawGithubOauthEnv) -> anyhow::Result<()> {
    config.github_oauth.client_id = optional_non_empty_string(raw.github_oauth_client_id);
    config.github_oauth.client_secret =
        optional_non_empty_string(raw.github_oauth_client_secret).map(SecretString::from);
    config.github_oauth.redirect_url = raw
        .github_oauth_redirect_url
        .map(|value| -> anyhow::Result<GithubOauthRedirectUrl> {
            let value = required_trimmed_string("AGENTICS_GITHUB_OAUTH_REDIRECT_URL", value)?;
            Ok(GithubOauthRedirectUrl::try_new(value)?)
        })
        .transpose()?;
    config.github_oauth.authorize_url = match raw.github_oauth_authorize_url {
        Some(value) => GithubOauthAuthorizeUrl::try_new(required_trimmed_string(
            "AGENTICS_GITHUB_OAUTH_AUTHORIZE_URL",
            value,
        )?)?,
        None => builtin_github_oauth_authorize_url(),
    };
    config.github_oauth.token_url = match raw.github_oauth_token_url {
        Some(value) => GithubOauthTokenUrl::try_new(required_trimmed_string(
            "AGENTICS_GITHUB_OAUTH_TOKEN_URL",
            value,
        )?)?,
        None => builtin_github_oauth_token_url(),
    };
    config.github_oauth.api_user_url = match raw.github_api_user_url {
        Some(value) => GithubApiUserUrl::try_new(required_trimmed_string(
            "AGENTICS_GITHUB_API_USER_URL",
            value,
        )?)?,
        None => builtin_github_api_user_url(),
    };
    Ok(())
}

fn apply_runner_env(config: &mut Config, raw: RawRunnerEnv) -> anyhow::Result<()> {
    config.runner.docker_host = optional_non_empty_string(raw.docker_host);
    config.runner.host_probe_mode = raw.host_probe_mode.unwrap_or(DEFAULT_HOST_PROBE_MODE);
    config.runner.host_probe_command = string_or_default(
        ENV_AGENTICS_HOST_PROBE_COMMAND,
        raw.host_probe_command,
        DEFAULT_HOST_PROBE_COMMAND,
    )?;
    config.runner.security_profile = raw
        .runner_security_profile
        .unwrap_or(DEFAULT_RUNNER_SECURITY_PROFILE);
    config.runner.official_log_redaction = raw
        .official_log_redaction
        .unwrap_or(DEFAULT_OFFICIAL_LOG_REDACTION_MODE);
    config.runner.require_digest_pinned_images = raw
        .require_digest_pinned_images
        .unwrap_or(DEFAULT_REQUIRE_DIGEST_PINNED_IMAGES);
    config.runner.writable_storage_mode = raw
        .runner_writable_storage_mode
        .unwrap_or(DEFAULT_RUNNER_WRITABLE_STORAGE_MODE);
    config.runner.namespace = match raw.runner_namespace {
        Some(value) => RunnerNamespace::try_new(required_trimmed_string(
            ENV_AGENTICS_RUNNER_NAMESPACE,
            value,
        )?)?,
        None => builtin_runner_namespace(),
    };
    config.runner.runtime_root = optional_non_empty_string(raw.runner_runtime_root);
    config.runner.phase_mount_root = optional_non_empty_string(raw.runner_phase_mount_root);
    config.runner.writable_slot_classes_mb = string_or_default(
        "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB",
        raw.runner_writable_slot_classes_mb,
        DEFAULT_RUNNER_WRITABLE_SLOT_CLASSES_MB,
    )?;
    config.runner.docker_layer_quota = raw
        .runner_docker_layer_quota
        .unwrap_or(DEFAULT_RUNNER_DOCKER_LAYER_QUOTA);
    config.runner.max_output_files = raw
        .runner_max_output_files
        .unwrap_or(DEFAULT_RUNNER_MAX_OUTPUT_FILES);
    config.runner.max_output_dirs = raw
        .runner_max_output_dirs
        .unwrap_or(DEFAULT_RUNNER_MAX_OUTPUT_DIRS);
    config.runner.max_output_depth = raw
        .runner_max_output_depth
        .unwrap_or(DEFAULT_RUNNER_MAX_OUTPUT_DEPTH);
    config.runner.max_runs = raw.runner_max_runs.unwrap_or(DEFAULT_RUNNER_MAX_RUNS);
    config.runner.max_result_json_bytes = raw
        .runner_max_result_json_bytes
        .unwrap_or(DEFAULT_RUNNER_MAX_RESULT_JSON_BYTES);
    config.runner.max_public_results = raw
        .runner_max_public_results
        .unwrap_or(DEFAULT_RUNNER_MAX_PUBLIC_RESULTS);
    config.runner.max_result_log_bytes = raw
        .runner_max_result_log_bytes
        .unwrap_or(DEFAULT_RUNNER_MAX_RESULT_LOG_BYTES);
    config.runner.max_interaction_bytes_per_direction = raw
        .runner_max_interaction_bytes_per_direction
        .unwrap_or(DEFAULT_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION);
    config.runner.interaction_shutdown_grace_secs = raw
        .runner_interaction_shutdown_grace_secs
        .unwrap_or(DEFAULT_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS);
    Ok(())
}

fn string_or_default(
    field: &'static str,
    value: Option<String>,
    default: &str,
) -> anyhow::Result<String> {
    match value {
        Some(value) => required_trimmed_string(field, value),
        None => Ok(default.to_string()),
    }
}

fn optional_non_empty_string(value: Option<String>) -> Option<String> {
    value.map(trimmed_string).filter(|value| !value.is_empty())
}

fn trimmed_string(value: String) -> String {
    value.trim().to_string()
}

fn required_trimmed_string(field: &'static str, value: String) -> anyhow::Result<String> {
    let trimmed = trimmed_string(value);
    if trimmed.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(trimmed)
}

fn required_secret_string(field: &'static str, value: String) -> anyhow::Result<String> {
    if value.trim().is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(value)
}

fn parse_url_env(field: &'static str, value: String) -> anyhow::Result<url::Url> {
    let trimmed = required_trimmed_string(field, value)?;
    trimmed
        .parse::<url::Url>()
        .map_err(|error| anyhow::anyhow!("invalid {field} value `{trimmed}`: {error}"))
}
