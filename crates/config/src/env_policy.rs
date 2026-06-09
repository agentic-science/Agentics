//! Stage-aware environment policy checks.
//!
//! `Config::from_env` keeps the typed runtime defaults, while this module owns
//! launch-time policy: every stage env var must either be required, optional
//! with a documented default, deprecated, ignored, or explicitly external.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::str::FromStr;

pub const ENV_AGENTICS_DEPLOYMENT_STAGE: &str = "AGENTICS_DEPLOYMENT_STAGE";
pub const ENV_AGENTICS_REHEARSAL_ENVIRONMENT: &str = "AGENTICS_REHEARSAL_ENVIRONMENT";
pub const ENV_STALE_REVIEW_RECORD_LIMIT: &str =
    "AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_AGENT";
pub const ENV_REVIEW_RECORD_LIMIT: &str = "AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_HUMAN";
pub const ENV_AGENTICS_WEB_HOST: &str = "AGENTICS_WEB_HOST";
pub const ENV_RUST_LOG: &str = "RUST_LOG";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentStage {
    Dev,
    Test,
    Rehearsal,
    Production,
}

impl DeploymentStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Test => "test",
            Self::Rehearsal => "rehearsal",
            Self::Production => "production",
        }
    }

    fn rejects_placeholders(self) -> bool {
        matches!(self, Self::Rehearsal | Self::Production)
    }
}

impl fmt::Display for DeploymentStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DeploymentStage {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "dev" => Ok(Self::Dev),
            "test" => Ok(Self::Test),
            "rehearsal" => Ok(Self::Rehearsal),
            "production" => Ok(Self::Production),
            other => anyhow::bail!(
                "{ENV_AGENTICS_DEPLOYMENT_STAGE} must be one of dev, test, rehearsal, or production; got `{other}`"
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvServiceRole {
    Compose,
    Api,
    Worker,
    Migrate,
    Web,
    LocalDev,
    TestHarness,
}

impl EnvServiceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compose => "compose",
            Self::Api => "api",
            Self::Worker => "worker",
            Self::Migrate => "migrate",
            Self::Web => "web",
            Self::LocalDev => "local-dev",
            Self::TestHarness => "test-harness",
        }
    }
}

