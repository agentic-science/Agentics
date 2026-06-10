use std::time::{Duration, Instant};

use agentics_contracts::validation::targets::{self, TargetSelectionMode};
use agentics_domain::models::challenge::ChallengeDetailResponse;
use agentics_domain::models::evaluation::SolutionSubmissionStatus;
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::names::{ChallengeName, MetricName, TargetName};
use agentics_domain::models::request::{CreateSolutionSubmissionRequest, RankingContextResponse};
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::api::ApiClient;
use crate::cli::{self, SubmitArgs, ValidateArgs};
use crate::config::ResolvedSettings;
use crate::{output, package};

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
    let challenge = client
        .get_challenge(&solution_submission.challenge_name)
        .await?;
    let primary_metric_name = &challenge.spec.metric_schema.ranking.primary_metric_name;
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
        Some(primary_metric_name),
    )
}

fn is_visibility_unavailable(error: &anyhow::Error) -> bool {
    ApiClient::is_not_found(error) || ApiClient::is_forbidden(error)
}

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
        super::local_validation::validate(args, output_format).await
    }
}

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
                    &challenge.spec.metric_schema.ranking.primary_metric_name,
                    &response.target,
                    error,
                ));
            }
        }
    }
    output::render_validation_run_status_batch(
        &final_responses,
        output_format,
        Some(&challenge.spec.metric_schema.ranking.primary_metric_name),
    )
}

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

fn batch_status_error(
    responses: &[agentics_domain::models::request::SolutionSubmissionResponse],
    output_format: cli::OutputFormat,
    primary_metric_name: &MetricName,
    failed_target: &TargetName,
    error: anyhow::Error,
) -> anyhow::Error {
    let completed = output::render_validation_run_status_batch(
        responses,
        output_format,
        Some(primary_metric_name),
    )
    .unwrap_or_default();
    if completed.is_empty() {
        anyhow::anyhow!("validation polling failed for target `{failed_target}`: {error}")
    } else {
        anyhow::anyhow!(
            "{completed}\nvalidation polling failed for target `{failed_target}` after receiving the completed runs above: {error}"
        )
    }
}

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

fn is_terminal_status(status: &SolutionSubmissionStatus) -> bool {
    matches!(
        status,
        SolutionSubmissionStatus::Completed | SolutionSubmissionStatus::Failed
    )
}
