//! Field-local configuration validation implemented with `garde`.

use garde::{Error, Validate};
use secrecy::{ExposeSecret, SecretString};
use std::path::Path;
use url::Url;

use crate::Config;

#[derive(Debug, Validate)]
#[garde(allow_unvalidated)]
struct ApiSecurityFields<'a> {
    #[garde(custom(trimmed_non_empty))]
    admin_username: &'a str,
    #[garde(custom(secret_non_empty))]
    admin_password: &'a SecretString,
    #[garde(custom(cookie_name))]
    web_session_cookie_name: &'a str,
    #[garde(custom(cookie_name))]
    web_csrf_cookie_name: &'a str,
    #[garde(range(min = 1))]
    web_session_ttl_hours: i64,
    #[garde(custom(cors_origin_list))]
    cors_allowed_origins: &'a str,
    #[garde(range(min = 1))]
    validation_runs_per_agent_challenge_day: u32,
    #[garde(range(min = 1))]
    official_runs_per_agent_challenge_day: u32,
    #[garde(range(min = 1))]
    max_active_official_jobs: u32,
    #[garde(range(min = 1))]
    max_active_agents: u32,
    #[garde(range(min = 1))]
    max_active_challenge_drafts_per_agent: u32,
    #[garde(range(min = 1))]
    challenge_private_asset_bytes_per_draft: u64,
    #[garde(range(min = 1))]
    challenge_draft_validations_per_day: u32,
    #[garde(range(min = 1))]
    challenge_draft_validation_timeout_minutes: i32,
    #[garde(range(min = 1))]
    challenge_private_asset_pending_timeout_minutes: i32,
    #[garde(range(min = 1))]
    challenge_draft_publish_timeout_minutes: i32,
    #[garde(range(min = 1))]
    challenge_draft_ttl_days: i64,
    #[garde(range(min = 1))]
    unpublished_challenge_asset_grace_days: i64,
}

#[derive(Debug, Validate)]
#[garde(allow_unvalidated)]
struct StorageCommonFields<'a> {
    #[garde(range(min = 1))]
    storage_max_bundle_archive_bytes: u64,
    #[garde(range(min = 1))]
    storage_max_statement_bytes: u64,
    #[garde(range(min = 1))]
    storage_max_json_artifact_bytes: u64,
    #[garde(range(min = 1))]
    storage_tmp_object_grace_hours: u64,
    #[garde(custom(optional_absolute_path))]
    storage_work_root: Option<&'a str>,
}

#[derive(Debug, Validate)]
#[garde(allow_unvalidated)]
struct S3Fields<'a> {
    #[garde(custom(optional_s3_prefix))]
    s3_prefix: Option<&'a str>,
    #[garde(custom(trimmed_non_empty))]
    s3_region: &'a str,
    #[garde(custom(optional_http_url))]
    s3_endpoint_url: Option<&'a Url>,
}

#[derive(Debug, Validate)]
#[garde(allow_unvalidated)]
struct RunnerOutputLimitFields {
    #[garde(range(min = 1))]
    runner_max_output_files: u64,
    #[garde(range(min = 1))]
    runner_max_output_dirs: u64,
    #[garde(range(min = 1))]
    runner_max_output_depth: u64,
    #[garde(range(min = 1, max = agentics_contracts::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION))]
    runner_max_runs: u64,
    #[garde(range(min = 1))]
    runner_max_result_json_bytes: u64,
    #[garde(range(min = 1))]
    runner_max_public_results: u64,
    #[garde(range(min = 1))]
    runner_max_result_log_bytes: u64,
    #[garde(range(min = 1))]
    runner_max_interaction_bytes_per_direction: u64,
    #[garde(range(min = 1))]
    runner_interaction_shutdown_grace_secs: u64,
}