impl fmt::Display for EnvServiceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EnvServiceRole {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "compose" => Ok(Self::Compose),
            "api" => Ok(Self::Api),
            "worker" => Ok(Self::Worker),
            "migrate" => Ok(Self::Migrate),
            "web" => Ok(Self::Web),
            "local-dev" => Ok(Self::LocalDev),
            "test-harness" => Ok(Self::TestHarness),
            other => anyhow::bail!(
                "env policy role must be one of compose, api, worker, migrate, web, local-dev, or test-harness; got `{other}`"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPolicyWarning {
    pub name: &'static str,
    pub message: String,
}

impl fmt::Display for EnvPolicyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPolicyReport {
    pub stage: DeploymentStage,
    pub role: EnvServiceRole,
    pub warnings: Vec<EnvPolicyWarning>,
}

#[derive(Debug, Clone, Copy)]
struct OptionalEnv {
    name: &'static str,
    default: &'static str,
}

impl OptionalEnv {
    const fn new(name: &'static str, default: &'static str) -> Self {
        Self { name, default }
    }
}

pub fn process_env_map() -> HashMap<String, String> {
    std::env::vars().collect()
}

pub fn deployment_stage_from_env_map(
    env: &HashMap<String, String>,
) -> anyhow::Result<DeploymentStage> {
    let value = required_env_value(env, ENV_AGENTICS_DEPLOYMENT_STAGE)?;
    value.parse()
}

pub fn validate_current_env_policy(role: EnvServiceRole) -> anyhow::Result<EnvPolicyReport> {
    let env = process_env_map();
    validate_env_policy(&env, role)
}

pub fn validate_env_policy(
    env: &HashMap<String, String>,
    role: EnvServiceRole,
) -> anyhow::Result<EnvPolicyReport> {
    let stage = deployment_stage_from_env_map(env)?;
    validate_stage_role(stage, role)?;

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    collect_deprecated_errors(env, &mut errors);
    collect_ignored_warnings(env, &mut warnings);
    collect_required_value_errors(stage, env, &mut errors);

    for name in required_envs(stage, role) {
        match env_value(env, name) {
            Some(value) => {
                if stage.rejects_placeholders() && is_placeholder(value) {
                    errors.push(format!("{name} still uses a replace-with-* placeholder"));
                }
            }
            None => errors.push(format!("{name} must be set for {stage} {role} startup")),
        }
    }

    for optional in optional_envs(stage, role) {
        if env_value(env, optional.name).is_none() {
            warnings.push(EnvPolicyWarning {
                name: optional.name,
                message: format!("unset; default: {}", optional.default),
            });
        }
    }

    if !errors.is_empty() {
        anyhow::bail!(errors.join("; "));
    }

    Ok(EnvPolicyReport {
        stage,
        role,
        warnings,
    })
}

pub fn known_stage_env_names() -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    for name in [
        ENV_AGENTICS_DEPLOYMENT_STAGE,
        ENV_AGENTICS_REHEARSAL_ENVIRONMENT,
        ENV_STALE_REVIEW_RECORD_LIMIT,
        ENV_REVIEW_RECORD_LIMIT,
        ENV_AGENTICS_WEB_HOST,
        ENV_RUST_LOG,
    ] {
        names.insert(name);
    }
    for name in DEV_REQUIRED
        .iter()
        .chain(TEST_REQUIRED)
        .chain(REHEARSAL_REQUIRED)
        .chain(PRODUCTION_REQUIRED)
        .chain(API_COMMON_REQUIRED)
        .chain(API_HOSTED_REQUIRED)
        .chain(WORKER_COMMON_REQUIRED)
        .chain(WORKER_HOSTED_REQUIRED)
        .chain(MIGRATE_REQUIRED)
        .chain(WEB_REQUIRED)
    {
        names.insert(*name);
    }
    for optional in DEV_OPTIONAL
        .iter()
        .chain(TEST_OPTIONAL)
        .chain(REHEARSAL_OPTIONAL)
        .chain(PRODUCTION_OPTIONAL)
        .chain(API_OPTIONAL)
        .chain(WORKER_OPTIONAL)
        .chain(MIGRATE_OPTIONAL)
        .chain(WEB_OPTIONAL)
    {
        names.insert(optional.name);
    }
    names.insert("COMPOSE_PROFILES");
    names.insert("DATABASE_URL");
    names
}

fn validate_stage_role(stage: DeploymentStage, role: EnvServiceRole) -> anyhow::Result<()> {
    match (stage, role) {
        (DeploymentStage::Dev, EnvServiceRole::LocalDev)
        | (DeploymentStage::Test, EnvServiceRole::TestHarness)
        | (DeploymentStage::Rehearsal | DeploymentStage::Production, EnvServiceRole::Compose)
        | (_, EnvServiceRole::Api | EnvServiceRole::Worker | EnvServiceRole::Migrate)
        | (_, EnvServiceRole::Web) => Ok(()),
        (DeploymentStage::Dev | DeploymentStage::Test, EnvServiceRole::Compose) => Ok(()),
        (other_stage, other_role) => anyhow::bail!(
            "{ENV_AGENTICS_DEPLOYMENT_STAGE}={other_stage} cannot be validated with {other_role} env policy"
        ),
    }
}

fn collect_deprecated_errors(env: &HashMap<String, String>, errors: &mut Vec<String>) {
    if env_value(env, ENV_STALE_REVIEW_RECORD_LIMIT).is_some() {
        errors.push(format!(
            "{ENV_STALE_REVIEW_RECORD_LIMIT} has been removed; use {ENV_REVIEW_RECORD_LIMIT}"
        ));
    }
    if env_value(env, ENV_AGENTICS_REHEARSAL_ENVIRONMENT).is_some() {
        errors.push(format!(
            "{ENV_AGENTICS_REHEARSAL_ENVIRONMENT} has been removed; use {ENV_AGENTICS_DEPLOYMENT_STAGE}=rehearsal"
        ));
    }
}

fn collect_ignored_warnings(env: &HashMap<String, String>, warnings: &mut Vec<EnvPolicyWarning>) {
    if env_value(env, ENV_AGENTICS_WEB_HOST).is_some() {
        warnings.push(EnvPolicyWarning {
            name: ENV_AGENTICS_WEB_HOST,
            message: "ignored; web bind host is owned by the Compose command".to_string(),
        });
    }
    if env_value(env, ENV_RUST_LOG).is_some() {
        warnings.push(EnvPolicyWarning {
            name: ENV_RUST_LOG,
            message: "ignored; use AGENTICS_LOG_LEVEL for Agentics service logging".to_string(),
        });
    }
}

fn collect_required_value_errors(
    stage: DeploymentStage,
    env: &HashMap<String, String>,
    errors: &mut Vec<String>,
) {
    if stage == DeploymentStage::Production {
        require_exact_value(env, "AGENTICS_WEB_SESSION_COOKIE_SECURE", "true", errors);
    }
    if matches!(
        stage,
        DeploymentStage::Rehearsal | DeploymentStage::Production
    ) {
        require_exact_value(
            env,
            "AGENTICS_RUNNER_SECURITY_PROFILE",
            "production",
            errors,
        );
        require_exact_value(env, "AGENTICS_HOST_PROBE_MODE", "require", errors);
        require_exact_value(env, "AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES", "true", errors);
        require_exact_value(
            env,
            "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE",
            "xfs-project-quota-slots",
            errors,
        );
        require_exact_value(env, "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA", "true", errors);
    }
}

fn require_exact_value(
    env: &HashMap<String, String>,
    name: &'static str,
    expected: &'static str,
    errors: &mut Vec<String>,
) {
    if let Some(value) = env_value(env, name)
        && value != expected
    {
        errors.push(format!("{name} must be `{expected}`"));
    }
}

fn required_envs(stage: DeploymentStage, role: EnvServiceRole) -> &'static [&'static str] {
    match role {
        EnvServiceRole::Compose => match stage {
            DeploymentStage::Dev => DEV_REQUIRED,
            DeploymentStage::Test => TEST_REQUIRED,
            DeploymentStage::Rehearsal => REHEARSAL_REQUIRED,
            DeploymentStage::Production => PRODUCTION_REQUIRED,
        },
        EnvServiceRole::LocalDev => DEV_REQUIRED,
        EnvServiceRole::TestHarness => TEST_REQUIRED,
        EnvServiceRole::Api => match stage {
            DeploymentStage::Rehearsal | DeploymentStage::Production => API_HOSTED_REQUIRED,
            DeploymentStage::Dev | DeploymentStage::Test => API_COMMON_REQUIRED,
        },
        EnvServiceRole::Worker => match stage {
            DeploymentStage::Rehearsal | DeploymentStage::Production => WORKER_HOSTED_REQUIRED,
            DeploymentStage::Dev | DeploymentStage::Test => WORKER_COMMON_REQUIRED,
        },
        EnvServiceRole::Migrate => MIGRATE_REQUIRED,
        EnvServiceRole::Web => WEB_REQUIRED,
    }
}

