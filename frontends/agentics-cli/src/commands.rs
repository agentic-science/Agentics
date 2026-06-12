use agentics_domain::models::pioneer_codes::{PioneerCode, PioneerCodeInput};
use agentics_domain::models::request::RegisterAgentRequest;
use anyhow::{Context, Result, bail};
use secrecy::{ExposeSecret, SecretString};

use crate::CommandInput;
use crate::api::ApiClient;
use crate::cli::{self, ConfigKey, RegisterArgs};
use crate::config::{ApiBaseUrl, CliConfig, ConfigStore, ResolvedSettings};
use crate::output;

mod admin;
mod creator;
mod local_validation;
mod submissions;

pub(crate) use admin::admin_command;
pub(crate) use creator::{
    challenge_creator_check, challenge_review_record, challenge_shortlist, creator_participants,
    creator_stats,
};
pub(crate) use submissions::{
    challenge_stats, list_public_solution_submissions, solution_submission_report, submit,
    validate, wait_for_solution_submission,
};

/// Handles register for this module.
pub(crate) async fn register(
    args: RegisterArgs,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    mut file_config: CliConfig,
    settings: &ResolvedSettings,
) -> Result<String> {
    let model_info = parse_model_info(&args.model_info_json)?;
    let pioneer_code = args
        .pioneer_code
        .as_deref()
        .map(SecretString::from)
        .or_else(|| settings.pioneer_code.clone())
        .map(|pioneer_code| {
            let pioneer_code = PioneerCode::try_new(pioneer_code.expose_secret().to_string())
                .context("invalid pioneer code")?;
            PioneerCodeInput::try_new(pioneer_code.expose_secret().to_string())
                .context("invalid pioneer code")
        })
        .transpose()?;
    let request = RegisterAgentRequest {
        display_name: args.display_name,
        pioneer_code,
        agent_description: args.agent_description,
        model_info,
    };

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let response = client.register(&request).await?;
    let saved_token = !args.print_token;
    if saved_token {
        file_config.api_base_url = Some(settings.api_base_url.to_string());
        file_config.token = Some(response.token.clone());
        store.save(&file_config)?;
    }

    output::render_register_agent(&response, saved_token, settings, output_format)
}

/// Sets config after applying domain validation.
pub(crate) fn set_config(
    key: ConfigKey,
    value: Option<&str>,
    read_stdin: bool,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    settings: &ResolvedSettings,
    input: &CommandInput,
) -> Result<String> {
    let mut config = store.load()?;
    let updated_key = match key {
        ConfigKey::ApiBaseUrl => {
            if read_stdin {
                bail!("api-base-url is not a secret; pass it as a positional value");
            }
            let value = value.context("api-base-url requires a value")?;
            config.api_base_url = Some(
                ApiBaseUrl::try_new_with_policy(value, settings.allow_insecure_remote_http)?
                    .to_string(),
            );
            "api_base_url"
        }
        ConfigKey::Token => {
            let token = read_secret_config_value(value, read_stdin, "token", input)?;
            let token = token.trim();
            if token.is_empty() {
                bail!("token must not be empty");
            }
            config.token = Some(token.to_string());
            "token"
        }
        ConfigKey::CreatorApiToken => {
            let token = read_secret_config_value(value, read_stdin, "creator_api_token", input)?;
            let token = token.trim();
            if token.is_empty() {
                bail!("creator_api_token must not be empty");
            }
            config.creator_api_token = Some(token.to_string());
            "creator_api_token"
        }
    };
    store.save(&config)?;
    output::render_config_set(updated_key, settings, output_format)
}

fn read_secret_config_value(
    value: Option<&str>,
    read_stdin: bool,
    key_name: &str,
    input: &CommandInput,
) -> Result<String> {
    if value.is_some() {
        bail!("{key_name} is secret; pass it with --stdin instead of argv");
    }
    if !read_stdin {
        bail!("{key_name} is secret; pass --stdin and pipe the value on stdin");
    }
    Ok(input
        .read_to_string(key_name)?
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

fn parse_model_info(raw: &str) -> Result<serde_json::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).context("--model-info-json must be valid JSON")
}