pub(crate) fn validate_api_security_fields(config: &Config) -> anyhow::Result<()> {
    validate_report(ApiSecurityFields {
        admin_username: &config.admin_username,
        admin_password: &config.admin_password,
        web_session_cookie_name: &config.web_session_cookie_name,
        web_csrf_cookie_name: &config.web_csrf_cookie_name,
        web_session_ttl_hours: config.web_session_ttl_hours,
        cors_allowed_origins: &config.cors_allowed_origins,
        validation_runs_per_agent_challenge_day: config.validation_runs_per_agent_challenge_day,
        official_runs_per_agent_challenge_day: config.official_runs_per_agent_challenge_day,
        max_active_official_jobs: config.max_active_official_jobs,
        max_active_agents: config.max_active_agents,
        max_active_challenge_drafts_per_agent: config.max_active_challenge_drafts_per_agent,
        challenge_private_asset_bytes_per_draft: config.challenge_private_asset_bytes_per_draft,
        challenge_draft_validations_per_day: config.challenge_draft_validations_per_day,
        challenge_draft_validation_timeout_minutes: config
            .challenge_draft_validation_timeout_minutes,
        challenge_private_asset_pending_timeout_minutes: config
            .challenge_private_asset_pending_timeout_minutes,
        challenge_draft_publish_timeout_minutes: config.challenge_draft_publish_timeout_minutes,
        challenge_draft_ttl_days: config.challenge_draft_ttl_days,
        unpublished_challenge_asset_grace_days: config.unpublished_challenge_asset_grace_days,
    })
}

pub(crate) fn validate_storage_common_fields(config: &Config) -> anyhow::Result<()> {
    validate_report(StorageCommonFields {
        storage_max_bundle_archive_bytes: config.storage_max_bundle_archive_bytes,
        storage_max_statement_bytes: config.storage_max_statement_bytes,
        storage_max_json_artifact_bytes: config.storage_max_json_artifact_bytes,
        storage_tmp_object_grace_hours: config.storage_tmp_object_grace_hours,
        storage_work_root: config.storage_work_root.as_deref(),
    })
}

pub(crate) fn validate_s3_fields(config: &Config) -> anyhow::Result<()> {
    validate_report(S3Fields {
        s3_prefix: config.s3_prefix.as_deref(),
        s3_region: &config.s3_region,
        s3_endpoint_url: config.s3_endpoint_url.as_ref(),
    })
}

pub(crate) fn validate_runner_output_limits(config: &Config) -> anyhow::Result<()> {
    validate_report(RunnerOutputLimitFields {
        runner_max_output_files: config.runner_max_output_files,
        runner_max_output_dirs: config.runner_max_output_dirs,
        runner_max_output_depth: config.runner_max_output_depth,
        runner_max_runs: config.runner_max_runs,
        runner_max_result_json_bytes: config.runner_max_result_json_bytes,
        runner_max_public_results: config.runner_max_public_results,
        runner_max_result_log_bytes: config.runner_max_result_log_bytes,
        runner_max_interaction_bytes_per_direction: config
            .runner_max_interaction_bytes_per_direction,
        runner_interaction_shutdown_grace_secs: config.runner_interaction_shutdown_grace_secs,
    })
}

fn validate_report<T>(value: T) -> anyhow::Result<()>
where
    T: Validate<Context = ()>,
{
    value.validate().map_err(|report| {
        let message = report
            .iter()
            .map(|(path, error)| {
                let field = path.to_string();
                let field = env_name_for_field(&field).unwrap_or(field.as_str());
                format!("{field}: {error}")
            })
            .collect::<Vec<_>>()
            .join("; ");
        anyhow::anyhow!(message)
    })
}

