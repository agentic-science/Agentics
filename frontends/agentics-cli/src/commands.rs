use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use shared::models::request::{CreateSolutionSubmissionRequest, RegisterAgentRequest};

use crate::api::ApiClient;
use crate::cli::{self, ConfigKey, RegisterArgs, SubmitArgs, ValidateArgs};
use crate::config::{CliConfig, ConfigStore, ResolvedSettings, normalize_api_base_url};
use crate::{output, package};

pub async fn register(
    args: RegisterArgs,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    mut file_config: CliConfig,
    settings: &ResolvedSettings,
) -> Result<String> {
    let model_info = parse_model_info(&args.model_info_json)?;
    let request = RegisterAgentRequest {
        name: args.name,
        agent_description: args.agent_description,
        owner: args.owner,
        model_info,
    };

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let response = client.register(&request).await?;
    let saved_token = !args.no_save_token;
    if saved_token {
        file_config.api_base_url = Some(settings.api_base_url.clone());
        file_config.token = Some(response.token.clone());
        store.save(&file_config)?;
    }

    output::render_register_agent(&response, saved_token, settings, output_format)
}

pub fn set_config(
    key: ConfigKey,
    value: &str,
    output_format: cli::OutputFormat,
    store: &ConfigStore,
    settings: &ResolvedSettings,
) -> Result<String> {
    let mut config = store.load()?;
    let updated_key = match key {
        ConfigKey::ApiBaseUrl => {
            config.api_base_url = Some(normalize_api_base_url(value)?);
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

pub async fn submit(
    args: SubmitArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let package = package::package_solution_workspace(&args.dir)?;
    let request = create_solution_submission_request(
        args.challenge_id,
        &package,
        args.explanation,
        args.parent_solution_submission_id,
        args.credit_text,
    );

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let response = client.create_solution_submission(&request).await?;

    output::render_create_solution_submission(&response, &package, output_format)
}

pub async fn validate(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    if !args.remote {
        bail!("local validation is not implemented yet; pass --remote to use the Agentics API");
    }

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_id).await?;
    if !challenge.spec.datasets.validation_enabled {
        bail!(
            "validation pass is disabled for challenge `{}`; submit officially or ask the challenge owner to enable validation",
            challenge.id
        );
    }

    let package = package::package_solution_workspace(&args.dir)?;
    let request = create_solution_submission_request(
        args.challenge_id,
        &package,
        args.explanation,
        args.parent_solution_submission_id,
        args.credit_text,
    );

    let response = client.create_validation_run(&request).await?;
    if args.no_wait {
        return output::render_create_validation_run(&response, &package, output_format);
    }

    let final_response = poll_validation_run(
        &client,
        &response.id,
        Duration::from_millis(args.poll_interval_ms.max(1)),
        Duration::from_secs(args.timeout_sec),
    )
    .await?;
    output::render_validation_run_status(&final_response, output_format)
}

fn parse_model_info(raw: &str) -> Result<serde_json::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).context("--model-info-json must be valid JSON")
}

fn create_solution_submission_request(
    challenge_id: String,
    package: &package::SolutionPackage,
    explanation: String,
    parent_solution_submission_id: Option<String>,
    credit_text: String,
) -> CreateSolutionSubmissionRequest {
    CreateSolutionSubmissionRequest {
        challenge_id,
        artifact_base64: STANDARD.encode(&package.bytes),
        explanation,
        parent_solution_submission_id,
        credit_text,
    }
}

async fn poll_validation_run(
    client: &ApiClient,
    validation_run_id: &str,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<shared::models::request::SolutionSubmissionResponse> {
    let deadline = Instant::now() + timeout;
    loop {
        let response = client.get_validation_run(validation_run_id).await?;
        if is_terminal_status(&response.status) {
            return Ok(response);
        }
        if Instant::now() >= deadline {
            bail!("validation run {validation_run_id} did not finish within {timeout:?}");
        }
        tokio::time::sleep(poll_interval).await;
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed")
}
