use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use agentics_config::Config;
use agentics_contracts::validation::targets::{self, TargetSelectionMode};
use agentics_domain::models::challenge::ChallengeBundleSpec;
use agentics_domain::models::evaluation::{EvaluationJobPayload, ScoringMode};
use agentics_domain::models::names::{ChallengeName, TargetName};
use agentics_storage::{
    LocalStorage, Storage, StorageKey, StorageWriteIntent, pack_directory_to_tar,
};
use anyhow::{Context, Result, bail};

use crate::cli::{self, ValidateArgs};
use crate::{output, package};

/// Validates local invariants for this contract.
pub(super) async fn validate(
    args: ValidateArgs,
    output_format: cli::OutputFormat,
) -> Result<String> {
    reject_remote_only_local_flags(&args)?;
    let context = prepare_local_validation(args).await?;
    let target_reports = execute_local_validation_targets(&context, output_format).await?;
    let report = output::LocalValidationReport {
        challenge_name: context.spec.challenge_name,
        bundle_dir: context.bundle_dir,
        storage_root: context.storage_root,
        package: context.package_report,
        targets: target_reports,
    };
    output::render_local_validation_report(&report, output_format)
}

fn reject_remote_only_local_flags(args: &ValidateArgs) -> Result<()> {
    if args.no_wait {
        bail!("--no-wait can only be used with --remote validation");
    }
    if args.parent_solution_submission_id.is_some() {
        bail!("--parent-solution-submission-id can only be used with --remote validation");
    }
    Ok(())
}

#[derive(Debug)]
struct LocalValidationContext {
    bundle_dir: PathBuf,
    spec: ChallengeBundleSpec,
    targets: Vec<TargetName>,
    package: package::SolutionPackage,
    package_report: output::LocalValidationPackageReport,
    storage_root: PathBuf,
    config: Config,
}

async fn prepare_local_validation(args: ValidateArgs) -> Result<LocalValidationContext> {
    let bundle_dir = args
        .bundle_dir
        .as_deref()
        .context("--bundle-dir is required for local validation")?;
    let bundle_dir = canonical_dir(bundle_dir, "challenge bundle")?;
    let spec =
        agentics_contracts::challenge_bundle::read_challenge_bundle_spec(&bundle_dir).await?;
    require_requested_challenge(&spec, args.challenge_name.as_ref())?;
    let targets = select_local_targets(&spec, args.target.as_ref(), args.all_targets)?;
    let package = package::package_solution_workspace(&args.dir)?;
    let storage_root = prepare_local_storage_root(args.local_storage_dir.as_deref()).await?;
    let config = local_runner_config(&storage_root)?;
    let package_report = local_package_report(&package);

    Ok(LocalValidationContext {
        bundle_dir,
        spec,
        targets,
        package,
        package_report,
        storage_root,
        config,
    })
}

fn require_requested_challenge(
    spec: &ChallengeBundleSpec,
    requested_challenge_name: Option<&ChallengeName>,
) -> Result<()> {
    let requested_challenge_name =
        requested_challenge_name.context("challenge_name is required for local validation")?;
    if &spec.challenge_name == requested_challenge_name {
        Ok(())
    } else {
        bail!(
            "local challenge bundle declares challenge `{}`, but command requested `{}`",
            spec.challenge_name,
            requested_challenge_name
        );
    }
}

fn select_local_targets(
    spec: &ChallengeBundleSpec,
    target: Option<&TargetName>,
    all_targets: bool,
) -> Result<Vec<TargetName>> {
    targets::select_targets_from_spec(
        &spec.challenge_name,
        &spec.targets,
        target,
        all_targets,
        TargetSelectionMode::Validation,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

async fn prepare_local_storage_root(configured: Option<&Path>) -> Result<PathBuf> {
    let storage_root = resolve_local_storage_dir(configured)?;
    tokio::fs::create_dir_all(&storage_root)
        .await
        .with_context(|| {
            format!(
                "failed to create local validation storage {}",
                storage_root.display()
            )
        })?;
    tokio::fs::canonicalize(&storage_root)
        .await
        .with_context(|| {
            format!(
                "failed to resolve local validation storage {}",
                storage_root.display()
            )
        })
}

fn local_runner_config(storage_root: &Path) -> Result<Config> {
    let storage_root_value = storage_root.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "local validation storage path is not valid UTF-8: {}",
            storage_root.display()
        )
    })?;

    let mut config = Config::from_env()?;
    config.storage.root = storage_root_value.to_string();
    config.storage.backend = agentics_config::StorageBackend::Local;
    config.storage.work_root = Some(storage_root.join("_work").to_string_lossy().to_string());
    config.validate_runner_storage()?;
    Ok(config)
}

fn local_package_report(
    package: &package::SolutionPackage,
) -> output::LocalValidationPackageReport {
    output::LocalValidationPackageReport {
        workspace_dir: package.workspace_dir.clone(),
        file_count: package.file_count,
        uncompressed_bytes: package.uncompressed_bytes,
        zip_bytes: package.bytes.len(),
    }
}

