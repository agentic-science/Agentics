use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use shared::config::Config;
use shared::models::challenge::{
    BenchmarkTargetSpec, ChallengeBundleSpec, ChallengeDetailResponse,
};
use shared::models::challenge_creation::{
    ChallengeCreationManifest, ChallengePrivateAssetKind, CreateChallengeDraftRequest,
    ReviewChallengeDraftRequest, UploadChallengePrivateAssetRequest, ValidateChallengeDraftRequest,
};
use shared::models::evaluation::{EvaluationJobPayload, ScoringMode};
use shared::models::request::CreateChallengeShortlistRevisionRequest;
use shared::models::request::{CreateSolutionSubmissionRequest, RegisterAgentRequest};
use shared::storage::{LocalStorage, Storage};

use crate::api::ApiClient;
use crate::cli::{
    self, AdminAuthArgs, ChallengeDraftCommand, ChallengePrivateAssetKindArg,
    ChallengeShortlistCommand, ConfigKey, RegisterArgs, SubmitArgs, ValidateArgs,
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

pub(crate) async fn challenge_shortlist(
    command: ChallengeShortlistCommand,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    match command {
        ChallengeShortlistCommand::Show { challenge_id } => {
            let response = client.get_challenge_shortlist(&challenge_id).await?;
            output::render_challenge_shortlist(&response, output_format)
        }
        ChallengeShortlistCommand::Upload { challenge_id, file } => {
            let raw = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read shortlist delta {}", file.display()))?;
            let request: CreateChallengeShortlistRevisionRequest = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse shortlist delta {}", file.display()))?;
            let response = client
                .create_challenge_shortlist_revision(&challenge_id, &request)
                .await?;
            output::render_challenge_shortlist_revision(&response, output_format)
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
    let target_ids = select_benchmark_targets(
        &challenge,
        args.target.as_deref(),
        args.all_targets,
        TargetSelectionMode::Official,
    )?;
    validate_parent_submission_scope(
        &client,
        &challenge.id,
        &target_ids,
        args.all_targets,
        args.parent_solution_submission_id.as_deref(),
    )
    .await?;

    let package = package::package_solution_workspace(&args.dir)?;
    let mut responses = Vec::with_capacity(target_ids.len());
    for target_id in target_ids {
        let request = create_solution_submission_request(
            challenge.id.clone(),
            target_id.clone(),
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
                    &target_id,
                    error,
                ));
            }
        }
    }

    output::render_create_solution_submission_batch(&responses, &package, output_format)
}

pub(crate) async fn validate(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    if args.remote {
        validate_remote(args, output_format, settings).await
    } else {
        validate_local(args, output_format).await
    }
}

async fn validate_remote(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
    settings: &ResolvedSettings,
) -> Result<String> {
    let client = ApiClient::new(&settings.api_base_url, settings.token.clone())?;
    let challenge = client.get_challenge(&args.challenge_id).await?;
    let target_ids = select_benchmark_targets(
        &challenge,
        args.target.as_deref(),
        args.all_targets,
        TargetSelectionMode::Validation,
    )?;
    validate_parent_submission_scope(
        &client,
        &challenge.id,
        &target_ids,
        args.all_targets,
        args.parent_solution_submission_id.as_deref(),
    )
    .await?;

    let package = package::package_solution_workspace(&args.dir)?;
    let mut responses = Vec::with_capacity(target_ids.len());
    for target_id in target_ids {
        let request = create_solution_submission_request(
            challenge.id.clone(),
            target_id.clone(),
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
                    &target_id,
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
                    &response.benchmark_target_id,
                    error,
                ));
            }
        }
    }
    output::render_validation_run_status_batch(&final_responses, output_format)
}

