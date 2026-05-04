use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use shared::models::challenge::{BenchmarkTargetSpec, ChallengeDetailResponse};
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
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_id).await?;
    let target_ids = select_benchmark_targets(
        &challenge,
        args.target.as_deref(),
        args.all_targets,
        TargetSelectionMode::Official,
    )?;

    let package = package::package_solution_workspace(&args.dir)?;
    let mut responses = Vec::with_capacity(target_ids.len());
    for target_id in target_ids {
        let request = create_solution_submission_request(
            args.challenge_id.clone(),
            target_id,
            &package,
            args.explanation.clone(),
            args.parent_solution_submission_id.clone(),
            args.credit_text.clone(),
        );
        responses.push(client.create_solution_submission(&request).await?);
    }

    output::render_create_solution_submission_batch(&responses, &package, output_format)
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
    let target_ids = select_benchmark_targets(
        &challenge,
        args.target.as_deref(),
        args.all_targets,
        TargetSelectionMode::Validation,
    )?;

    let package = package::package_solution_workspace(&args.dir)?;
    let mut responses = Vec::with_capacity(target_ids.len());
    for target_id in target_ids {
        let request = create_solution_submission_request(
            args.challenge_id.clone(),
            target_id,
            &package,
            args.explanation.clone(),
            args.parent_solution_submission_id.clone(),
            args.credit_text.clone(),
        );
        responses.push(client.create_validation_run(&request).await?);
    }
    if args.no_wait {
        return output::render_create_validation_run_batch(&responses, &package, output_format);
    }

    let mut final_responses = Vec::with_capacity(responses.len());
    for response in responses {
        final_responses.push(
            poll_validation_run(
                &client,
                &response.id,
                Duration::from_millis(args.poll_interval_ms.max(1)),
                Duration::from_secs(args.timeout_sec),
            )
            .await?,
        );
    }
    output::render_validation_run_status_batch(&final_responses, output_format)
}

fn parse_model_info(raw: &str) -> Result<serde_json::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).context("--model-info-json must be valid JSON")
}

fn create_solution_submission_request(
    challenge_id: String,
    benchmark_target_id: String,
    package: &package::SolutionPackage,
    explanation: String,
    parent_solution_submission_id: Option<String>,
    credit_text: String,
) -> CreateSolutionSubmissionRequest {
    CreateSolutionSubmissionRequest {
        challenge_id,
        benchmark_target_id,
        artifact_base64: STANDARD.encode(&package.bytes),
        explanation,
        parent_solution_submission_id,
        credit_text,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetSelectionMode {
    Official,
    Validation,
}

fn select_benchmark_targets(
    challenge: &ChallengeDetailResponse,
    requested_target: Option<&str>,
    all_targets: bool,
    mode: TargetSelectionMode,
) -> Result<Vec<String>> {
    if all_targets {
        let targets = challenge.spec.benchmark_targets.iter().collect::<Vec<_>>();
        validate_selected_targets(challenge, &targets, mode)?;
        return Ok(targets.iter().map(|target| target.id.clone()).collect());
    }

    if let Some(target_id) = requested_target
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let target = challenge.spec.benchmark_target(target_id).ok_or_else(|| {
            anyhow::anyhow!(
                "challenge `{}` does not support benchmark target `{target_id}`",
                challenge.id
            )
        })?;
        validate_selected_targets(challenge, &[target], mode)?;
        return Ok(vec![target.id.clone()]);
    }

    match challenge.spec.benchmark_targets.as_slice() {
        [target] => {
            validate_selected_targets(challenge, &[target], mode)?;
            Ok(vec![target.id.clone()])
        }
        [] => bail!(
            "challenge `{}` does not declare any benchmark targets",
            challenge.id
        ),
        targets => {
            let available = targets
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "benchmark target is required for challenge `{}`; pass --target <target-id> or --all-targets. Available targets: {available}",
                challenge.id
            )
        }
    }
}

fn validate_selected_targets(
    challenge: &ChallengeDetailResponse,
    targets: &[&BenchmarkTargetSpec],
    mode: TargetSelectionMode,
) -> Result<()> {
    if mode != TargetSelectionMode::Validation {
        return Ok(());
    }

    let disabled = targets
        .iter()
        .filter(|target| !target.validation_enabled)
        .map(|target| target.id.as_str())
        .collect::<Vec<_>>();
    if disabled.is_empty() {
        return Ok(());
    }

    bail!(
        "validation pass is disabled for challenge `{}` target(s): {}; submit officially or ask the challenge owner to enable validation",
        challenge.id,
        disabled.join(", ")
    )
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