fn optional_envs(stage: DeploymentStage, role: EnvServiceRole) -> &'static [OptionalEnv] {
    match role {
        EnvServiceRole::Compose => match stage {
            DeploymentStage::Dev => DEV_OPTIONAL,
            DeploymentStage::Test => TEST_OPTIONAL,
            DeploymentStage::Rehearsal => REHEARSAL_OPTIONAL,
            DeploymentStage::Production => PRODUCTION_OPTIONAL,
        },
        EnvServiceRole::LocalDev => DEV_OPTIONAL,
        EnvServiceRole::TestHarness => TEST_OPTIONAL,
        EnvServiceRole::Api => API_OPTIONAL,
        EnvServiceRole::Worker => WORKER_OPTIONAL,
        EnvServiceRole::Migrate => MIGRATE_OPTIONAL,
        EnvServiceRole::Web => WEB_OPTIONAL,
    }
}

fn required_env_value<'a>(
    env: &'a HashMap<String, String>,
    name: &'static str,
) -> anyhow::Result<&'a str> {
    env_value(env, name).ok_or_else(|| anyhow::anyhow!("{name} must be set"))
}

fn env_value<'a>(env: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    env.get(name)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_placeholder(value: &str) -> bool {
    value
        .split(['/', ':', '@', ','])
        .any(|part| part.trim().starts_with("replace-with-"))
        || value.trim().starts_with("replace-with-")
}

const DEV_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_DATABASE_URL",
    "AGENTICS_LOCAL_DEV_DATABASE_NAME",
    "AGENTICS_LOCAL_DEV_DATABASE_URL",
    "AGENTICS_LOCAL_DEV_DATABASE_URL_CONFIRM",
    "AGENTICS_LOCAL_DEV_CHALLENGE_SOURCE_ROOT",
    "AGENTICS_LOCAL_DEV_TEST_SOLUTIONS_ROOT",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_API_BASE_URL",
];

