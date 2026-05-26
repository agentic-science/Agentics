use std::path::Path;
use std::time::{Duration, Instant};

use agentics_contracts::validation::targets::{self, TargetSelectionMode};
use agentics_domain::models::challenge::ChallengeDetailResponse;
use agentics_domain::models::challenge_creation::{
    ChallengePrivateAssetKind, ReviewChallengeDraftRequest, ValidateChallengeDraftRequest,
};
use agentics_domain::models::evaluation::SolutionSubmissionStatus;
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{ChallengeDraftId, SolutionSubmissionId};
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::pioneer_codes::{PioneerCode, PioneerCodeInput};
use agentics_domain::models::request::{
    CreateSolutionSubmissionRequest, RankingContextResponse, RegisterAgentRequest,
};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::{ExposeSecret, SecretString};

use crate::api::ApiClient;
use crate::cli::{
    self, AdminAuthArgs, ChallengeDraftCommand, ChallengePrivateAssetKindArg,
    ChallengeShortlistCommand, ConfigKey, RegisterArgs, SubmitArgs, ValidateArgs,
};
use crate::config::{ApiBaseUrl, CliConfig, ConfigStore, ResolvedSettings};
use crate::{output, package};

mod local_validation;

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

/// Handles challenge draft for this module.
pub(crate) async fn challenge_draft(
    command: ChallengeDraftCommand,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match command {
        ChallengeDraftCommand::Create { .. } => {
            bail!(
                "creator draft creation requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeDraftCommand::Status {
            draft_id: _draft_id,
        } => {
            bail!(
                "creator draft status requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeDraftCommand::UploadPrivateAsset { .. } => {
            bail!(
                "creator private asset upload requires GitHub OAuth web-session support; use the creator web UI"
            )
        }
        ChallengeDraftCommand::Validate {
            draft_id,
            repository_path,
            admin,
        } => {
            let admin_password = resolve_admin_password(&admin, settings)?;
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .validate_challenge_draft_admin(
                    &draft_id,
                    &ValidateChallengeDraftRequest { repository_path },
                    &admin.admin_username,
                    &admin_password,
                )
                .await?;
            output::render_challenge_draft(&response, output_format)
        }
        ChallengeDraftCommand::Approve {
            draft_id,
            expected_validation_bundle_sha256,
            message,
            admin,
        } => {
            review_draft(
                &client,
                output_format,
                DraftReviewRequest {
                    admin,
                    draft_id,
                    message,
                    expected_validation_bundle_sha256: Some(expected_validation_bundle_sha256),
                    action: DraftReviewAction::Approve,
                },
                settings,
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
                DraftReviewRequest {
                    admin,
                    draft_id,
                    message,
                    expected_validation_bundle_sha256: None,
                    action: DraftReviewAction::Reject,
                },
                settings,
            )
            .await
        }
        ChallengeDraftCommand::Publish {
            draft_id,
            repository_path,
            admin,
        } => {
            let admin_password = resolve_admin_password(&admin, settings)?;
            let repository_path = admin_repository_path_to_wire(&repository_path)?;
            let response = client
                .publish_challenge_draft_admin(
                    &draft_id,
                    &ValidateChallengeDraftRequest { repository_path },
                    &admin.admin_username,
                    &admin_password,
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
                DraftReviewRequest {
                    admin,
                    draft_id,
                    message,
                    expected_validation_bundle_sha256: None,
                    action: DraftReviewAction::Abandon,
                },
                settings,
            )
            .await
        }
        ChallengeDraftCommand::Cleanup { admin } => {
            let admin_password = resolve_admin_password(&admin, settings)?;
            let response = client
                .cleanup_challenge_drafts_admin(&admin.admin_username, &admin_password)
                .await?;
            output::render_challenge_draft_cleanup(&response, output_format)
        }
    }
}

/// Converts an admin repository path into the UTF-8 API wire contract.
fn admin_repository_path_to_wire(path: &Path) -> Result<String> {
    path.to_str().map(ToOwned::to_owned).with_context(|| {
        format!(
            "admin repository path `{}` is not valid UTF-8; pass a UTF-8 path",
            path.display()
        )
    })
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

/// Builds a challenge statistics view from public challenge result surfaces.
pub(crate) async fn challenge_stats(
    challenge_name: ChallengeName,
    target: TargetName,
    metric: Option<MetricName>,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&challenge_name).await?;
    if challenge.spec.target(&target).is_none() {
        bail!(
            "challenge `{}` does not support target `{target}`",
            challenge.challenge_name
        );
    }
    let metric_name = metric.unwrap_or_else(|| {
        challenge
            .spec
            .metric_schema
            .ranking
            .primary_metric_name
            .clone()
    });
    let leaderboard = client.get_leaderboard(&challenge_name, &target).await?;
    let distribution = client
        .get_score_distribution(&challenge_name, &target, &metric_name)
        .await?;
    let submissions = match client
        .list_public_solution_submissions(&challenge_name, &target, 20)
        .await
    {
        Ok(response) => Some(response),
        Err(error) if is_visibility_unavailable(&error) => None,
        Err(error) => return Err(error),
    };
    output::render_challenge_stats(
        &challenge,
        &leaderboard,
        &distribution,
        submissions.as_ref(),
        &metric_name,
        output_format,
    )
}

/// Lists visible public solution submissions for one challenge target.
pub(crate) async fn list_public_solution_submissions(
    challenge_name: ChallengeName,
    target: TargetName,
    limit: i64,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let response = client
        .list_public_solution_submissions(&challenge_name, &target, limit)
        .await?;
    output::render_public_solution_submission_list(
        &response,
        &challenge_name,
        &target,
        output_format,
    )
}

/// Fetches a detailed result report and ranking context for one solution submission.
pub(crate) async fn solution_submission_report(
    submission_id: SolutionSubmissionId,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let (report, owner_visible_report) = if settings.token_configured() {
        match client
            .get_solution_submission_result_report(&submission_id)
            .await
        {
            Ok(report) => (report, true),
            Err(error) if ApiClient::is_not_found(&error) => (
                client
                    .get_public_solution_submission_result_report(&submission_id)
                    .await?,
                false,
            ),
            Err(error) => return Err(error),
        }
    } else {
        (
            client
                .get_public_solution_submission_result_report(&submission_id)
                .await?,
            false,
        )
    };
    let solution_submission = &report.solution_submission;
    let ranking_context = if settings.token_configured() {
        match client
            .get_solution_submission_ranking_context(
                &submission_id,
                &solution_submission.challenge_name,
                &solution_submission.target,
            )
            .await
        {
            Ok(context) => Some(context),
            Err(error) if is_visibility_unavailable(&error) => {
                public_ranking_context_or_none(
                    &client,
                    &submission_id,
                    &solution_submission.challenge_name,
                    &solution_submission.target,
                )
                .await?
            }
            Err(error) => return Err(error),
        }
    } else {
        public_ranking_context_or_none(
            &client,
            &submission_id,
            &solution_submission.challenge_name,
            &solution_submission.target,
        )
        .await?
    };
    output::render_solution_submission_report(
        &report,
        ranking_context.as_ref(),
        owner_visible_report,
        output_format,
    )
}

/// Treats missing or forbidden public surfaces as unavailable optional context.
fn is_visibility_unavailable(error: &anyhow::Error) -> bool {
    ApiClient::is_not_found(error) || ApiClient::is_forbidden(error)
}

/// Fetches public ranking context when challenge visibility allows it.
async fn public_ranking_context_or_none(
    client: &ApiClient,
    submission_id: &SolutionSubmissionId,
    challenge_name: &ChallengeName,
    target: &TargetName,
) -> Result<Option<RankingContextResponse>> {
    match client
        .get_public_solution_submission_ranking_context(submission_id, challenge_name, target)
        .await
    {
        Ok(context) => Ok(Some(context)),
        Err(error) if is_visibility_unavailable(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Resolve the admin password from a non-argv source.
fn resolve_admin_password(
    admin: &AdminAuthArgs,
    settings: &ResolvedSettings,
) -> Result<SecretString> {
    let password = if admin.admin_password_stdin {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
            .context("failed to read admin password from stdin")?;
        SecretString::from(input.trim_end_matches(['\r', '\n']).to_string())
    } else {
        settings.admin_password.clone().unwrap_or_default()
    };
    if password.expose_secret().is_empty() {
        bail!("set AGENTICS_ADMIN_PASSWORD or pass --admin-password-stdin for admin commands");
    }
    Ok(password)
}

/// Enumerates draft review action variants supported by this module.
enum DraftReviewAction {
    Approve,
    Reject,
    Abandon,
}

/// Carries draft review inputs through the command handler.
struct DraftReviewRequest {
    admin: AdminAuthArgs,
    draft_id: ChallengeDraftId,
    message: String,
    expected_validation_bundle_sha256: Option<Sha256Digest>,
    action: DraftReviewAction,
}

/// Handles review draft for this module.
async fn review_draft(
    client: &ApiClient,
    output_format: cli::OutputFormat,
    review: DraftReviewRequest,
    settings: &ResolvedSettings,
) -> Result<String> {
    let request = ReviewChallengeDraftRequest {
        message: review.message,
        expected_validation_bundle_sha256: review.expected_validation_bundle_sha256,
    };
    let admin_password = resolve_admin_password(&review.admin, settings)?;
    let response = match review.action {
        DraftReviewAction::Approve => {
            client
                .approve_challenge_draft_admin(
                    &review.draft_id,
                    &request,
                    &review.admin.admin_username,
                    &admin_password,
                )
                .await?
        }
        DraftReviewAction::Reject => {
            client
                .reject_challenge_draft_admin(
                    &review.draft_id,
                    &request,
                    &review.admin.admin_username,
                    &admin_password,
                )
                .await?
        }
        DraftReviewAction::Abandon => {
            client
                .abandon_challenge_draft_admin(
                    &review.draft_id,
                    &request,
                    &review.admin.admin_username,
                    &admin_password,
                )
                .await?
        }
    };
    output::render_challenge_draft(&response, output_format)
}

impl From<ChallengePrivateAssetKindArg> for ChallengePrivateAssetKind {
    /// Handles from for this module.
    fn from(value: ChallengePrivateAssetKindArg) -> Self {
        match value {
            ChallengePrivateAssetKindArg::BenchmarkData => Self::PrivateBenchmarkData,
            ChallengePrivateAssetKindArg::EvaluatorPackage => Self::PrivateEvaluatorPackage,
            ChallengePrivateAssetKindArg::Seeds => Self::PrivateSeeds,
            ChallengePrivateAssetKindArg::ReferenceOutputs => Self::PrivateReferenceOutputs,
        }
    }
}

/// Handles submit for this module.
pub(crate) async fn submit(
    args: SubmitArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_name).await?;
    let targets = select_targets(
        &challenge,
        args.target.as_ref(),
        args.all_targets,
        TargetSelectionMode::Official,
    )?;
    validate_parent_submission_scope(
        &client,
        &challenge.challenge_name,
        &targets,
        args.all_targets,
        args.parent_solution_submission_id.as_ref(),
    )
    .await?;

    let package = package::package_solution_workspace(&args.dir)?;
    let mut responses = Vec::with_capacity(targets.len());
    for target in targets {
        let request = create_solution_submission_request(
            challenge.challenge_name.clone(),
            target.clone(),
            &package,
            args.explanation.clone(),
            args.parent_solution_submission_id.clone(),
            args.credit_text.clone(),
        );
        match client.create_solution_submission(&request).await {
            Ok(response) => responses.push(response),
            Err(error) => {
                return Err(batch_error_with_created_ids(
                    "submit",
                    &responses,
                    Some(&package),
                    output_format,
                    &target,
                    error,
                ));
            }
        }
    }

    output::render_create_solution_submission_batch(&responses, &package, output_format)
}

/// Handles validate for this module.
pub(crate) async fn validate(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    if args.remote {
        validate_remote(args, output_format, settings).await
    } else {
        local_validation::validate(args, output_format).await
    }
}

/// Validates remote invariants for this contract.
async fn validate_remote(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge_name = args
        .remote_challenge_name
        .clone()
        .context("--challenge-name is required for remote validation")?;
    let challenge = client.get_challenge(&challenge_name).await?;
    let targets = select_targets(
        &challenge,
        args.target.as_ref(),
        args.all_targets,
        TargetSelectionMode::Validation,
    )?;
    validate_parent_submission_scope(
        &client,
        &challenge.challenge_name,
        &targets,
        args.all_targets,
        args.parent_solution_submission_id.as_ref(),
    )
    .await?;

    let package = package::package_solution_workspace(&args.dir)?;
    let mut responses = Vec::with_capacity(targets.len());
    for target in targets {
        let request = create_solution_submission_request(
            challenge.challenge_name.clone(),
            target.clone(),
            &package,
            args.explanation.clone(),
            args.parent_solution_submission_id.clone(),
            args.credit_text.clone(),
        );
        match client.create_validation_run(&request).await {
            Ok(response) => responses.push(response),
            Err(error) => {
                return Err(batch_error_with_created_ids(
                    "validate",
                    &responses,
                    Some(&package),
                    output_format,
                    &target,
                    error,
                ));
            }
        }
    }
    if args.no_wait {
        return output::render_create_validation_run_batch(&responses, &package, output_format);
    }

    let mut final_responses = Vec::with_capacity(responses.len());
    for response in responses {
        match poll_validation_run(
            &client,
            &response.id,
            Duration::from_millis(args.poll_interval_ms.max(1)),
            Duration::from_secs(args.timeout_sec),
        )
        .await
        {
            Ok(final_response) => final_responses.push(final_response),
            Err(error) => {
                return Err(batch_status_error(
                    &final_responses,
                    output_format,
                    &response.target,
                    error,
                ));
            }
        }
    }
    output::render_validation_run_status_batch(&final_responses, output_format)
}

/// Validates parent submission scope invariants for this contract.
async fn validate_parent_submission_scope(
    client: &ApiClient,
    challenge_name: &ChallengeName,
    targets: &[TargetName],
    all_targets: bool,
    parent_solution_submission_id: Option<&SolutionSubmissionId>,
) -> Result<()> {
    let Some(parent_solution_submission_id) = parent_solution_submission_id else {
        return Ok(());
    };
    if all_targets {
        bail!("--parent-solution-submission-id cannot be used with --all-targets");
    }
    let [target] = targets else {
        bail!("--parent-solution-submission-id requires exactly one selected target");
    };
    let parent = client
        .get_solution_submission(parent_solution_submission_id)
        .await
        .with_context(|| {
            format!(
                "failed to inspect parent solution submission `{parent_solution_submission_id}`"
            )
        })?;
    if &parent.challenge_name != challenge_name || parent.target != *target {
        bail!(
            "parent solution submission `{parent_solution_submission_id}` must belong to challenge_name `{challenge_name}` target `{target}`"
        );
    }
    Ok(())
}

/// Handles batch error with created ids for this module.
fn batch_error_with_created_ids(
    action: &str,
    responses: &[agentics_domain::models::request::CreateSolutionSubmissionResponse],
    package: Option<&package::SolutionPackage>,
    output_format: cli::OutputFormat,
    failed_target: &TargetName,
    error: anyhow::Error,
) -> anyhow::Error {
    let created = package
        .and_then(|package| {
            if action == "submit" {
                output::render_create_solution_submission_batch(responses, package, output_format)
                    .ok()
            } else {
                output::render_create_validation_run_batch(responses, package, output_format).ok()
            }
        })
        .unwrap_or_default();
    if created.is_empty() {
        anyhow::anyhow!("{action} failed for target `{failed_target}`: {error}")
    } else {
        anyhow::anyhow!(
            "{created}\n{action} failed for target `{failed_target}` after creating the submissions above: {error}"
        )
    }
}

/// Handles batch status error for this module.
fn batch_status_error(
    responses: &[agentics_domain::models::request::SolutionSubmissionResponse],
    output_format: cli::OutputFormat,
    failed_target: &TargetName,
    error: anyhow::Error,
) -> anyhow::Error {
    let completed =
        output::render_validation_run_status_batch(responses, output_format).unwrap_or_default();
    if completed.is_empty() {
        anyhow::anyhow!("validation polling failed for target `{failed_target}`: {error}")
    } else {
        anyhow::anyhow!(
            "{completed}\nvalidation polling failed for target `{failed_target}` after receiving the completed runs above: {error}"
        )
    }
}

/// Parses model info from an external boundary string.
fn parse_model_info(raw: &str) -> Result<serde_json::Value> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw).context("--model-info-json must be valid JSON")
}

/// Creates solution submission request after validating caller inputs.
fn create_solution_submission_request(
    challenge_name: ChallengeName,
    target: TargetName,
    package: &package::SolutionPackage,
    explanation: String,
    parent_solution_submission_id: Option<SolutionSubmissionId>,
    credit_text: String,
) -> CreateSolutionSubmissionRequest {
    CreateSolutionSubmissionRequest {
        challenge_name,
        target,
        artifact_base64: STANDARD.encode(&package.bytes),
        explanation,
        parent_solution_submission_id,
        credit_text,
    }
}

/// Handles select targets for this module.
fn select_targets(
    challenge: &ChallengeDetailResponse,
    requested_target: Option<&TargetName>,
    all_targets: bool,
    mode: TargetSelectionMode,
) -> Result<Vec<TargetName>> {
    targets::select_targets_from_spec(
        &challenge.challenge_name,
        &challenge.spec.targets,
        requested_target,
        all_targets,
        mode,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

/// Handles poll validation run for this module.
async fn poll_validation_run(
    client: &ApiClient,
    validation_run_id: &SolutionSubmissionId,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<agentics_domain::models::request::SolutionSubmissionResponse> {
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

/// Handles wait for solution submission for this module.
pub(crate) async fn wait_for_solution_submission(
    client: &ApiClient,
    solution_submission_id: &SolutionSubmissionId,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<agentics_domain::models::request::SolutionSubmissionResponse> {
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

/// Returns whether terminal status holds.
fn is_terminal_status(status: &SolutionSubmissionStatus) -> bool {
    matches!(
        status,
        SolutionSubmissionStatus::Completed | SolutionSubmissionStatus::Failed
    )
}
