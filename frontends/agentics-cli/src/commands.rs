use agentics_domain::models::pioneer_codes::{PioneerCode, PioneerCodeInput};
use agentics_domain::models::request::RegisterAgentRequest;
use anyhow::{Context, Result, bail};
use secrecy::{ExposeSecret, SecretString};

use crate::api::ApiClient;
use crate::cli::{self, ChallengeShortlistCommand, ConfigKey, RegisterArgs};
use crate::config::{ApiBaseUrl, CliConfig, ConfigStore, ResolvedSettings};
use crate::output;

mod admin;
mod local_validation;
mod submissions;

pub(crate) use admin::challenge_draft;
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
        owner: args.owner,
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
    value: &str,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    settings: &ResolvedSettings,
) -> Result<String> {
    let mut config = store.load()?;
    let updated_key = match key {
        ConfigKey::ApiBaseUrl => {
            config.api_base_url = Some(ApiBaseUrl::try_new(value)?.to_string());
            "api_base_url"
        }
        ConfigKey::Token => {
            let token = value.trim();
            if token.is_empty() {
                bail!("token must not be empty");
            }
            config.token = Some(token.to_string());
            "token"
        }
    };
    store.save(&config)?;
    output::render_config_set(updated_key, settings, output_format)
}

/// Handles challenge shortlist for this module.
pub(crate) fn challenge_shortlist(
    _command: ChallengeShortlistCommand,
    _output_format: cli::OutputFormat,
    _settings: &ResolvedSettings,
) -> Result<String> {
    bail!(
        "challenge shortlist commands require GitHub OAuth web-session support; use the creator web UI"
    )
}

fn parse_model_info(raw: &str) -> Result<serde_json::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).context("--model-info-json must be valid JSON")
}