const TEST_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "DATABASE_URL",
    "AGENTICS_DATABASE_URL",
    "AGENTICS_POSTGRES_USER",
    "AGENTICS_POSTGRES_PASSWORD",
    "AGENTICS_POSTGRES_DB",
    "AGENTICS_RUSTFS_ACCESS_KEY",
    "AGENTICS_RUSTFS_SECRET_KEY",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_PREFIX",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_API_HOST",
    "AGENTICS_API_PORT",
    "AGENTICS_WEB_PORT",
    "AGENTICS_CORS_ALLOWED_ORIGINS",
    "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS",
    "AGENTICS_AGENT_REGISTRATION_MODE",
    "AGENTICS_WEB_SESSION_COOKIE_SECURE",
    "AGENTICS_TEST_DOCKER_HOST",
    "AGENTICS_TEST_DOCKER_SOCKET_PATH",
];

const REHEARSAL_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_POSTGRES_USER",
    "AGENTICS_POSTGRES_PASSWORD",
    "AGENTICS_POSTGRES_DB",
    "AGENTICS_POSTGRES_PORT",
    "AGENTICS_DATABASE_URL",
    "AGENTICS_RUSTFS_ACCESS_KEY",
    "AGENTICS_RUSTFS_SECRET_KEY",
    "AGENTICS_RUSTFS_PORT",
    "AGENTICS_RUSTFS_CONSOLE_PORT",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_PREFIX",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_REHEARSAL_HOST_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_STORAGE_WORK_ROOT",
    "AGENTICS_API_HOST_PORT",
    "AGENTICS_WEB_HOST_PORT",
    "AGENTICS_API_BASE_URL",
    "AGENTICS_WEB_BASE_URL",
    "AGENTICS_CORS_ALLOWED_ORIGINS",
    "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS",
    "AGENTICS_WEB_SESSION_COOKIE_SECURE",
    "AGENTICS_AGENT_REGISTRATION_MODE",
    "AGENTICS_GITHUB_APP_CLIENT_ID",
    "AGENTICS_GITHUB_APP_CLIENT_SECRET",
    "AGENTICS_GITHUB_APP_REDIRECT_URL",
    "AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT",
    "AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT",
    "AGENTICS_DGX_STATE_ROOT",
    "AGENTICS_DOCKER_SOCKET_PATH",
    "AGENTICS_DOCKER_HOST",
    "AGENTICS_RUNNER_NAMESPACE",
    "AGENTICS_RUNTIME_UID",
    "AGENTICS_RUNTIME_GID",
    "AGENTICS_DOCKER_SOCKET_GID",
    "AGENTICS_RUNNER_SECURITY_PROFILE",
    "AGENTICS_HOST_PROBE_MODE",
    "AGENTICS_HOST_PROBE_COMMAND",
    "AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES",
    "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE",
    "AGENTICS_RUNNER_RUNTIME_ROOT",
    "AGENTICS_RUNNER_PHASE_MOUNT_ROOT",
    "AGENTICS_DGX_PHASE_MOUNT_ROOT",
    "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB",
    "AGENTICS_DGX_PHASE_SLOT_CLASSES_MB",
    "AGENTICS_DGX_PHASE_SLOTS_PER_CLASS",
    "AGENTICS_DGX_PHASE_SLOT_INODES_PER_MB",
    "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA",
];

const PRODUCTION_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_POSTGRES_USER",
    "AGENTICS_POSTGRES_PASSWORD",
    "AGENTICS_POSTGRES_DB",
    "AGENTICS_RUSTFS_ACCESS_KEY",
    "AGENTICS_RUSTFS_SECRET_KEY",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_PREFIX",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_STORAGE_WORK_ROOT",
    "AGENTICS_API_BASE_URL",
    "AGENTICS_WEB_BASE_URL",
    "AGENTICS_CORS_ALLOWED_ORIGINS",
    "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS",
    "AGENTICS_WEB_SESSION_COOKIE_SECURE",
    "AGENTICS_GITHUB_APP_CLIENT_ID",
    "AGENTICS_GITHUB_APP_CLIENT_SECRET",
    "AGENTICS_GITHUB_APP_REDIRECT_URL",
    "AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT",
    "AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT",
    "AGENTICS_DOCKER_SOCKET_PATH",
    "AGENTICS_DOCKER_HOST",
    "AGENTICS_RUNNER_NAMESPACE",
    "AGENTICS_RUNTIME_UID",
    "AGENTICS_RUNTIME_GID",
    "AGENTICS_DOCKER_SOCKET_GID",
    "AGENTICS_RUNNER_SECURITY_PROFILE",
    "AGENTICS_HOST_PROBE_MODE",
    "AGENTICS_HOST_PROBE_COMMAND",
    "AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES",
    "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE",
    "AGENTICS_RUNNER_RUNTIME_ROOT",
    "AGENTICS_RUNNER_PHASE_MOUNT_ROOT",
    "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB",
    "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA",
];