async fn validate_local(args: ValidateArgs, output_format: cli::OutputFormat) -> Result<String> {
    if args.no_wait {
        bail!("--no-wait can only be used with --remote validation");
    }
    if args
        .parent_solution_submission_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        bail!("--parent-solution-submission-id can only be used with --remote validation");
    }

    let bundle_dir = args
        .bundle_dir
        .as_deref()
        .context("--bundle-dir is required for local validation")?;
    let bundle_dir = canonical_dir(bundle_dir, "challenge bundle")?;
    let spec = shared::challenge_bundle::read_challenge_bundle_spec(&bundle_dir).await?;
    if spec.challenge_id != args.challenge_id {
        bail!(
            "local challenge bundle declares challenge `{}`, but command requested `{}`",
            spec.challenge_id,
            args.challenge_id
        );
    }

    let target_ids = select_benchmark_targets_from_spec(
        &spec.challenge_id,
        &spec,
        args.target.as_deref(),
        args.all_targets,
        TargetSelectionMode::Validation,
    )?;
    let package = package::package_solution_workspace(&args.dir)?;
    let storage_root = resolve_local_storage_dir(args.local_storage_dir.as_deref())?;
    tokio::fs::create_dir_all(&storage_root)
        .await
        .with_context(|| {
            format!(
                "failed to create local validation storage {}",
                storage_root.display()
            )
        })?;
    let storage_root = tokio::fs::canonicalize(&storage_root)
        .await
        .with_context(|| {
            format!(
                "failed to resolve local validation storage {}",
                storage_root.display()
            )
        })?;
    let storage_root_value = storage_root.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "local validation storage path is not valid UTF-8: {}",
            storage_root.display()
        )
    })?;

    let mut config = Config::from_env()?;
    config.storage_root = storage_root_value.to_string();
    config.validate_runner_storage()?;

    let docker = shared::runner::connect_docker(&config)?;
    let storage = LocalStorage::new(&storage_root);
    let package_report = output::LocalValidationPackageReport {
        workspace_dir: package.workspace_dir.clone(),
        file_count: package.file_count,
        uncompressed_bytes: package.uncompressed_bytes,
        zip_bytes: package.bytes.len(),
    };
    let mut target_reports = Vec::with_capacity(target_ids.len());
    for target_id in target_ids {
        let job_id = local_validation_job_id(&spec.challenge_id, &target_id)?;
        let artifact_path = storage
            .put(
                &format!("local-validation/{job_id}/solution.zip"),
                &package.bytes,
            )
            .await?;
        let payload = EvaluationJobPayload {
            artifact_path,
            bundle_path: bundle_dir.to_string_lossy().to_string(),
            challenge_id: spec.challenge_id.clone(),
            benchmark_target_id: target_id.clone(),
        };
        let log_path = storage_root.join(runner_log_key(&job_id));
        match shared::runner::execute_evaluation_job(
            &docker,
            &config,
            &job_id,
            ScoringMode::Validation,
            &payload,
            &storage,
        )
        .await
        {
            Ok(execution) => target_reports.push(output::LocalValidationTargetReport {
                benchmark_target_id: target_id,
                log_path,
                result: execution.result,
            }),
            Err(error) => {
                return Err(local_validation_error(
                    LocalValidationErrorContext {
                        challenge_id: &spec.challenge_id,
                        bundle_dir: &bundle_dir,
                        storage_root: &storage_root,
                        package: &package_report,
                        completed_targets: &target_reports,
                        output_format,
                        failed_target_id: &target_id,
                        log_path: &log_path,
                    },
                    error.into(),
                ));
            }
        }
    }

    let report = output::LocalValidationReport {
        challenge_id: spec.challenge_id,
        bundle_dir,
        storage_root,
        package: package_report,
        targets: target_reports,
    };
    output::render_local_validation_report(&report, output_format)
}

#[derive(Debug, Clone, Copy)]
struct LocalValidationErrorContext<'a> {
    challenge_id: &'a str,
    bundle_dir: &'a Path,
    storage_root: &'a Path,
    package: &'a output::LocalValidationPackageReport,
    completed_targets: &'a [output::LocalValidationTargetReport],
    output_format: cli::OutputFormat,
    failed_target_id: &'a str,
    log_path: &'a Path,
}

fn local_validation_error(
    context: LocalValidationErrorContext<'_>,
    error: anyhow::Error,
) -> anyhow::Error {
    let completed = if context.completed_targets.is_empty() {
        String::new()
    } else {
        let report = output::LocalValidationReport {
            challenge_id: context.challenge_id.to_string(),
            bundle_dir: context.bundle_dir.to_path_buf(),
            storage_root: context.storage_root.to_path_buf(),
            package: context.package.clone(),
            targets: context.completed_targets.to_vec(),
        };
        output::render_local_validation_report(&report, context.output_format)
            .map(|rendered| format!("{rendered}\n"))
            .unwrap_or_default()
    };
    anyhow::anyhow!(
        "{completed}local validation failed for target `{}`: {error}\nlog: {}",
        context.failed_target_id,
        context.log_path.display()
    )
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("failed to resolve {label} {}", path.display()))?;
    if !path.is_dir() {
        bail!("{label} is not a directory: {}", path.display());
    }
    Ok(path)
}

