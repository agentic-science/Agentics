//! Docker-backed `zip_project` evaluation runner.
//!
//! v0.2 uses one build solution container for setup/build, fresh no-egress run
//! solution containers that mount the build workspace read-only for benchmark
//! invocations, and a separate evaluator container. Run containers receive only the
//! current invocation's input files, while evaluator-only reference data stays in
//! the evaluator container.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bollard::Docker;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::challenge::{
    ChallengeBundleSpec, ChallengePrepareSpec, ChallengeRunManifest, DockerPlatform,
    MetricSchemaSpec, ResourceProfileSpec, TargetAccelerator,
};
use crate::models::evaluation::{EvaluationJobPayload, EvaluatorRunResult, ScoringMode};
use crate::models::paths::BundleRelativePath;
use crate::storage::{Storage, StorageKey};
use crate::zip_project::{
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, ZipProjectPhaseLimits, ZipProjectPhaseName,
    ZipProjectResolvedPhase,
};

mod docker;
mod errors;
mod filesystem;
mod logs;
mod run_io;
mod storage;
#[cfg(test)]
mod tests;

pub use docker::{
    RunnerContainerCleanupSummary, connect_docker, reconcile_runner_containers,
    remove_stale_local_validation_containers, remove_stopped_runner_containers,
};

use docker::{ContainerOutcome, ContainerRequest, bind_mount, pre_pull_image, run_container};
use errors::{ensure_container_succeeded, ensure_prepare_succeeded};
use filesystem::{
    OutputTreeLimits, cleanup_paths, copy_dir_all, ensure_disk_limit, ensure_prepare_disk_limit,
    extract_zip_safe, validate_evaluator_visible_output_tree,
};
use logs::{
    EVALUATION_LOG_BYTES_PER_RUN, EvaluationLogs, append_named_logs, append_phase_logs,
    append_run_logs, include_log_excerpts, phase_name, visible_log_content,
};
use run_io::{
    copy_evaluator_visible_run_tree, ensure_declared_outputs_exist, make_container_readable_tree,
    make_container_writable_tree, materialize_run_io, run_alias, run_interface, write_run_metadata,
};
use storage::{RunnerStorage, WritableMountLease, WritablePhase};

const RUNNER_KIND_LABEL: &str = "agentics.runner";
const RUNNER_KIND_ZIP_PROJECT: &str = "zip_project";
const RUNNER_SCOPE_LABEL: &str = "agentics.runner_scope";
const RUNNER_SCOPE_HOSTED_WORKER: &str = "hosted-worker";
const RUNNER_SCOPE_LOCAL_VALIDATION: &str = "local-validation";

/// Validated evaluator result plus the persisted runner log location.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Parsed and completed `result.json` emitted by the evaluator.
    pub result: EvaluatorRunResult,
    /// Storage-relative path to stdout and stderr captured from runner containers.
    pub log_key: StorageKey,
}

#[derive(Clone, Copy)]
/// Carries runner context data across this module boundary.
struct RunnerContext<'a> {
    docker: &'a Docker,
    storage: &'a RunnerStorage,
    job_id: &'a str,
    attempt: &'a RunnerAttempt,
    container_scope: RunnerContainerScope,
}

impl RunnerContext<'_> {
    /// Build Docker labels that identify one runner container and its slot owner.
    fn container_labels(
        self,
        phase: &str,
        writable_mount: Option<&WritableMountLease>,
    ) -> HashMap<String, String> {
        let mut labels = HashMap::from([
            ("agentics.job_id".to_string(), self.job_id.to_string()),
            (
                "agentics.worker_id".to_string(),
                self.attempt.worker_id.clone(),
            ),
            (
                "agentics.attempt_count".to_string(),
                self.attempt.attempt_count.to_string(),
            ),
            (
                RUNNER_SCOPE_LABEL.to_string(),
                self.container_scope.as_label().to_string(),
            ),
            ("agentics.phase".to_string(), phase.to_string()),
        ]);
        if let Some(writable_mount) = writable_mount {
            labels.extend(writable_mount.docker_labels());
        }
        labels
    }
}

/// Identifies one concrete execution attempt for transient runner resources.
struct RunnerAttempt {
    worker_id: String,
    attempt_count: i32,
    transient_name: String,
}

impl RunnerAttempt {
    /// Build an attempt identity safe for Docker names and temporary paths.
    fn new(job_id: &str, worker_id: &str, attempt_count: i32) -> Self {
        let nonce = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>();
        Self {
            worker_id: sanitize_name_component(worker_id),
            attempt_count,
            transient_name: format!(
                "{}-attempt-{}-{}",
                sanitize_name_component(job_id),
                attempt_count,
                nonce
            ),
        }
    }
}

/// Keeps a retained runner tree alive when it is backed by a bounded slot lease.
struct RetainedRunnerTree {
    path: PathBuf,
    _lease: Option<WritableMountLease>,
}