const API_COMMON_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_DATABASE_URL",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_API_HOST",
    "AGENTICS_API_PORT",
    "AGENTICS_CORS_ALLOWED_ORIGINS",
];

const API_HOSTED_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_DATABASE_URL",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_STORAGE_WORK_ROOT",
    "AGENTICS_API_HOST",
    "AGENTICS_API_PORT",
    "AGENTICS_CORS_ALLOWED_ORIGINS",
    "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS",
    "AGENTICS_WEB_SESSION_COOKIE_SECURE",
    "AGENTICS_GITHUB_APP_CLIENT_ID",
    "AGENTICS_GITHUB_APP_CLIENT_SECRET",
    "AGENTICS_GITHUB_APP_REDIRECT_URL",
];

const WORKER_COMMON_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_DATABASE_URL",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_RUNNER_NAMESPACE",
    "AGENTICS_RUNNER_RUNTIME_ROOT",
];

const WORKER_HOSTED_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_DATABASE_URL",
    "AGENTICS_STORAGE_BACKEND",
    "AGENTICS_S3_BUCKET",
    "AGENTICS_S3_REGION",
    "AGENTICS_S3_ENDPOINT_URL",
    "AGENTICS_S3_FORCE_PATH_STYLE",
    "AGENTICS_STORAGE_WORK_ROOT",
    "AGENTICS_DOCKER_HOST",
    "AGENTICS_RUNNER_NAMESPACE",
    "AGENTICS_RUNNER_SECURITY_PROFILE",
    "AGENTICS_HOST_PROBE_MODE",
    "AGENTICS_HOST_PROBE_COMMAND",
    "AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES",
    "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE",
    "AGENTICS_RUNNER_RUNTIME_ROOT",
    "AGENTICS_RUNNER_PHASE_MOUNT_ROOT",
    "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB",
    "AGENTICS_RUNNER_DOCKER_LAYER_QUOTA",
];

const MIGRATE_REQUIRED: &[&str] = &[ENV_AGENTICS_DEPLOYMENT_STAGE, "AGENTICS_DATABASE_URL"];

const WEB_REQUIRED: &[&str] = &[
    ENV_AGENTICS_DEPLOYMENT_STAGE,
    "AGENTICS_API_BASE_URL",
    "AGENTICS_WEB_PORT",
];

const DEV_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new(
        "AGENTICS_RUST_TOOLCHAIN_IMAGE",
        "local rust-toolchain image",
    ),
    OptionalEnv::new("AGENTICS_POSTGRES_USER", "agentics"),
    OptionalEnv::new("AGENTICS_POSTGRES_PASSWORD", "agentics"),
    OptionalEnv::new("AGENTICS_POSTGRES_DB", "agentics_dev"),
    OptionalEnv::new("AGENTICS_POSTGRES_PORT", "5432-derived local database port"),
    OptionalEnv::new("AGENTICS_POSTGRES_IMAGE", "postgres:18-alpine"),
    OptionalEnv::new("AGENTICS_POSTGRES_DATA_MOUNT", "/var/lib/postgresql"),
    OptionalEnv::new("AGENTICS_POSTGRES_IO_METHOD", "io_uring"),
    OptionalEnv::new(
        "AGENTICS_CHALLENGES_ROOT",
        "prepared local dev challenge root",
    ),
    OptionalEnv::new("AGENTICS_RUSTFS_ACCESS_KEY", "agenticsrustfs"),
    OptionalEnv::new("AGENTICS_RUSTFS_SECRET_KEY", "agenticsrustfssecret"),
    OptionalEnv::new("AGENTICS_RUSTFS_PORT", "9000"),
    OptionalEnv::new("AGENTICS_RUSTFS_CONSOLE_PORT", "9001"),
    OptionalEnv::new("AGENTICS_S3_PREFIX", "no prefix"),
    OptionalEnv::new("AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES", "1073741824"),
    OptionalEnv::new("AGENTICS_STORAGE_MAX_STATEMENT_BYTES", "1048576"),
    OptionalEnv::new("AGENTICS_STORAGE_MAX_JSON_ARTIFACT_BYTES", "1048576"),
    OptionalEnv::new("AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS", "24"),
    OptionalEnv::new("AGENTICS_API_HOST", "127.0.0.1"),
    OptionalEnv::new("AGENTICS_API_PORT", "3100"),
    OptionalEnv::new("AGENTICS_WEB_PORT", "3001"),
    OptionalEnv::new("AGENTICS_WEB_BASE_URL", "http://localhost:3001"),
    OptionalEnv::new("AGENTICS_CORS_ALLOWED_ORIGINS", "localhost web origins"),
    OptionalEnv::new(
        "AGENTICS_WEB_ALLOWED_DEV_ORIGINS",
        "127.0.0.1 and localhost",
    ),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
        "same-origin Next proxy",
    ),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
        "analytics disabled",
    ),
    OptionalEnv::new("AGENTICS_MOLTBOOK_SUBMOLT_NAME", "agentics-platform"),
    OptionalEnv::new(
        "AGENTICS_MOLTBOOK_SUBMOLT_URL",
        "https://www.moltbook.com/m/agentics-platform",
    ),
    OptionalEnv::new(
        "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS",
        "no bootstrap admins",
    ),
    OptionalEnv::new(
        "AGENTICS_GITHUB_APP_CLIENT_ID",
        "GitHub sign-in unavailable",
    ),
    OptionalEnv::new(
        "AGENTICS_GITHUB_APP_CLIENT_SECRET",
        "GitHub sign-in unavailable",
    ),
    OptionalEnv::new(
        "AGENTICS_GITHUB_APP_REDIRECT_URL",
        "GitHub sign-in unavailable",
    ),
    OptionalEnv::new("AGENTICS_WEB_SESSION_COOKIE_SECURE", "false for loopback"),
    OptionalEnv::new("AGENTICS_AGENT_REGISTRATION_MODE", "pioneer_code"),
    OptionalEnv::new("AGENTICS_OFFICIAL_LOG_REDACTION", "contract_based"),
    OptionalEnv::new("AGENTICS_LOG_LEVEL", "info"),
    OptionalEnv::new("AGENTICS_MAX_ACTIVE_AGENTS", "1000"),
    OptionalEnv::new("AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY", "20"),
    OptionalEnv::new("AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY", "5"),
    OptionalEnv::new("AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS", "20"),
];

