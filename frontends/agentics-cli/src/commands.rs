use std::time::{Duration, Instant};

use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use shared::models::challenge::{BenchmarkTargetSpec, ChallengeDetailResponse};
use shared::models::challenge_creation::{
    ChallengeCreationManifest, ChallengePrivateAssetKind, CreateChallengeDraftRequest,
    ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest, ValidateChallengeDraftRequest,
};
use shared::models::request::{CreateSolutionSubmissionRequest, RegisterAgentRequest};

use crate::api::ApiClient;
use crate::cli::{
    self, AdminAuthArgs, ChallengeDraftCommand, ChallengePrivateAssetKindArg, ConfigKey,
    RegisterArgs, SubmitArgs, ValidateArgs,
};
use crate::config::{CliConfig, ConfigStore, ResolvedSettings, normalize_api_base_url};
use crate::{output, package};

pub(crate) async fn register(
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

pub(crate) async fn challenge_draft(
    command: ChallengeDraftCommand,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match command {
        ChallengeDraftCommand::Create {
            repo_url,
            pr_number,
            pr_url,
            commit_sha,
            repo_dir,
            challenge_path,
            pr_author_github_user_id,
        } => {
            let manifest = read_challenge_creation_manifest(&repo_dir, &challenge_path)?;
            let response = client
                .create_challenge_draft(&CreateChallengeDraftRequest {
                    repo_url,
                    pr_number,
                    pr_url,
                    commit_sha,
                    challenge_path,
                    pr_author_github_user_id,
                    manifest,
                })
                .await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::Status { draft_id } => {
            let response = client.get_challenge_draft(&draft_id).await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::UploadPrivateAsset {
            draft_id,
            asset_id,
            kind,
            file,
            required,
        } => {
            let bytes = std::fs::read(&file)
                .with_context(|| format!("failed to read private asset {}", file.display()))?;
            let response = client
                .upload_challenge_private_asset(
                    &draft_id,
                    &UploadChallengePrivateAssetRequest {
                        asset_id,
                        kind: kind.into(),
                        required,
                        asset_base64: STANDARD.encode(bytes),
                    },
                )
                .await?;
            output::render_challenge_private_asset(&response, output_format)
        }
        ChallengeDraftCommand::Validate {
            draft_id,
            repository_path,
            admin,
        } => {
            let response = client
                .validate_challenge_draft_admin(
                    &draft_id,
                    &ValidateChallengeDraftRequest {
                        repository_path: repository_path.to_string_lossy().to_string(),
                    },
                    &admin.admin_username,
                    &admin.admin_password,
                )
                .await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::Approve {
            draft_id,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                admin,
                draft_id,
                message,
                DraftReviewAction::Approve,
            )
            .await
        }
        ChallengeDraftCommand::Reject {
            draft_id,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                admin,
                draft_id,
                message,
                DraftReviewAction::Reject,
            )
            .await
        }
        ChallengeDraftCommand::Publish {
            draft_id,
            repository_path,
            admin,
        } => {
            let response = client
                .publish_challenge_draft_admin(
                    &draft_id,
                    &ValidateChallengeDraftRequest {
                        repository_path: repository_path.to_string_lossy().to_string(),
                    },
                    &admin.admin_username,
                    &admin.admin_password,
                )
                .await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::Abandon {
            draft_id,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                admin,
                draft_id,
                message,
                DraftReviewAction::Abandon,
            )
            .await
        }
        ChallengeDraftCommand::Cleanup { admin } => {
            let response = client
                .cleanup_challenge_drafts_admin(&admin.admin_username, &admin.admin_password)
                .await?;
            output::render_challenge_draft_cleanup(&response, output_format)
        }
    }
}

enum DraftReviewAction {
    Approve,
    Reject,
    Abandon,
}

async fn review_draft(
    client: &ApiClient,
    output_format: cli::OutputFormat,
    admin: AdminAuthArgs,
    draft_id: String,
    message: String,
    action: DraftReviewAction,
) -> Result<String> {
    let request = ReviewChallengeDraftRequest { message };
    let response = match action {
        DraftReviewAction::Approve => {
            client
                .approve_challenge_draft_admin(
                    &draft_id,
                    &request,
                    &admin.admin_username,
                    &admin.admin_password,
                )
                .await?
        }
        DraftReviewAction::Reject => {
            client
                .reject_challenge_draft_admin(
                    &draft_id,
                    &request,
                    &admin.admin_username,
                    &admin.admin_password,
                )
                .await?
        }
        DraftReviewAction::Abandon => {
            client
                .abandon_challenge_draft_admin(
                    &draft_id,
                    &request,
                    &admin.admin_username,
                    &admin.admin_password,
                )
                .await?
        }
    };
    output::render_challenge_draft(&response, output_format)
}

fn read_challenge_creation_manifest(
    repo_dir: &Path,
    challenge_path: &str,
) -> Result<ChallengeCreationManifest> {
    let path = repo_dir
        .join(challenge_path)
        .join(shared::models::challenge_creation::AGENTICS_CHALLENGE_MANIFEST_FILE);
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

impl From<ChallengePrivateAssetKindArg> for ChallengePrivateAssetKind {
    fn from(value: ChallengePrivateAssetKindArg) -> Self {
        match value {
            ChallengePrivateAssetKindArg::BenchmarkData => Self::PrivateBenchmarkData,
            ChallengePrivateAssetKindArg::ScorerPackage => Self::PrivateScorerPackage,
            ChallengePrivateAssetKindArg::Seeds => Self::PrivateSeeds,
            ChallengePrivateAssetKindArg::ReferenceOutputs => Self::PrivateReferenceOutputs,
        }
    }
}

pub(crate) async fn submit(
    args: SubmitArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_id).await?;
    validate_round_id(&challenge, &args.round)?;
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
            args.round.clone(),
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

pub(crate) async fn validate(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    if !args.remote {
        bail!("local validation is not implemented yet; pass --remote to use the Agentics API");
    }

    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_id).await?;
    validate_round_id(&challenge, &args.round)?;
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
            args.round.clone(),
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
    round_id: String,
    benchmark_target_id: String,
    package: &package::SolutionPackage,
    explanation: String,
    parent_solution_submission_id: Option<String>,
    credit_text: String,
) -> CreateSolutionSubmissionRequest {
    CreateSolutionSubmissionRequest {
        challenge_id,
        round_id,
        benchmark_target_id,
        artifact_base64: STANDARD.encode(&package.bytes),
        explanation,
        parent_solution_submission_id,
        credit_text,
    }
}

fn validate_round_id(challenge: &ChallengeDetailResponse, round_id: &str) -> Result<()> {
    let round_id = round_id.trim();
    if round_id.is_empty() {
        bail!("round id must not be empty");
    }
    if challenge.spec.round(round_id).is_some() {
        return Ok(());
    }
    let available = challenge
        .rounds
        .iter()
        .map(|round| round.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "challenge `{}` does not declare round `{round_id}`. Available rounds: {available}",
        challenge.id
    )
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
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("validation poll timeout is too large")?;
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

pub(crate) async fn wait_for_solution_submission(
    client: &ApiClient,
    solution_submission_id: &str,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<shared::models::request::SolutionSubmissionResponse> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("solution submission poll timeout is too large")?;
    loop {
        let response = client
            .get_solution_submission(solution_submission_id)
            .await?;
        if is_terminal_status(&response.status) {
            return Ok(response);
        }
        if Instant::now() >= deadline {
            bail!("solution submission {solution_submission_id} did not finish within {timeout:?}");
        }
        tokio::time::sleep(poll_interval).await;
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed")
}