impl RetainedRunnerTree {
    /// Return the host path used for subsequent read-only mounts.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Build a retained tree from an existing runtime path.
    fn runtime_path(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            _lease: None,
        }
    }

    /// Build a retained tree that keeps its writable mount lease alive.
    fn leased(lease: WritableMountLease) -> Self {
        let path = lease.path().to_path_buf();
        Self {
            path,
            _lease: Some(lease),
        }
    }
}

/// Keeps one evaluator-visible run tree alive until the evaluator finishes.
struct RetainedRunTree {
    run_name: String,
    tree: RetainedRunnerTree,
}

/// Carries solution run request data across this module boundary.
struct SolutionRunRequest<'a> {
    eval_type: ScoringMode,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    manifest: &'a ZipProjectManifest,
    run_manifest: &'a ChallengeRunManifest,
    input_source_root: &'a Path,
    build_root: &'a RetainedRunnerTree,
    run_work_root: &'a Path,
    runs_root: &'a Path,
    output_limits: OutputTreeLimits,
}

#[derive(Clone, Copy)]
/// Carries setup build request data across this module boundary.
struct SetupBuildRequest<'a> {
    eval_type: ScoringMode,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    manifest: &'a ZipProjectManifest,
    source_root: &'a Path,
    build_root: &'a Path,
}

/// Carries evaluator request data across this module boundary.
struct EvaluatorRequest<'a> {
    eval_type: ScoringMode,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    run_manifest_container_path: &'a str,
    bundle_dir: &'a Path,
    prepared_root: Option<&'a Path>,
    runs_root: &'a Path,
    retained_run_trees: &'a [RetainedRunTree],
    evaluator_output_root: &'a Path,
}

/// Carries resolved run plan data across this module boundary.
struct ResolvedRunPlan {
    manifest: ChallengeRunManifest,
    input_source_root: PathBuf,
    run_manifest_container_path: String,
    prepared_root: Option<RetainedRunnerTree>,
}

/// Carries run plan request data across this module boundary.
struct RunPlanRequest<'a> {
    runner: RunnerContext<'a>,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    bundle_dir: &'a Path,
    prepared_root: &'a Path,
}

/// Platform-owned limits applied to one runner evaluation.
#[derive(Clone, Copy)]
struct EvaluationLimitConfig {
    max_runs: u64,
    max_result_json_bytes: u64,
    max_public_results: u64,
    max_result_log_bytes: u64,
}

/// Carries prepare request data across this module boundary.
struct PrepareRequest<'a> {
    runner: RunnerContext<'a>,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    prepare: &'a ChallengePrepareSpec,
    bundle_dir: &'a Path,
    prepared_root: &'a Path,
}

/// Docker label scope separating hosted worker containers from CLI local validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerContainerScope {
    HostedWorker,
    LocalValidation,
}

impl RunnerContainerScope {
    /// Stable Docker label value for this runner container scope.
    fn as_label(self) -> &'static str {
        match self {
            Self::HostedWorker => RUNNER_SCOPE_HOSTED_WORKER,
            Self::LocalValidation => RUNNER_SCOPE_LOCAL_VALIDATION,
        }
    }
}

/// Carries all boundary inputs required to execute one evaluation job.
pub struct EvaluationJobExecution<'a> {
    /// Docker client used for phase containers.
    pub docker: &'a Docker,
    /// Runtime configuration that constrains runner behavior.
    pub config: &'a Config,
    /// Persistent evaluation job identifier.
    pub job_id: &'a str,
    /// Worker instance identifier used to make attempts unique.
    pub worker_id: &'a str,
    /// One-based attempt count from the evaluation job record.
    pub attempt_count: i32,
    /// Docker cleanup scope for containers created by this execution.
    pub container_scope: RunnerContainerScope,
    /// Scoring mode that controls privacy and log exposure.
    pub eval_type: ScoringMode,
    /// Validated job payload produced by the API.
    pub payload: &'a EvaluationJobPayload,
    /// Durable artifact storage for inputs and bounded logs.
    pub storage: &'a dyn Storage,
}

impl std::fmt::Debug for EvaluationJobExecution<'_> {
    /// Formats the execution boundary without requiring service handles to be debuggable.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EvaluationJobExecution")
            .field("job_id", &self.job_id)
            .field("worker_id", &self.worker_id)
            .field("attempt_count", &self.attempt_count)
            .field("container_scope", &self.container_scope)
            .field("eval_type", &self.eval_type)
            .finish_non_exhaustive()
    }
}