async fn execute_local_validation_targets(
    context: &LocalValidationContext,
    output_format: cli::OutputFormat,
) -> Result<Vec<output::LocalValidationTargetReport>> {
    let docker = agentics_runner::connect_docker(&context.config)?;
    agentics_runner::remove_stale_local_validation_containers(&docker, &context.config).await?;
    let storage = LocalStorage::new(&context.storage_root);
    let mut target_reports = Vec::with_capacity(context.targets.len());
    for target in &context.targets {
        let job_id = local_validation_job_id(&context.spec.challenge_name, target)?;
        let artifact_key = StorageKey::try_new(format!("local-validation/{job_id}/solution.zip"))?;
        let stored_artifact_key = storage
            .put(
                &artifact_key,
                &context.package.bytes,
                StorageWriteIntent::new(
                    "solution artifact ZIP",
                    agentics_contracts::zip_project::MAX_ZIP_PROJECT_ARTIFACT_BYTES,
                ),
            )
            .await?;
        let stored_bundle_key =
            pack_local_validation_public_bundle(context, &storage, &job_id).await?;
        let payload = EvaluationJobPayload {
            artifact_key: stored_artifact_key,
            bundle_key: stored_bundle_key.clone(),
            public_bundle_key: stored_bundle_key,
            challenge_name: context.spec.challenge_name.clone(),
            target: target.clone(),
        };
        let log_key = agentics_runner::evaluation_runner_log_key(&job_id, 1)?;
        let log_path = context.storage_root.join(log_key.as_str());
        match agentics_runner::execute_evaluation_job(agentics_runner::EvaluationJobExecution {
            docker: &docker,
            config: &context.config,
            job_id: &job_id,
            worker_id: "local-validation",
            attempt_count: 1,
            container_scope: agentics_runner::RunnerContainerScope::LocalValidation,
            eval_type: ScoringMode::Validation,
            payload: &payload,
            storage: &storage,
        })
        .await
        {
            Ok(execution) => {
                let primary_metric = agentics_domain::models::evaluation::MetricValue::find_by_name(
                    &execution.result.aggregate_metrics,
                    &context.spec.metric_schema.ranking.primary_metric_name,
                );
                target_reports.push(output::LocalValidationTargetReport {
                    target: target.clone(),
                    log_path,
                    primary_metric,
                    result: execution.result,
                })
            }
            Err(error) => {
                return Err(local_validation_error(
                    LocalValidationErrorContext {
                        challenge_name: &context.spec.challenge_name,
                        bundle_dir: &context.bundle_dir,
                        storage_root: &context.storage_root,
                        package: &context.package_report,
                        completed_targets: &target_reports,
                        output_format,
                        failed_target: target,
                        log_path: &log_path,
                    },
                    error.into(),
                ));
            }
        }
    }
    Ok(target_reports)
}

/// Store a public-only challenge bundle archive for local validation.
async fn pack_local_validation_public_bundle(
    context: &LocalValidationContext,
    storage: &LocalStorage,
    job_id: &str,
) -> Result<StorageKey> {
    let tmp_root = context.storage_root.join("_tmp");
    let public_bundle_dir = tmp_root.join(format!("{job_id}-public-bundle"));
    let bundle_archive_path = tmp_root.join(format!("{job_id}-public-bundle.tar"));
    drop(tokio::fs::remove_dir_all(&public_bundle_dir).await);
    drop(tokio::fs::remove_file(&bundle_archive_path).await);

    let bundle_copy = async {
        if context.spec.datasets.private_benchmark_enabled
            && let Some(private_benchmark_dir) = &context.spec.datasets.private_benchmark_dir
        {
            agentics_contracts::challenge_bundle::copy_challenge_bundle_dir_excluding(
                &context.bundle_dir,
                &public_bundle_dir,
                private_benchmark_dir.as_path(),
                true,
            )
            .await?;
        } else {
            agentics_contracts::challenge_bundle::copy_challenge_bundle_dir(
                &context.bundle_dir,
                &public_bundle_dir,
                true,
            )
            .await?;
        }

        pack_directory_to_tar(
            &public_bundle_dir,
            &bundle_archive_path,
            StorageWriteIntent::new(
                "challenge public bundle archive",
                context.config.storage.max_bundle_archive_bytes,
            ),
        )
        .await?;
        let bundle_key =
            StorageKey::try_new(format!("local-validation/{job_id}/public-bundle.tar"))?;
        Ok(storage
            .put_file(
                &bundle_key,
                &bundle_archive_path,
                StorageWriteIntent::new(
                    "challenge public bundle archive",
                    context.config.storage.max_bundle_archive_bytes,
                ),
            )
            .await?)
    }
    .await;

    drop(tokio::fs::remove_file(&bundle_archive_path).await);
    drop(tokio::fs::remove_dir_all(&public_bundle_dir).await);
    bundle_copy
}

#[derive(Debug, Clone, Copy)]
/// Carries local validation error context data across this module boundary.
struct LocalValidationErrorContext<'a> {
    challenge_name: &'a ChallengeName,
    bundle_dir: &'a Path,
    storage_root: &'a Path,
    package: &'a output::LocalValidationPackageReport,
    completed_targets: &'a [output::LocalValidationTargetReport],
    output_format: cli::OutputFormat,
    failed_target: &'a TargetName,
    log_path: &'a Path,
}

/// Handles local validation error for this module.
fn local_validation_error(
    context: LocalValidationErrorContext<'_>,
    error: anyhow::Error,
) -> anyhow::Error {
    let completed = if context.completed_targets.is_empty() {
        String::new()
    } else {
        let report = output::LocalValidationReport {
            challenge_name: context.challenge_name.clone(),
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
        context.failed_target,
        context.log_path.display()
    )
}

/// Handles canonical dir for this module.
fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("failed to resolve {label} {}", path.display()))?;
    if !path.is_dir() {
        bail!("{label} is not a directory: {}", path.display());
    }
    Ok(path)
}

/// Handles resolve local storage dir for this module.
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

/// Handles local validation job id for this module.
fn local_validation_job_id(challenge_name: &ChallengeName, target: &TargetName) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?;
    Ok(format!(
        "local-{}-{}-{}-{}",
        sanitize_identifier_component(challenge_name.as_str()),
        sanitize_identifier_component(target.as_str()),
        std::process::id(),
        timestamp.as_nanos()
    ))
}

/// Handles sanitize identifier component for this module.
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