fn resolve_local_storage_dir(configured: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = configured {
        return Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .context("failed to read current directory")?
                .join(path)
        });
    }

    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine a local cache directory"))?;
    Ok(cache_dir.join("agentics").join("local-validation"))
}

fn local_validation_job_id(challenge_id: &str, target_id: &str) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?;
    Ok(format!(
        "local-{}-{}-{}-{}",
        sanitize_identifier_component(challenge_id),
        sanitize_identifier_component(target_id),
        std::process::id(),
        timestamp.as_nanos()
    ))
}

fn sanitize_identifier_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "item".to_string()
    } else {
        sanitized
    }
}

fn runner_log_key(job_id: &str) -> PathBuf {
    PathBuf::from("eval-artifacts")
        .join(job_id)
        .join("runner.log")
}

async fn validate_parent_submission_scope(
    client: &ApiClient,
    challenge_id: &str,
    target_ids: &[String],
    all_targets: bool,
    parent_solution_submission_id: Option<&str>,
) -> Result<()> {
    let Some(parent_solution_submission_id) = parent_solution_submission_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if all_targets {
        bail!("--parent-solution-submission-id cannot be used with --all-targets");
    }
    let [target_id] = target_ids else {
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
    if parent.challenge_id != challenge_id || parent.benchmark_target_id != target_id.as_str() {
        bail!(
            "parent solution submission `{parent_solution_submission_id}` must belong to challenge `{challenge_id}` target `{target_id}`"
        );
    }
    Ok(())
}

fn batch_error_with_created_ids(
    action: &str,
    responses: &[shared::models::request::CreateSolutionSubmissionResponse],
    package: Option<&package::SolutionPackage>,
    output_format: cli::OutputFormat,
    failed_target_id: &str,
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
        anyhow::anyhow!("{action} failed for target `{failed_target_id}`: {error}")
    } else {
        anyhow::anyhow!(
            "{created}\n{action} failed for target `{failed_target_id}` after creating the submissions above: {error}"
        )
    }
}

fn batch_status_error(
    responses: &[shared::models::request::SolutionSubmissionResponse],
    output_format: cli::OutputFormat,
    failed_target_id: &str,
    error: anyhow::Error,
) -> anyhow::Error {
    let completed =
        output::render_validation_run_status_batch(responses, output_format).unwrap_or_default();
    if completed.is_empty() {
        anyhow::anyhow!("validation polling failed for target `{failed_target_id}`: {error}")
    } else {
        anyhow::anyhow!(
            "{completed}\nvalidation polling failed for target `{failed_target_id}` after receiving the completed runs above: {error}"
        )
    }
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
    select_benchmark_targets_from_spec(
        &challenge.id,
        &challenge.spec,
        requested_target,
        all_targets,
        mode,
    )
}

fn select_benchmark_targets_from_spec(
    challenge_id: &str,
    spec: &ChallengeBundleSpec,
    requested_target: Option<&str>,
    all_targets: bool,
    mode: TargetSelectionMode,
) -> Result<Vec<String>> {
    if all_targets {
        let targets = spec.benchmark_targets.iter().collect::<Vec<_>>();
        validate_selected_targets(challenge_id, &targets, mode)?;
        return Ok(targets.iter().map(|target| target.id.clone()).collect());
    }

    if let Some(target_id) = requested_target
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let target = spec.benchmark_target(target_id).ok_or_else(|| {
            anyhow::anyhow!(
                "challenge `{}` does not support benchmark target `{target_id}`",
                challenge_id
            )
        })?;
        validate_selected_targets(challenge_id, &[target], mode)?;
        return Ok(vec![target.id.clone()]);
    }

    match spec.benchmark_targets.as_slice() {
        [] => bail!(
            "challenge `{}` does not declare any benchmark targets",
            challenge_id
        ),
        targets => {
            let available = targets
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "benchmark target is required for challenge `{}`; pass --target <target-id> or --all-targets. Available targets: {available}",
                challenge_id
            )
        }
    }
}

fn validate_selected_targets(
    challenge_id: &str,
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
        challenge_id,
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