/// Execute one evaluation job in Docker and return the validated evaluator result.
pub async fn execute_evaluation_job(
    request: EvaluationJobExecution<'_>,
) -> Result<ExecutionResult> {
    let EvaluationJobExecution {
        docker,
        config,
        job_id,
        worker_id,
        attempt_count,
        container_scope,
        eval_type,
        payload,
        storage,
    } = request;
    let attempt = RunnerAttempt::new(job_id, worker_id, attempt_count);
    let runner_runtime_root = config
        .runner_runtime_root()
        .map_err(|error| AppError::Runner(error.to_string()))?;
    let working_root = runner_runtime_root
        .join("agentics-eval-artifacts")
        .join(&attempt.transient_name);
    let source_root = working_root.join("source");
    let build_root = working_root.join("build-workspace");
    let run_work_root = working_root.join("solution-run-work");
    let runs_root = working_root.join("solution-runs");
    let prepared_root = working_root.join("prepared");
    let evaluator_output_root = working_root.join("evaluator-output");
    let challenge_bundle_root = working_root.join("challenge-bundle");
    let log_key = evaluation_runner_log_key(job_id, attempt_count)?;

    cleanup_paths([working_root.clone()]).await?;
    tokio::fs::create_dir_all(&working_root).await?;
    tokio::fs::create_dir_all(&source_root).await?;
    tokio::fs::create_dir_all(&build_root).await?;
    tokio::fs::create_dir_all(&run_work_root).await?;
    tokio::fs::create_dir_all(&runs_root).await?;
    tokio::fs::create_dir_all(&evaluator_output_root).await?;

    copy_dir_all(payload.bundle_path.as_path(), &challenge_bundle_root).await?;
    make_container_readable_tree(&challenge_bundle_root).await?;
    let bundle_dir = challenge_bundle_root.as_path();
    let spec = crate::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
    if config.requires_digest_pinned_images() {
        crate::challenge_bundle::validate_digest_pinned_images(&spec)?;
    }
    let result_path = evaluator_output_root.join(spec.execution.evaluator().result_file.as_path());
    let limits = EvaluationLimitConfig {
        max_runs: config.runner_max_runs,
        max_result_json_bytes: config.runner_max_result_json_bytes,
        max_public_results: config.runner_max_public_results,
        max_result_log_bytes: config.runner_max_result_log_bytes,
    };
    let max_log_bytes = EVALUATION_LOG_BYTES_PER_RUN
        .checked_mul(limits.max_runs)
        .ok_or_else(|| AppError::Runner("evaluation log limit overflow".to_string()))?;
    let mut logs = EvaluationLogs::new(max_log_bytes);
    let runner_storage = RunnerStorage::from_config(config)?;
    let output_limits = OutputTreeLimits {
        max_files: config.runner_max_output_files,
        max_dirs: config.runner_max_output_dirs,
        max_depth: config.runner_max_output_depth,
    };
    let runner_context = RunnerContext {
        docker,
        storage: &runner_storage,
        job_id,
        attempt: &attempt,
        container_scope,
    };

    let execution = async {
        let target = spec.target(&payload.target).ok_or_else(|| {
            AppError::Runner(format!(
                "challenge contract does not declare target `{}`",
                payload.target
            ))
        })?;
        let profile = &target.resource_profile;
        pre_pull_image(
            docker,
            profile.solution_image.docker_reference(),
            target.docker_platform,
        )
        .await?;
        pre_pull_image(
            docker,
            profile.evaluator_image.docker_reference(),
            target.docker_platform,
        )
        .await?;

        let artifact_bytes = storage.get(&payload.artifact_key).await?;
        let artifact_path = working_root.join("solution.zip");
        tokio::fs::write(&artifact_path, artifact_bytes).await?;
        extract_zip_safe(&artifact_path, &source_root).await?;
        let manifest = read_solution_manifest(&source_root, &spec).await?;
        let build_workspace = run_setup_and_build(
            runner_context,
            SetupBuildRequest {
                eval_type,
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                manifest: &manifest,
                source_root: &source_root,
                build_root: &build_root,
            },
            &mut logs,
        )
        .await?;

        let run_plan = resolve_run_plan(
            RunPlanRequest {
                runner: runner_context,
                spec: &spec,
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                target: target.name.as_str(),
                eval_type,
                bundle_dir,
                prepared_root: &prepared_root,
            },
            &mut logs,
        )
        .await?;
        configure_run_count_limits(&run_plan.manifest, limits, &mut logs)?;
        let retained_run_trees = run_solution_invocations(
            runner_context,
            SolutionRunRequest {
                eval_type,
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                manifest: &manifest,
                run_manifest: &run_plan.manifest,
                input_source_root: &run_plan.input_source_root,
                build_root: &build_workspace,
                run_work_root: &run_work_root,
                runs_root: &runs_root,
                output_limits,
            },
            &mut logs,
        )
        .await?;

        run_evaluator(
            runner_context,
            EvaluatorRequest {
                eval_type,
                spec: &spec,
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                run_manifest_container_path: &run_plan.run_manifest_container_path,
                bundle_dir,
                prepared_root: run_plan
                    .prepared_root
                    .as_ref()
                    .map(RetainedRunnerTree::path),
                runs_root: &runs_root,
                retained_run_trees: &retained_run_trees,
                evaluator_output_root: &evaluator_output_root,
            },
            &mut logs,
        )
        .await?;

        let result_raw =
            read_limited_result_json(&result_path, limits.max_result_json_bytes).await?;
        let mut result: EvaluatorRunResult = serde_json::from_str(&result_raw)
            .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
        validate_evaluator_result(&mut result, eval_type, &spec.metric_schema, limits)?;

        Ok(ExecutionResult {
            result,
            log_key: log_key.clone(),
        })
    }
    .await;

    let log_write = storage.put(&log_key, logs.as_bytes()).await;
    let cleanup = cleanup_paths([working_root]).await;
    match (execution, log_write, cleanup) {
        (Ok(result), Ok(_), Ok(())) => Ok(result),
        (Ok(_), Err(log_err), Ok(())) => Err(log_err),
        (Ok(_), Ok(_), Err(cleanup_err)) => Err(cleanup_err),
        (Ok(_), Err(log_err), Err(cleanup_err)) => Err(AppError::Runner(format!(
            "{log_err}; additionally failed to clean runner workspace: {cleanup_err}"
        ))),
        (Err(run_err), _, Ok(())) => Err(sanitize_runner_error(eval_type, run_err)),
        (Err(run_err), _, Err(cleanup_err)) => Err(AppError::Runner(format!(
            "{}; additionally failed to clean runner workspace: {cleanup_err}",
            sanitize_runner_error(eval_type, run_err)
        ))),
    }
}

