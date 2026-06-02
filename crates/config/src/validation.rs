//! Field-local configuration validation implemented with `garde`.

use garde::{Error, Validate};
use secrecy::{ExposeSecret, SecretString};
use std::path::Path;

/// Validate a `garde` report and render field names as `AGENTICS_*` env vars.
pub(crate) fn validate_report<T>(value: &T) -> anyhow::Result<()>
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
    let leaf = field.rsplit('.').next().unwrap_or(field);
    Some(match leaf {
        "bootstrap_admin_github_user_ids" => "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS",
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
        "max_active_challenge_review_records_per_human" => {
            "AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_HUMAN"
        }
        "challenge_private_asset_bytes_per_review_record" => {
            "AGENTICS_CHALLENGE_PRIVATE_ASSET_BYTES_PER_REVIEW_RECORD"
        }
        "challenge_review_record_validations_per_day" => {
            "AGENTICS_CHALLENGE_REVIEW_RECORD_VALIDATIONS_PER_DAY"
        }
        "challenge_review_record_validation_timeout_minutes" => {
            "AGENTICS_CHALLENGE_REVIEW_RECORD_VALIDATION_TIMEOUT_MINUTES"
        }
        "challenge_private_asset_pending_timeout_minutes" => {
            "AGENTICS_CHALLENGE_PRIVATE_ASSET_PENDING_TIMEOUT_MINUTES"
        }
        "challenge_review_record_publish_timeout_minutes" => {
            "AGENTICS_CHALLENGE_REVIEW_RECORD_PUBLISH_TIMEOUT_MINUTES"
        }
        "challenge_review_record_ttl_days" => "AGENTICS_CHALLENGE_REVIEW_RECORD_TTL_DAYS",
        "unpublished_challenge_asset_grace_days" => {
            "AGENTICS_UNPUBLISHED_CHALLENGE_ASSET_GRACE_DAYS"
        }
        "max_bundle_archive_bytes" => "AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES",
        "max_statement_bytes" => "AGENTICS_STORAGE_MAX_STATEMENT_BYTES",
        "max_json_artifact_bytes" => "AGENTICS_STORAGE_MAX_JSON_ARTIFACT_BYTES",
        "tmp_object_grace_hours" => "AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS",
        "work_root" => "AGENTICS_STORAGE_WORK_ROOT",
        "s3_prefix" => "AGENTICS_S3_PREFIX",
        "s3_region" => "AGENTICS_S3_REGION",
        "s3_endpoint_url" => "AGENTICS_S3_ENDPOINT_URL",
        "poll_interval_ms" => "AGENTICS_WORKER_POLL_INTERVAL_MS",
        "stale_job_minutes" => "AGENTICS_WORKER_STALE_JOB_MINUTES",
        "gpu_probe_image" => "AGENTICS_WORKER_GPU_PROBE_IMAGE",
        "host_probe_command" => "AGENTICS_HOST_PROBE_COMMAND",
        "runtime_root" => "AGENTICS_RUNNER_RUNTIME_ROOT",
        "phase_mount_root" => "AGENTICS_RUNNER_PHASE_MOUNT_ROOT",
        "writable_slot_classes_mb" => "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB",
        "max_output_files" => "AGENTICS_RUNNER_MAX_OUTPUT_FILES",
        "max_output_dirs" => "AGENTICS_RUNNER_MAX_OUTPUT_DIRS",
        "max_output_depth" => "AGENTICS_RUNNER_MAX_OUTPUT_DEPTH",
        "max_runs" => "AGENTICS_RUNNER_MAX_RUNS",
        "max_result_json_bytes" => "AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES",
        "max_public_results" => "AGENTICS_RUNNER_MAX_PUBLIC_RESULTS",
        "max_result_log_bytes" => "AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES",
        "max_interaction_bytes_per_direction" => {
            "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION"
        }
        "interaction_shutdown_grace_secs" => "AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS",
        "log_level" => "AGENTICS_LOG_LEVEL",
        _ => return None,
    })
}

pub(crate) fn trimmed_non_empty(value: &str, _ctx: &()) -> Result<(), Error> {
    if value.trim().is_empty() {
        return Err(Error::new("must not be empty"));
    }
    Ok(())
}

pub(crate) fn optional_trimmed_non_empty(value: &Option<String>, _ctx: &()) -> Result<(), Error> {
    if let Some(value) = value
        && value.trim().is_empty()
    {
        return Err(Error::new("must not be empty when set"));
    }
    Ok(())
}

pub(crate) fn optional_secret_non_empty(
    value: &Option<SecretString>,
    _ctx: &(),
) -> Result<(), Error> {
    if let Some(value) = value
        && value.expose_secret().trim().is_empty()
    {
        return Err(Error::new("must not be empty when set"));
    }
    Ok(())
}

pub(crate) fn optional_absolute_path(value: &Option<String>, _ctx: &()) -> Result<(), Error> {
    let Some(path) = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if !Path::new(path).is_absolute() {
        return Err(Error::new("must be an absolute path"));
    }
    Ok(())
}

pub(crate) fn cookie_name(value: &str, _ctx: &()) -> Result<(), Error> {
    crate::local_urls::validate_cookie_name(value, "cookie name")
        .map_err(|error| Error::new(error.to_string()))
}

pub(crate) fn cors_origin_list(value: &str, _ctx: &()) -> Result<(), Error> {
    for origin in value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
    {
        crate::validate_cors_origin(origin).map_err(|error| Error::new(error.to_string()))?;
    }
    Ok(())
}

pub(crate) fn runner_slot_class_csv(value: &str, _ctx: &()) -> Result<(), Error> {
    let mut saw_value = false;
    for raw in value.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        saw_value = true;
        let parsed = value
            .parse::<u64>()
            .map_err(|error| Error::new(format!("contains invalid entry `{value}`: {error}")))?;
        if parsed == 0 {
            return Err(Error::new("entries must be positive"));
        }
    }
    if !saw_value {
        return Err(Error::new("must not be empty"));
    }
    Ok(())
}
