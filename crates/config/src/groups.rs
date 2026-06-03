//! Grouped runtime configuration structs.

use garde::Validate;
use secrecy::SecretString;

use agentics_domain::models::names::MoltbookSubmoltName;
use agentics_domain::models::urls::{
    GithubApiUserUrl, GithubAppAuthorizeUrl, GithubAppRedirectUrl, GithubAppTokenUrl,
    MoltbookSubmoltUrl,
};

use crate::runtime_modes::{
    AgentRegistrationMode, HostProbeMode, OfficialLogRedactionMode, RunnerNamespace,
    RunnerSecurityProfile, RunnerWritableStorageMode, WorkerAccelerators,
};
use crate::storage_config::StorageBackend;
use crate::validation;

/// Application configuration loaded from validated `AGENTICS_*` environment values.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct Config {
    #[garde(dive)]
    pub database: DatabaseConfig,
    #[garde(dive)]
    pub api_web: ApiWebConfig,
    #[garde(dive)]
    pub storage: StorageConfig,
    #[garde(dive)]
    pub auth: AuthConfig,
    #[garde(dive)]
    pub moltbook: MoltbookConfig,
    #[garde(dive)]
    pub worker: WorkerConfig,
    #[garde(dive)]
    pub quotas: QuotaConfig,
    #[garde(dive)]
    pub github_app: GithubAppConfig,
    #[garde(dive)]
    pub runner: RunnerConfig,
    #[garde(dive)]
    pub logging: LoggingConfig,
}

/// Database connection configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct DatabaseConfig {
    pub url: SecretString,
}

/// API listener and browser session configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct ApiWebConfig {
    pub api_host: String,
    pub api_port: u16,
    #[garde(custom(validation::cors_origin_list))]
    pub cors_allowed_origins: String,
    #[garde(custom(validation::cookie_name))]
    pub web_session_cookie_name: String,
    #[garde(custom(validation::cookie_name))]
    pub web_csrf_cookie_name: String,
    #[garde(range(min = 1))]
    pub web_session_ttl_hours: i64,
    pub web_session_cookie_secure: bool,
}

/// Durable object storage and challenge seed-root configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct StorageConfig {
    pub root: String,
    pub backend: StorageBackend,
    #[garde(custom(validation::optional_absolute_path))]
    pub work_root: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_prefix: Option<String>,
    pub s3_region: String,
    pub s3_endpoint_url: Option<url::Url>,
    pub s3_force_path_style: bool,
    #[garde(range(min = 1))]
    pub max_bundle_archive_bytes: u64,
    #[garde(range(min = 1))]
    pub max_statement_bytes: u64,
    #[garde(range(min = 1))]
    pub max_json_artifact_bytes: u64,
    #[garde(range(min = 1))]
    pub tmp_object_grace_hours: u64,
    pub challenges_root: String,
}

/// Human bootstrap and agent-registration configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct AuthConfig {
    pub bootstrap_admin_github_user_ids: Vec<i64>,
    pub agent_registration_mode: AgentRegistrationMode,
}

/// Platform Moltbook community configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct MoltbookConfig {
    pub submolt_name: MoltbookSubmoltName,
    pub submolt_url: MoltbookSubmoltUrl,
}

/// Worker polling and accelerator capability configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct WorkerConfig {
    #[garde(range(min = 1))]
    pub poll_interval_ms: u64,
    #[garde(range(min = 1))]
    pub stale_job_minutes: i32,
    pub accelerators: WorkerAccelerators,
    #[garde(custom(validation::optional_trimmed_non_empty))]
    pub gpu_probe_image: Option<String>,
}

/// Platform quota and lifecycle configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct QuotaConfig {
    #[garde(range(min = 1))]
    pub validation_runs_per_agent_challenge_day: u32,
    #[garde(range(min = 1))]
    pub official_runs_per_agent_challenge_day: u32,
    #[garde(range(min = 1))]
    pub max_active_official_jobs: u32,
    #[garde(range(min = 1))]
    pub max_active_agents: u32,
    #[garde(range(min = 1))]
    pub max_active_challenge_review_records_per_human: u32,
    #[garde(range(min = 1))]
    pub challenge_private_asset_bytes_per_review_record: u64,
    #[garde(range(min = 1))]
    pub challenge_review_record_validations_per_day: u32,
    #[garde(range(min = 1))]
    pub challenge_review_record_validation_timeout_minutes: i32,
    #[garde(range(min = 1))]
    pub challenge_private_asset_pending_timeout_minutes: i32,
    #[garde(range(min = 1))]
    pub challenge_review_record_publish_timeout_minutes: i32,
    #[garde(range(min = 1))]
    pub challenge_review_record_ttl_days: i64,
    #[garde(range(min = 1))]
    pub unpublished_challenge_asset_grace_days: i64,
}

/// GitHub sign-in configuration for challenge creators.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct GithubAppConfig {
    #[garde(custom(validation::optional_trimmed_non_empty))]
    pub client_id: Option<String>,
    #[garde(custom(validation::optional_secret_non_empty))]
    pub client_secret: Option<SecretString>,
    pub redirect_url: Option<GithubAppRedirectUrl>,
    pub authorize_url: GithubAppAuthorizeUrl,
    pub token_url: GithubAppTokenUrl,
    pub api_user_url: GithubApiUserUrl,
}

/// Docker runner and execution safety configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct RunnerConfig {
    /// Optional Docker host URI used by CI or remote Docker setups.
    pub docker_host: Option<String>,
    pub host_probe_mode: HostProbeMode,
    #[garde(custom(validation::trimmed_non_empty))]
    pub host_probe_command: String,
    pub security_profile: RunnerSecurityProfile,
    pub official_log_redaction: OfficialLogRedactionMode,
    pub require_digest_pinned_images: bool,
    pub writable_storage_mode: RunnerWritableStorageMode,
    pub namespace: RunnerNamespace,
    #[garde(custom(validation::optional_absolute_path))]
    pub runtime_root: Option<String>,
    #[garde(custom(validation::optional_absolute_path))]
    pub phase_mount_root: Option<String>,
    #[garde(custom(validation::runner_slot_class_csv))]
    pub writable_slot_classes_mb: String,
    pub docker_layer_quota: bool,
    #[garde(range(min = 1))]
    pub max_output_files: u64,
    #[garde(range(min = 1))]
    pub max_output_dirs: u64,
    #[garde(range(min = 1))]
    pub max_output_depth: u64,
    #[garde(range(min = 1, max = agentics_contracts::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION))]
    pub max_runs: u64,
    #[garde(range(min = 1))]
    pub max_result_json_bytes: u64,
    #[garde(range(min = 1))]
    pub max_public_results: u64,
    #[garde(range(min = 1))]
    pub max_result_log_bytes: u64,
    #[garde(range(min = 1))]
    pub max_interaction_bytes_per_direction: u64,
    #[garde(range(min = 1))]
    pub interaction_shutdown_grace_secs: u64,
}

/// Runtime logging configuration.
#[derive(Debug, Clone, Validate)]
#[garde(allow_unvalidated)]
pub struct LoggingConfig {
    #[garde(custom(validation::trimmed_non_empty))]
    pub log_level: String,
}