/// Return the durable storage key used for one runner log.
pub fn evaluation_runner_log_key(job_id: &str, attempt_count: i32) -> Result<StorageKey> {
    StorageKey::try_new(format!(
        "eval-artifacts/{job_id}/attempt-{attempt_count}/runner.log"
    ))
}

/// Remove private official benchmark identifiers from runner errors crossing trust boundaries.
fn sanitize_runner_error(eval_type: ScoringMode, error: AppError) -> AppError {
    match eval_type {
        ScoringMode::Validation => error,
        ScoringMode::Official => AppError::Runner(
            "official evaluation failed; runner details are redacted for private benchmark execution"
                .to_string(),
        ),
    }
}

/// Reads solution manifest from disk or storage.
async fn read_solution_manifest(
    source_root: &Path,
    spec: &ChallengeBundleSpec,
) -> Result<ZipProjectManifest> {
    let manifest_path = source_root.join(spec.solution.manifest_file.as_path());
    let raw = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|e| {
            AppError::Validation(format!(
                "missing {ZIP_PROJECT_MANIFEST_FILE} in solution submission: {e}"
            ))
        })?;
    ZipProjectManifest::parse_json(&raw)
}

/// Handles run setup and build for this module.
async fn run_setup_and_build(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    if runner.storage.uses_bounded_slots() {
        return run_setup_and_build_bounded(runner, request, logs).await;
    }

    cleanup_paths([request.build_root.to_path_buf()]).await?;
    copy_dir_all(request.source_root, request.build_root).await?;
    make_container_writable_tree(request.build_root).await?;

    for phase in request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .filter(|phase| phase.name != ZipProjectPhaseName::Run)
    {
        let limits = effective_phase_limits(request.profile, &phase);
        let cmd = vec!["sh".to_string(), format!("/workspace/{}", phase.command)];
        let outcome = run_container(
            runner.docker,
            ContainerRequest {
                name: container_name(runner.attempt, &format!("{:?}", phase.name).to_lowercase()),
                image: request
                    .profile
                    .solution_image
                    .docker_reference()
                    .to_string(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(request.build_root, "/workspace", false)],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
                labels: runner.container_labels(phase_name(&phase.name), None),
            },
        )
        .await?;
        append_phase_logs(
            logs,
            phase.name,
            visible_log_content(request.eval_type, &outcome.logs),
        );
        ensure_container_succeeded(
            phase.name,
            &outcome,
            include_log_excerpts(request.eval_type),
        )?;
        ensure_disk_limit(request.build_root, limits.disk_limit_mb, phase.name).await?;
    }

    Ok(RetainedRunnerTree::runtime_path(request.build_root))
}