const TEST_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new(
        "AGENTICS_RUST_TOOLCHAIN_IMAGE",
        "local rust-toolchain image",
    ),
    OptionalEnv::new("AGENTICS_POSTGRES_IMAGE", "postgres:18-alpine"),
    OptionalEnv::new("AGENTICS_POSTGRES_DATA_MOUNT", "/var/lib/postgresql"),
    OptionalEnv::new("AGENTICS_POSTGRES_IO_METHOD", "io_uring"),
    OptionalEnv::new("AGENTICS_TEST_DISABLE_CARGO_CACHE", "false"),
    OptionalEnv::new(
        "AGENTICS_TEST_CARGO_REGISTRY_VOLUME",
        "agentics-test-cargo-registry",
    ),
    OptionalEnv::new("AGENTICS_TEST_CARGO_GIT_VOLUME", "agentics-test-cargo-git"),
    OptionalEnv::new(
        "AGENTICS_TEST_CARGO_TARGET_VOLUME",
        "agentics-test-cargo-target",
    ),
    OptionalEnv::new("AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES", "1073741824"),
    OptionalEnv::new("AGENTICS_STORAGE_MAX_STATEMENT_BYTES", "1048576"),
    OptionalEnv::new("AGENTICS_STORAGE_MAX_JSON_ARTIFACT_BYTES", "1048576"),
    OptionalEnv::new("AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS", "24"),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
        "analytics disabled",
    ),
    OptionalEnv::new(
        "AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE",
        "xfs-project-quota-slots",
    ),
    OptionalEnv::new(
        "AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB",
        "64,256,1024,4096",
    ),
    OptionalEnv::new("AGENTICS_TEST_RUNNER_DOCKER_LAYER_QUOTA", "true"),
    OptionalEnv::new("AGENTICS_LOG_LEVEL", "info"),
];

