//! Runtime config resolution for the production rehearsal harness.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentics_config::{Config, DEFAULT_API_HOST, DEFAULT_API_PORT, local_api_base_url};
use reqwest::Url;
use secrecy::SecretString;
use serde::Deserialize;
use uuid::Uuid;

use super::{
    DEFAULT_CPU_IMAGE_REFERENCE, DEFAULT_CPU_IMAGE_SOURCE, DEFAULT_ENV_FILE,
    DEFAULT_WAIT_TIMEOUT_SECONDS, ProductionRehearsalError, RehearsalImageConfig, RunArgs,
};

#[derive(Debug, Deserialize)]
struct RawRehearsalEnv {
    api_base_url: Option<String>,
    web_base_url: Option<String>,
    rehearsal_cpu_image_source: Option<String>,
    rehearsal_cpu_image_reference: Option<String>,
}

pub(super) struct ResolvedRunConfig {
    pub config: Config,
    pub admin_service_token: SecretString,
    pub api_base_url: Url,
    pub web_base_url: Option<Url>,
    pub run_id: String,
    pub output_dir: PathBuf,
    pub image_config: RehearsalImageConfig,
    pub wait_timeout: Duration,
}

pub(super) fn resolve_run_config(
    args: &RunArgs,
) -> Result<ResolvedRunConfig, ProductionRehearsalError> {
    load_rehearsal_env_file(args.env_file.as_deref())?;
    let config = Config::from_env()?;
    let env = read_env()?;
    let admin_service_token = admin_service_token(args)?;
    let api_base_url = resolve_api_base_url(args.api_base_url.as_deref(), &env, &config)?;
    let web_base_url = resolve_optional_url(
        "web_base_url",
        args.web_base_url.as_deref().or(env.web_base_url.as_deref()),
    )?;
    let run_id = args.run_id.clone().unwrap_or_else(generate_run_id);
    let output_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("rehearsals").join(&run_id));
    let output_dir = if output_dir.is_absolute() {
        output_dir
    } else {
        std::env::current_dir()?.join(output_dir)
    };
    let image_config = RehearsalImageConfig {
        source: args
            .cpu_image_source
            .clone()
            .or(env.rehearsal_cpu_image_source)
            .unwrap_or_else(|| DEFAULT_CPU_IMAGE_SOURCE.to_string()),
        reference: args
            .cpu_image_reference
            .clone()
            .or(env.rehearsal_cpu_image_reference)
            .unwrap_or_else(|| DEFAULT_CPU_IMAGE_REFERENCE.to_string()),
    };
    let wait_timeout = Duration::from_secs(
        args.wait_timeout_seconds
            .unwrap_or(DEFAULT_WAIT_TIMEOUT_SECONDS),
    );

    Ok(ResolvedRunConfig {
        config,
        admin_service_token,
        api_base_url,
        web_base_url,
        run_id,
        output_dir,
        image_config,
        wait_timeout,
    })
}

pub(super) fn registration_code() -> String {
    let random_hex = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    format!("reh-{random_hex}")
}

fn admin_service_token(args: &RunArgs) -> Result<SecretString, ProductionRehearsalError> {
    if args.admin_service_token_stdin {
        let mut token = String::new();
        use std::io::Read as _;
        std::io::stdin().read_to_string(&mut token)?;
        let token = token.trim_end().to_string();
        if token.trim().is_empty() {
            return Err(ProductionRehearsalError::Config(
                "admin service token from stdin is empty".to_string(),
            ));
        }
        return Ok(SecretString::from(token));
    }
    let token = std::env::var("AGENTICS_ADMIN_SERVICE_TOKEN").map_err(|_| {
        ProductionRehearsalError::Config(
            "AGENTICS_ADMIN_SERVICE_TOKEN is required unless --admin-service-token-stdin is used"
                .to_string(),
        )
    })?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(ProductionRehearsalError::Config(
            "AGENTICS_ADMIN_SERVICE_TOKEN must not be empty".to_string(),
        ));
    }
    Ok(SecretString::from(token))
}

fn read_env() -> Result<RawRehearsalEnv, ProductionRehearsalError> {
    envy::prefixed("AGENTICS_")
        .from_env::<RawRehearsalEnv>()
        .map_err(|error| ProductionRehearsalError::Config(error.to_string()))
}

fn resolve_api_base_url(
    arg: Option<&str>,
    env: &RawRehearsalEnv,
    config: &Config,
) -> Result<Url, ProductionRehearsalError> {
    let value = arg
        .or(env.api_base_url.as_deref())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            local_api_base_url(
                if config.api_web.api_host.is_empty() {
                    DEFAULT_API_HOST
                } else {
                    &config.api_web.api_host
                },
                if config.api_web.api_port == 0 {
                    DEFAULT_API_PORT
                } else {
                    config.api_web.api_port
                },
            )
        });
    Url::parse(&value).map_err(|error| ProductionRehearsalError::InvalidUrl {
        field: "api_base_url",
        value,
        source: error,
    })
}

fn resolve_optional_url(
    field: &'static str,
    value: Option<&str>,
) -> Result<Option<Url>, ProductionRehearsalError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            Url::parse(value)
                .map(Some)
                .map_err(|error| ProductionRehearsalError::InvalidUrl {
                    field,
                    value: value.to_string(),
                    source: error,
                })
        }
        None => Ok(None),
    }
}

fn load_rehearsal_env_file(path: Option<&Path>) -> Result<(), ProductionRehearsalError> {
    match path {
        Some(path) => {
            dotenvy::from_path(path)?;
            Ok(())
        }
        None => {
            let default = Path::new(DEFAULT_ENV_FILE);
            if default.exists() {
                dotenvy::from_path(default)?;
            }
            Ok(())
        }
    }
}

fn generate_run_id() -> String {
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect()
}