/// Handles run setup and build bounded for this module.
async fn run_setup_and_build_bounded(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let phases = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .filter(|phase| phase.name != ZipProjectPhaseName::Run)
        .collect::<Vec<_>>();

    if phases.is_empty() {
        replace_dir_all(request.source_root, request.build_root).await?;
        return Ok(RetainedRunnerTree::runtime_path(request.build_root));
    }

    let mut retained_workspace: Option<RetainedRunnerTree> = None;
    for phase in phases {
        let limits = effective_phase_limits(request.profile, &phase);
        let workspace = runner
            .storage
            .writable_mount(
                runner.docker,
                request.build_root,
                writable_phase_for_solution_phase(phase.name),
                limits.disk_limit_mb,
            )
            .await?;
        let source_workspace = retained_workspace
            .as_ref()
            .map(RetainedRunnerTree::path)
            .unwrap_or(request.source_root);
        copy_dir_all(source_workspace, workspace.path()).await?;
        make_container_writable_tree(workspace.path()).await?;

        let cmd = vec!["sh".to_string(), format!("/workspace/{}", phase.command)];
        let outcome = run_container(
            runner.docker,
            ContainerRequest {
                name: container_name(runner.attempt, &format!("{:?}", phase.name).to_lowercase()),
                image: request
                    .profile
                    .solution_image
                    .docker_reference()
                    .to_string(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(workspace.path(), "/workspace", false)],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
                labels: runner.container_labels(phase_name(&phase.name), Some(&workspace)),
            },
        )
        .await?;
        append_phase_logs(
            logs,
            phase.name,
            visible_log_content(request.eval_type, &outcome.logs),
        );
        ensure_container_succeeded(
            phase.name,
            &outcome,
            include_log_excerpts(request.eval_type),
        )?;
        ensure_disk_limit(workspace.path(), limits.disk_limit_mb, phase.name).await?;
        retained_workspace = Some(RetainedRunnerTree::leased(workspace));
    }

    retained_workspace.ok_or_else(|| {
        AppError::Internal("setup/build phase list unexpectedly ended empty".to_string())
    })
}

/// Handles run solution invocations for this module.
async fn run_solution_invocations(
    runner: RunnerContext<'_>,
    request: SolutionRunRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<Vec<RetainedRunTree>> {
    let run_phase = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .find(|phase| phase.name == ZipProjectPhaseName::Run)
        .ok_or_else(|| AppError::Runner("zip_project manifest has no run phase".to_string()))?;

    let mut retained_run_trees = Vec::with_capacity(request.run_manifest.runs.len());
    for (run_index, run) in request.run_manifest.runs.iter().enumerate() {
        let run_alias = run_alias(run_index)?;
        let solution_io_root = request.run_work_root.join(run_alias.as_str());
        let evaluator_run_root = request.runs_root.join(run.run_name.as_str());
        cleanup_paths([solution_io_root.clone(), evaluator_run_root.clone()]).await?;
        let limits = effective_phase_limits(request.profile, &run_phase);
        let io_mount = runner
            .storage
            .writable_mount(
                runner.docker,
                &solution_io_root,
                WritablePhase::SolutionRun,
                limits.disk_limit_mb,
            )
            .await?;
        let io_root = io_mount.path().to_path_buf();
        let input_dir = io_root.join("input");
        let output_dir = io_root.join("output");
        let tmp_dir = io_root.join("tmp");
        tokio::fs::create_dir_all(&input_dir).await?;
        tokio::fs::create_dir_all(&output_dir).await?;
        tokio::fs::create_dir_all(&tmp_dir).await?;
        materialize_run_io(
            run,
            run_alias.as_str(),
            request.eval_type,
            request.input_source_root,
            &io_root,
            &input_dir,
        )
        .await?;
        make_container_writable_tree(&io_root).await?;

        let outcome = run_container(
            runner.docker,
            ContainerRequest {
                name: container_name(runner.attempt, &format!("run-{run_alias}")),
                image: request.profile.solution_image.docker_reference().to_string(),
                cmd: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "mkdir -p /io/output /io/tmp; if [ -f /io/stdin.txt ]; then sh \"$1\" < /io/stdin.txt > /io/stdout.txt 2> /io/stderr.txt; else sh \"$1\" > /io/stdout.txt 2> /io/stderr.txt; fi"
                        .to_string(),
                    "agentics-run".to_string(),
                    format!("/workspace/{}", run_phase.command),
                ],
                env: vec![
                    "AGENTICS_PHASE=run".to_string(),
                    format!("AGENTICS_RUN_NAME={run_alias}"),
                    format!("AGENTICS_INTERFACE={}", run_interface(run.interface)),
                    "AGENTICS_INPUT_DIR=/io/input".to_string(),
                    "AGENTICS_OUTPUT_DIR=/io/output".to_string(),
                    "HOME=/io".to_string(),
                    "TMPDIR=/io/tmp".to_string(),
                    "PYTHONDONTWRITEBYTECODE=1".to_string(),
                ],
                mounts: vec![
                    bind_mount(request.build_root.path(), "/workspace", true),
                    bind_mount(&io_root, "/io", false),
                    bind_mount(&input_dir, "/io/input", true),
                ],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                accelerator_count: effective_accelerator_count(
                    request.profile,
                    request.accelerator,
                )?,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
                labels: runner.container_labels("run", Some(&io_mount)),
            },
        )
        .await?;
        append_run_logs(
            logs,
            run_alias.as_str(),
            visible_log_content(request.eval_type, &outcome.logs),
        );
        ensure_container_succeeded(
            ZipProjectPhaseName::Run,
            &outcome,
            include_log_excerpts(request.eval_type),
        )?;
        write_run_metadata(&io_root, run, run_alias.as_str(), &outcome).await?;
        ensure_disk_limit(&io_root, limits.disk_limit_mb, ZipProjectPhaseName::Run).await?;
        ensure_declared_outputs_exist(run, run_alias.as_str(), &output_dir).await?;
        if runner.storage.uses_bounded_slots() {
            validate_evaluator_visible_output_tree(
                &io_root,
                run_alias.as_str(),
                request.output_limits,
            )?;
            make_container_readable_tree(&io_root).await?;
            tokio::fs::create_dir_all(&evaluator_run_root).await?;
            retained_run_trees.push(RetainedRunTree {
                run_name: run.run_name.as_str().to_string(),
                tree: RetainedRunnerTree::leased(io_mount),
            });
        } else {
            copy_evaluator_visible_run_tree(
                &io_root,
                &evaluator_run_root,
                run_alias.as_str(),
                request.output_limits,
            )
            .await?;
            make_container_readable_tree(&evaluator_run_root).await?;
            cleanup_paths([solution_io_root]).await?;
        }
    }

    Ok(retained_run_trees)
}