const REHEARSAL_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new("COMPOSE_PROFILES", "no optional Compose profile"),
    OptionalEnv::new("AGENTICS_COMPOSE_PROD_PROJECT", "agentics-prod"),
    OptionalEnv::new(
        "AGENTICS_COMPOSE_PROD_SERVICE_ENV_FILE",
        "./env/prod.env.example",
    ),
    OptionalEnv::new("AGENTICS_COMPOSE_BIND_IP", "127.0.0.1"),
    OptionalEnv::new("AGENTICS_POSTGRES_IMAGE", "postgres:18-alpine"),
    OptionalEnv::new("AGENTICS_POSTGRES_DATA_MOUNT", "/var/lib/postgresql"),
    OptionalEnv::new("AGENTICS_POSTGRES_IO_METHOD", "io_uring"),
    OptionalEnv::new("AGENTICS_CHALLENGES_ROOT", "/app/challenges"),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
        "same-origin Next proxy",
    ),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
        "analytics disabled",
    ),
    OptionalEnv::new("AGENTICS_MOLTBOOK_SUBMOLT_NAME", "agentics-platform"),
    OptionalEnv::new(
        "AGENTICS_MOLTBOOK_SUBMOLT_URL",
        "https://www.moltbook.com/m/agentics-platform",
    ),
    OptionalEnv::new("AGENTICS_WORKER_ACCELERATORS", "none"),
    OptionalEnv::new(
        "AGENTICS_WORKER_GPU_PROBE_IMAGE",
        "required only for gpu workers",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_DOCKER_DATA_ROOT",
        "/srv/agentics/docker-data-root",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_RUNNER_DOCKER_EXEC_ROOT",
        "/srv/agentics/docker-exec",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_RUNNER_DOCKER_PIDFILE",
        "/srv/agentics/docker.pid",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_RUNNER_DOCKER_LOG",
        "/srv/agentics/dockerd.log",
    ),
    OptionalEnv::new("AGENTICS_DGX_RUNNER_DOCKER_BRIDGE", "agentics0"),
    OptionalEnv::new("AGENTICS_DGX_RUNNER_DOCKER_BRIDGE_CIDR", "172.30.0.1/16"),
    OptionalEnv::new("AGENTICS_DGX_DOCKER_PULL_POLICY", "if-not-present"),
    OptionalEnv::new("AGENTICS_DGX_PERSIST_FSTAB", "false"),
    OptionalEnv::new("AGENTICS_DGX_RUN_MUTATING_PROBES", "false"),
    OptionalEnv::new("AGENTICS_REHEARSAL_CPU_IMAGE_SOURCE", "registry"),
    OptionalEnv::new(
        "AGENTICS_REHEARSAL_CPU_IMAGE_REFERENCE",
        "built-in CPU fixture image",
    ),
    OptionalEnv::new("AGENTICS_LOG_LEVEL", "info"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_OUTPUT_FILES", "8192"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_OUTPUT_DIRS", "1024"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_OUTPUT_DEPTH", "32"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_RUNS", "challenge bundle maximum"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES", "4194304"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_PUBLIC_RESULTS", "1024"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES", "262144"),
    OptionalEnv::new(
        "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION",
        "268435456",
    ),
    OptionalEnv::new("AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS", "2"),
    OptionalEnv::new("AGENTICS_MAX_ACTIVE_AGENTS", "1000"),
    OptionalEnv::new("AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY", "20"),
    OptionalEnv::new("AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY", "5"),
    OptionalEnv::new("AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS", "20"),
    OptionalEnv::new(ENV_REVIEW_RECORD_LIMIT, "10"),
    OptionalEnv::new(
        "AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_REVIEW_RECORD",
        "1073741824",
    ),
    OptionalEnv::new("AGENTICS_CHALLENGE_REVIEW_RECORD_VALIDATIONS_PER_DAY", "10"),
    OptionalEnv::new("AGENTICS_CHALLENGE_REVIEW_RECORD_TTL_DAYS", "14"),
    OptionalEnv::new("AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS", "7"),
];