fn env_name_for_field(field: &str) -> Option<&'static str> {
    Some(match field {
        "admin_username" => "AGENTICS_ADMIN_USERNAME",
        "admin_password" => "AGENTICS_ADMIN_PASSWORD",
        "web_session_cookie_name" => "AGENTICS_WEB_SESSION_COOKIE_NAME",
        "web_csrf_cookie_name" => "AGENTICS_WEB_CSRF_COOKIE_NAME",
        "web_session_ttl_hours" => "AGENTICS_WEB_SESSION_TTL_HOURS",
        "cors_allowed_origins" => "AGENTICS_CORS_ALLOWED_ORIGINS",
        "validation_runs_per_agent_challenge_day" => {
            "AGENTICS_VALIDATION_RUNS_PER_AGENT_CHALLENGE_DAY"
        }
        "official_runs_per_agent_challenge_day" => "AGENTICS_OFFICIAL_RUNS_PER_AGENT_CHALLENGE_DAY",
        "max_active_official_jobs" => "AGENTICS_MAX_ACTIVE_OFFICIAL_JOBS",
        "max_active_agents" => "AGENTICS_MAX_ACTIVE_AGENTS",
        "max_active_challenge_drafts_per_agent" => "AGENTICS_MAX_ACTIVE_CHALLENGE_DRAFTS_PER_AGENT",
        "challenge_private_asset_bytes_per_draft" => {
            "AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_DRAFT"
        }
        "challenge_draft_validations_per_day" => "AGENTICS_CHALLENGE_DRAFT_VALIDATIONS_PER_DAY",
        "challenge_draft_validation_timeout_minutes" => {
            "AGENTICS_CHALLENGE_DRAFT_VALIDATION_TIMEOUT_MINUTES"
        }
        "challenge_private_asset_pending_timeout_minutes" => {
            "AGENTICS_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES"
        }
        "challenge_draft_publish_timeout_minutes" => {
            "AGENTICS_CHALLENGE_DRAFT_PUBLISH_TIMEOUT_MINUTES"
        }
        "challenge_draft_ttl_days" => "AGENTICS_CHALLENGE_DRAFT_TTL_DAYS",
        "unpublished_challenge_asset_grace_days" => {
            "AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS"
        }
        "storage_max_bundle_archive_bytes" => "AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES",
        "storage_max_statement_bytes" => "AGENTICS_STORAGE_MAX_STATEMENT_BYTES",
        "storage_max_json_artifact_bytes" => "AGENTICS_STORAGE_MAX_JSON_ARTIFACT_BYTES",
        "storage_tmp_object_grace_hours" => "AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS",
        "storage_work_root" => "AGENTICS_STORAGE_WORK_ROOT",
        "s3_prefix" => "AGENTICS_S3_PREFIX",
        "s3_region" => "AGENTICS_S3_REGION",
        "s3_endpoint_url" => "AGENTICS_S3_ENDPOINT_URL",
        "runner_max_output_files" => "AGENTICS_RUNNER_MAX_OUTPUT_FILES",
        "runner_max_output_dirs" => "AGENTICS_RUNNER_MAX_OUTPUT_DIRS",
        "runner_max_output_depth" => "AGENTICS_RUNNER_MAX_OUTPUT_DEPTH",
        "runner_max_runs" => "AGENTICS_RUNNER_MAX_RUNS",
        "runner_max_result_json_bytes" => "AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES",
        "runner_max_public_results" => "AGENTICS_RUNNER_MAX_PUBLIC_RESULTS",
        "runner_max_result_log_bytes" => "AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES",
        "runner_max_interaction_bytes_per_direction" => {
            "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION"
        }
        "runner_interaction_shutdown_grace_secs" => {
            "AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS"
        }
        _ => return None,
    })
}

fn trimmed_non_empty(value: &str, _ctx: &()) -> Result<(), Error> {
    if value.trim().is_empty() {
        return Err(Error::new("must not be empty"));
    }
    Ok(())
}

fn secret_non_empty(value: &SecretString, _ctx: &()) -> Result<(), Error> {
    if value.expose_secret().trim().is_empty() {
        return Err(Error::new("must not be empty"));
    }
    Ok(())
}

fn optional_absolute_path(value: &Option<&str>, _ctx: &()) -> Result<(), Error> {
    let Some(path) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if !Path::new(path).is_absolute() {
        return Err(Error::new("must be an absolute path"));
    }
    Ok(())
}

fn cookie_name(value: &str, _ctx: &()) -> Result<(), Error> {
    crate::local_urls::validate_cookie_name(value, "cookie name")
        .map_err(|error| Error::new(error.to_string()))
}

fn cors_origin_list(value: &str, _ctx: &()) -> Result<(), Error> {
    for origin in value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
    {
        crate::validate_cors_origin(origin).map_err(|error| Error::new(error.to_string()))?;
    }
    Ok(())
}

fn optional_s3_prefix(value: &Option<&str>, _ctx: &()) -> Result<(), Error> {
    crate::storage_config::validate_s3_prefix(*value).map_err(|error| Error::new(error.to_string()))
}

fn optional_http_url(value: &Option<&Url>, _ctx: &()) -> Result<(), Error> {
    if let Some(url) = value
        && !matches!(url.scheme(), "http" | "https")
    {
        return Err(Error::new("must start with http:// or https://"));
    }
    Ok(())
}