/// Handles run evaluator for this module.
async fn run_evaluator(
    runner: RunnerContext<'_>,
    request: EvaluatorRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    make_container_readable_tree(request.bundle_dir).await?;
    make_container_readable_tree(request.runs_root).await?;
    let limits = evaluator_limits(request.profile);
    let output_mount = runner
        .storage
        .writable_mount(
            runner.docker,
            request.evaluator_output_root,
            WritablePhase::EvaluatorScore,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(output_mount.path()).await?;

    let mut cmd = request.spec.execution.evaluator().command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--solution-runs-dir".to_string(),
        "/solution-runs".to_string(),
        "--output-path".to_string(),
        format!("/output/{}", request.spec.execution.evaluator().result_file),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--runs-file".to_string(),
        request.run_manifest_container_path.to_string(),
    ]);

    let mut mounts = vec![
        bind_mount(request.bundle_dir, "/challenge", true),
        bind_mount(request.runs_root, "/solution-runs", true),
        bind_mount(output_mount.path(), "/output", false),
    ];
    for run_tree in request.retained_run_trees {
        mounts.push(bind_mount(
            run_tree.tree.path(),
            &format!("/solution-runs/{}", run_tree.run_name),
            true,
        ));
    }
    if let Some(prepared_root) = request.prepared_root {
        mounts.push(bind_mount(prepared_root, "/prepared", true));
    }
    let outcome = run_container(
        runner.docker,
        ContainerRequest {
            name: container_name(runner.attempt, "evaluator"),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec!["AGENTICS_PHASE=evaluator".to_string()],
            mounts,
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
            labels: runner.container_labels("evaluator", Some(&output_mount)),
        },
    )
    .await?;
    append_named_logs(
        logs,
        "evaluator",
        visible_log_content(request.eval_type, &outcome.logs),
    );
    if outcome.timed_out || outcome.exit_code != 0 {
        return Err(AppError::Runner(format!(
            "evaluator container failed: exit_code={}, timed_out={}",
            outcome.exit_code, outcome.timed_out
        )));
    }
    replace_dir_all_if_separate(output_mount.path(), request.evaluator_output_root).await?;

    Ok(())
}

/// Validates evaluator result invariants for this contract.
fn validate_evaluator_result(
    result: &mut EvaluatorRunResult,
    eval_type: ScoringMode,
    metric_schema: &MetricSchemaSpec,
    limits: EvaluationLimitConfig,
) -> Result<()> {
    result
        .validate_size_limits(limits.max_public_results, limits.max_result_log_bytes)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
    result
        .validate_for_mode(eval_type)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
    result
        .complete_metric_result(metric_schema, eval_type)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
    result.mode = Some(eval_type);
    Ok(())
}

/// Apply run-count limits and shrink log storage to the concrete run count.
fn configure_run_count_limits(
    run_manifest: &ChallengeRunManifest,
    limits: EvaluationLimitConfig,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    let run_count = u64::try_from(run_manifest.runs.len())
        .map_err(|_| AppError::Runner("run count exceeds supported range".to_string()))?;
    if run_count == 0 {
        return Err(AppError::Runner(
            "run manifest must declare at least one run".to_string(),
        ));
    }
    if run_count > limits.max_runs {
        return Err(AppError::Runner(format!(
            "run manifest exceeded runner run limit: {run_count} > {} runs",
            limits.max_runs
        )));
    }
    let log_limit = run_count
        .checked_mul(EVALUATION_LOG_BYTES_PER_RUN)
        .ok_or_else(|| AppError::Runner("evaluation log limit overflow".to_string()))?;
    logs.set_limit(log_limit);
    Ok(())
}