const PRODUCTION_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new("AGENTICS_COMPOSE_PROD_PROJECT", "agentics-prod"),
    OptionalEnv::new(
        "AGENTICS_COMPOSE_PROD_SERVICE_ENV_FILE",
        "./env/prod.env.example",
    ),
    OptionalEnv::new("AGENTICS_COMPOSE_BIND_IP", "127.0.0.1"),
    OptionalEnv::new("AGENTICS_POSTGRES_PORT", "5432"),
    OptionalEnv::new("AGENTICS_POSTGRES_IMAGE", "postgres:16-alpine"),
    OptionalEnv::new("AGENTICS_POSTGRES_VOLUME", "postgres_data"),
    OptionalEnv::new("AGENTICS_POSTGRES_DATA_MOUNT", "/var/lib/postgresql/data"),
    OptionalEnv::new("AGENTICS_POSTGRES_IO_METHOD", "worker"),
    OptionalEnv::new("AGENTICS_CHALLENGES_ROOT", "/app/challenges"),
    OptionalEnv::new("AGENTICS_API_HOST", "0.0.0.0 in Compose API service"),
    OptionalEnv::new("AGENTICS_API_HOST_PORT", "3100"),
    OptionalEnv::new("AGENTICS_API_PORT", "3100"),
    OptionalEnv::new("AGENTICS_WEB_HOST_PORT", "3001"),
    OptionalEnv::new("AGENTICS_WEB_PORT", "3001"),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
        "same-origin Next proxy",
    ),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
        "analytics disabled",
    ),
    OptionalEnv::new("AGENTICS_MOLTBOOK_SUBMOLT_NAME", "agentics-platform"),
    OptionalEnv::new(
        "AGENTICS_MOLTBOOK_SUBMOLT_URL",
        "https://www.moltbook.com/m/agentics-platform",
    ),
    OptionalEnv::new("AGENTICS_AGENT_REGISTRATION_MODE", "pioneer_code"),
    OptionalEnv::new("AGENTICS_WORKER_ACCELERATORS", "none"),
    OptionalEnv::new(
        "AGENTICS_WORKER_GPU_PROBE_IMAGE",
        "required only for gpu workers",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_DOCKER_DATA_ROOT",
        "/srv/agentics/docker-data-root",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_RUNNER_DOCKER_EXEC_ROOT",
        "/srv/agentics/docker-exec",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_RUNNER_DOCKER_PIDFILE",
        "/srv/agentics/docker.pid",
    ),
    OptionalEnv::new(
        "AGENTICS_DGX_RUNNER_DOCKER_LOG",
        "/srv/agentics/dockerd.log",
    ),
    OptionalEnv::new("AGENTICS_DGX_RUNNER_DOCKER_BRIDGE", "agentics0"),
    OptionalEnv::new("AGENTICS_DGX_RUNNER_DOCKER_BRIDGE_CIDR", "172.30.0.1/16"),
    OptionalEnv::new("AGENTICS_DGX_DOCKER_PULL_POLICY", "if-not-present"),
    OptionalEnv::new("AGENTICS_DGX_RUN_MUTATING_PROBES", "false"),
    OptionalEnv::new("AGENTICS_LOG_LEVEL", "info"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_OUTPUT_FILES", "8192"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_OUTPUT_DIRS", "1024"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_OUTPUT_DEPTH", "32"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_RUNS", "challenge bundle maximum"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES", "4194304"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_PUBLIC_RESULTS", "1024"),
    OptionalEnv::new("AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES", "262144"),
    OptionalEnv::new(
        "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION",
        "268435456",
    ),
    OptionalEnv::new("AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS", "2"),
    OptionalEnv::new("AGENTICS_MAX_ACTIVE_AGENTS", "1000"),
    OptionalEnv::new("AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY", "20"),
    OptionalEnv::new("AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY", "5"),
    OptionalEnv::new("AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS", "20"),
    OptionalEnv::new(ENV_REVIEW_RECORD_LIMIT, "10"),
    OptionalEnv::new(
        "AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_REVIEW_RECORD",
        "1073741824",
    ),
    OptionalEnv::new("AGENTICS_CHALLENGE_REVIEW_RECORD_VALIDATIONS_PER_DAY", "10"),
    OptionalEnv::new("AGENTICS_CHALLENGE_REVIEW_RECORD_TTL_DAYS", "14"),
    OptionalEnv::new("AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS", "7"),
];

const API_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new("AGENTICS_LOG_LEVEL", "info"),
    OptionalEnv::new("AGENTICS_WEB_SESSION_COOKIE_NAME", "agentics_session"),
    OptionalEnv::new("AGENTICS_WEB_CSRF_COOKIE_NAME", "agentics_csrf"),
    OptionalEnv::new("AGENTICS_WEB_SESSION_TTL_HOURS", "24"),
];

const WORKER_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new("AGENTICS_LOG_LEVEL", "info"),
    OptionalEnv::new("AGENTICS_WORKER_POLL_INTERVAL_MS", "3000"),
    OptionalEnv::new("AGENTICS_WORKER_STALE_JOB_MINUTES", "1"),
    OptionalEnv::new("AGENTICS_WORKER_ACCELERATORS", "none"),
    OptionalEnv::new(
        "AGENTICS_WORKER_GPU_PROBE_IMAGE",
        "required only for gpu workers",
    ),
];

const MIGRATE_OPTIONAL: &[OptionalEnv] = &[OptionalEnv::new("AGENTICS_LOG_LEVEL", "info")];

const WEB_OPTIONAL: &[OptionalEnv] = &[
    OptionalEnv::new(
        "AGENTICS_WEB_ALLOWED_DEV_ORIGINS",
        "127.0.0.1 and localhost",
    ),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_API_BASE_URL",
        "same-origin Next proxy",
    ),
    OptionalEnv::new(
        "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID",
        "analytics disabled",
    ),
];