/// Read evaluator result JSON only after proving its raw byte size is bounded.
async fn read_limited_result_json(path: &Path, max_bytes: u64) -> Result<String> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|e| AppError::Runner(format!("missing result.json: {e}")))?;
    if !metadata.is_file() {
        return Err(AppError::Runner(
            "result.json is not a regular file".to_string(),
        ));
    }
    let size = metadata.len();
    if size > max_bytes {
        return Err(AppError::Runner(format!(
            "result.json exceeded size limit: {size} > {max_bytes} bytes"
        )));
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| AppError::Runner(format!("invalid result.json bytes: {e}")))
}

/// Enumerates run manifest source variants supported by this module.
enum RunManifestSource<'a> {
    Static(&'a BundleRelativePath),
    Prepared(&'a ChallengePrepareSpec),
}

/// Handles resolve run plan for this module.
async fn resolve_run_plan(
    request: RunPlanRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<ResolvedRunPlan> {
    match run_manifest_source(request.spec, request.eval_type)? {
        RunManifestSource::Static(manifest_path) => {
            let manifest = crate::challenge_bundle::read_challenge_run_manifest(
                request.bundle_dir,
                manifest_path,
            )
            .await?;
            Ok(ResolvedRunPlan {
                manifest,
                input_source_root: request.bundle_dir.to_path_buf(),
                run_manifest_container_path: format!("/challenge/{manifest_path}"),
                prepared_root: None,
            })
        }
        RunManifestSource::Prepared(prepare) => {
            let retained_prepared_root = run_prepare_phase(
                PrepareRequest {
                    runner: request.runner,
                    profile: request.profile,
                    docker_platform: request.docker_platform,
                    accelerator: request.accelerator,
                    target: request.target,
                    eval_type: request.eval_type,
                    prepare,
                    bundle_dir: request.bundle_dir,
                    prepared_root: request.prepared_root,
                },
                logs,
            )
            .await?;
            let manifest_path = retained_prepared_root
                .path()
                .join(prepare.result_runs_file.as_path());
            let manifest = crate::challenge_bundle::read_challenge_run_manifest_file(
                &manifest_path,
                &format!("prepared run manifest {}", manifest_path.display()),
            )
            .await?;
            crate::challenge_bundle::validate_challenge_run_manifest_sources(
                retained_prepared_root.path(),
                &manifest,
            )
            .await?;
            Ok(ResolvedRunPlan {
                manifest,
                input_source_root: retained_prepared_root.path().to_path_buf(),
                run_manifest_container_path: format!("/prepared/{}", prepare.result_runs_file),
                prepared_root: Some(retained_prepared_root),
            })
        }
    }
}

/// Handles run prepare phase for this module.
async fn run_prepare_phase(
    request: PrepareRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let limits = prepare_limits(request.profile, request.prepare);
    let prepared_mount = request
        .runner
        .storage
        .writable_mount(
            request.runner.docker,
            request.prepared_root,
            WritablePhase::EvaluatorPrepare,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(prepared_mount.path()).await?;
    let mut cmd = request.prepare.command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--prepared-dir".to_string(),
        "/prepared".to_string(),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--target".to_string(),
        request.target.to_string(),
        "--runs-file".to_string(),
        format!("/prepared/{}", request.prepare.result_runs_file),
    ]);

    let outcome = run_container(
        request.runner.docker,
        ContainerRequest {
            name: container_name(
                request.runner.attempt,
                &format!("prepare-{}", request.eval_type.evaluator_mode_arg()),
            ),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec![
                "AGENTICS_PHASE=prepare".to_string(),
                format!("AGENTICS_MODE={}", request.eval_type.evaluator_mode_arg()),
            ],
            mounts: vec![
                bind_mount(request.bundle_dir, "/challenge", true),
                bind_mount(prepared_mount.path(), "/prepared", false),
            ],
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: request.runner.storage.docker_layer_quota_mb(&limits),
            labels: request
                .runner
                .container_labels("prepare", Some(&prepared_mount)),
        },
    )
    .await?;
    append_named_logs(
        logs,
        &format!("prepare-{}", request.eval_type.evaluator_mode_arg()),
        visible_log_content(request.eval_type, &outcome.logs),
    );
    ensure_prepare_succeeded(&outcome, include_log_excerpts(request.eval_type))?;
    ensure_prepare_disk_limit(prepared_mount.path(), limits.disk_limit_mb).await?;
    make_container_readable_tree(prepared_mount.path()).await?;

    Ok(RetainedRunnerTree::leased(prepared_mount))
}

/// Handles run manifest source for this module.
fn run_manifest_source(
    spec: &ChallengeBundleSpec,
    eval_type: ScoringMode,
) -> Result<RunManifestSource<'_>> {
    match eval_type {
        ScoringMode::Validation => {
            if let Some(path) = spec.execution.validation_runs() {
                Ok(RunManifestSource::Static(path))
            } else if let Some(prepare) = spec.execution.validation_prepare() {
                Ok(RunManifestSource::Prepared(prepare))
            } else {
                Err(AppError::Runner(
                    "challenge does not declare validation runs or validation prepare".to_string(),
                ))
            }
        }
        ScoringMode::Official => {
            if let Some(path) = spec.execution.official_runs() {
                Ok(RunManifestSource::Static(path))
            } else if let Some(prepare) = spec.execution.official_prepare() {
                Ok(RunManifestSource::Prepared(prepare))
            } else {
                Err(AppError::Runner(
                    "challenge does not declare official runs or official prepare".to_string(),
                ))
            }
        }
    }
}

/// Return the enforced accelerator count for one container request.
fn effective_accelerator_count(
    profile: &ResourceProfileSpec,
    accelerator: TargetAccelerator,
) -> Result<Option<u32>> {
    match accelerator {
        TargetAccelerator::None => Ok(None),
        TargetAccelerator::Gpu => {
            let hardware = profile.hardware_metadata.as_ref().ok_or_else(|| {
                AppError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata".to_string(),
                )
            })?;
            let count = hardware.gpu_count.ok_or_else(|| {
                AppError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata.gpu_count"
                        .to_string(),
                )
            })?;
            if count == 0 {
                return Err(AppError::Runner(
                    "resource_profile.hardware_metadata.gpu_count must be greater than zero"
                        .to_string(),
                ));
            }
            Ok(Some(count))
        }
    }
}

/// Handles effective phase limits for this module.
fn effective_phase_limits(
    profile: &ResourceProfileSpec,
    phase: &ZipProjectResolvedPhase,
) -> ZipProjectPhaseLimits {
    let network_access = match phase.name {
        ZipProjectPhaseName::Setup => profile.setup_network_access,
        ZipProjectPhaseName::Build => profile.build_network_access,
        ZipProjectPhaseName::Run => profile.run_network_access,
    };
    ZipProjectPhaseLimits {
        timeout_sec: profile.timeout_sec,
        memory_limit_mb: profile.memory_limit_mb,
        cpu_limit_millis: profile.cpu_limit_millis,
        disk_limit_mb: profile.disk_limit_mb,
        network_access,
    }
}

/// Handles evaluator limits for this module.
fn evaluator_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    ZipProjectPhaseLimits {
        timeout_sec: profile.timeout_sec,
        memory_limit_mb: profile.memory_limit_mb,
        cpu_limit_millis: profile.cpu_limit_millis,
        disk_limit_mb: profile.disk_limit_mb,
        network_access: profile.evaluator_network_access,
    }
}

/// Handles prepare limits for this module.
fn prepare_limits(
    profile: &ResourceProfileSpec,
    prepare: &ChallengePrepareSpec,
) -> ZipProjectPhaseLimits {
    ZipProjectPhaseLimits {
        timeout_sec: profile.timeout_sec,
        memory_limit_mb: profile.memory_limit_mb,
        cpu_limit_millis: profile.cpu_limit_millis,
        disk_limit_mb: profile.disk_limit_mb,
        network_access: prepare.network_access,
    }
}

/// Handles replace dir all for this module.
async fn replace_dir_all(source: &Path, destination: &Path) -> Result<()> {
    cleanup_paths([destination.to_path_buf()]).await?;
    copy_dir_all(source, destination).await
}

/// Handles replace dir all if separate for this module.
async fn replace_dir_all_if_separate(source: &Path, destination: &Path) -> Result<()> {
    if source == destination {
        return Ok(());
    }
    replace_dir_all(source, destination).await
}

/// Handles writable phase for solution phase for this module.
fn writable_phase_for_solution_phase(phase: ZipProjectPhaseName) -> WritablePhase {
    match phase {
        ZipProjectPhaseName::Setup => WritablePhase::SolutionSetup,
        ZipProjectPhaseName::Build => WritablePhase::SolutionBuild,
        ZipProjectPhaseName::Run => WritablePhase::SolutionRun,
    }
}

/// Build a Docker container name for one attempt-local phase.
fn container_name(attempt: &RunnerAttempt, suffix: &str) -> String {
    let safe_suffix = sanitize_name_component(suffix);
    format!("agentics-{}-{safe_suffix}", attempt.transient_name)
}

/// Convert arbitrary identifiers into Docker-name-safe components.
fn sanitize_name_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
}
