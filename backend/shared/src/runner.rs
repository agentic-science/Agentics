//! Docker-backed `zip_project` evaluation runner.
//!
//! v0.2 uses one build solution container for setup/build, fresh no-egress run
//! solution containers that mount the build workspace read-only for benchmark
//! invocations, and a separate scorer container. Run containers receive only the
//! current invocation's input files, while scorer-only reference data stays in
//! the scorer container.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bollard::Docker;
use tokio::io::AsyncWriteExt;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::challenge::{
    ChallengeBundleSpec, ChallengePrepareSpec, ChallengeRunInputFile, ChallengeRunInterface,
    ChallengeRunManifest, ChallengeRunSpec, DockerPlatform, MetricSchemaSpec, ResourceProfileSpec,
    TargetAccelerator,
};
use crate::models::evaluation::{EvaluationJobPayload, ScorerRunResult, ScoringMode};
use crate::models::names::RunName;
use crate::models::paths::BundleRelativePath;
use crate::storage::{Storage, StorageKey};
use crate::zip_project::{
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, ZipProjectPhaseFailureReason,
    ZipProjectPhaseLimits, ZipProjectPhaseName, ZipProjectResolvedPhase,
};

mod docker;
mod errors;
mod filesystem;
mod logs;
mod storage;

pub use docker::{
    RunnerContainerCleanupSummary, connect_docker, reconcile_runner_containers,
    remove_stopped_runner_containers,
};

use docker::{ContainerOutcome, ContainerRequest, bind_mount, pre_pull_image, run_container};
use errors::{ensure_container_succeeded, ensure_prepare_succeeded, phase_error};
use filesystem::{
    OutputTreeLimits, cleanup_paths, copy_dir_all, ensure_disk_limit, ensure_prepare_disk_limit,
    extract_zip_safe, validate_scorer_visible_output_tree,
};
use logs::{
    EVALUATION_LOG_BYTES_PER_RUN, EvaluationLogs, append_named_logs, append_phase_logs,
    append_run_logs, include_log_excerpts, phase_name, visible_log_content,
};
use storage::{RunnerStorage, WritableMountLease, WritablePhase};

const RUNNER_KIND_LABEL: &str = "agentics.runner";
const RUNNER_KIND_ZIP_PROJECT: &str = "zip_project";
const RUNNER_SCOPE_LABEL: &str = "agentics.runner_scope";
const RUNNER_SCOPE_HOSTED_WORKER: &str = "hosted-worker";
const RUNNER_SCOPE_LOCAL_VALIDATION: &str = "local-validation";

/// Validated scorer result plus the persisted runner log location.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Parsed and normalized `result.json` emitted by the scorer.
    pub result: ScorerRunResult,
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
        Self {
            worker_id: sanitize_name_component(worker_id),
            attempt_count,
            transient_name: format!(
                "{}-attempt-{}",
                sanitize_name_component(job_id),
                attempt_count
            ),
        }
    }
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
    build_root: &'a Path,
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

/// Carries scorer request data across this module boundary.
struct ScorerRequest<'a> {
    eval_type: ScoringMode,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    run_manifest_container_path: &'a str,
    bundle_dir: &'a Path,
    prepared_root: Option<&'a Path>,
    runs_root: &'a Path,
    scorer_output_root: &'a Path,
}

/// Carries resolved run plan data across this module boundary.
struct ResolvedRunPlan {
    manifest: ChallengeRunManifest,
    input_source_root: PathBuf,
    run_manifest_container_path: String,
    prepared_root: Option<PathBuf>,
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

#[derive(Debug, serde::Serialize)]
/// Carries solution run metadata data across this module boundary.
struct SolutionRunMetadata {
    run_name: String,
    interface: ChallengeRunInterface,
    exit_code: i64,
    timed_out: bool,
    wall_time_ms: u64,
    stdout_path: String,
    stderr_path: String,
    output_dir: String,
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

/// Execute one evaluation job in Docker and return the validated scorer result.
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
    let working_root = std::env::temp_dir()
        .join("agentics-eval-artifacts")
        .join(&attempt.transient_name);
    let source_root = working_root.join("source");
    let build_root = working_root.join("build-workspace");
    let run_work_root = working_root.join("solution-run-work");
    let runs_root = working_root.join("solution-runs");
    let prepared_root = working_root.join("prepared");
    let scorer_output_root = working_root.join("scorer-output");
    let challenge_bundle_root = working_root.join("challenge-bundle");
    let log_key = evaluation_runner_log_key(job_id, attempt_count)?;

    cleanup_paths([working_root.clone()]).await?;
    tokio::fs::create_dir_all(&working_root).await?;
    tokio::fs::create_dir_all(&source_root).await?;
    tokio::fs::create_dir_all(&build_root).await?;
    tokio::fs::create_dir_all(&run_work_root).await?;
    tokio::fs::create_dir_all(&runs_root).await?;
    tokio::fs::create_dir_all(&scorer_output_root).await?;

    copy_dir_all(payload.bundle_path.as_path(), &challenge_bundle_root).await?;
    make_container_readable_tree(&challenge_bundle_root).await?;
    let bundle_dir = challenge_bundle_root.as_path();
    let spec = crate::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
    if config.require_digest_pinned_images {
        crate::challenge_bundle::validate_digest_pinned_images(&spec)?;
    }
    let result_path = scorer_output_root.join(spec.scorer.result_file.as_path());
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
            profile.scorer_image.docker_reference(),
            target.docker_platform,
        )
        .await?;

        let artifact_bytes = storage.get(&payload.artifact_key).await?;
        let artifact_path = working_root.join("solution.zip");
        tokio::fs::write(&artifact_path, artifact_bytes).await?;
        extract_zip_safe(&artifact_path, &source_root).await?;
        let manifest = read_solution_manifest(&source_root, &spec).await?;
        run_setup_and_build(
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
        run_solution_invocations(
            runner_context,
            SolutionRunRequest {
                eval_type,
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                manifest: &manifest,
                run_manifest: &run_plan.manifest,
                input_source_root: &run_plan.input_source_root,
                build_root: &build_root,
                run_work_root: &run_work_root,
                runs_root: &runs_root,
                output_limits,
            },
            &mut logs,
        )
        .await?;

        run_scorer(
            runner_context,
            ScorerRequest {
                eval_type,
                spec: &spec,
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                run_manifest_container_path: &run_plan.run_manifest_container_path,
                bundle_dir,
                prepared_root: run_plan.prepared_root.as_deref(),
                runs_root: &runs_root,
                scorer_output_root: &scorer_output_root,
            },
            &mut logs,
        )
        .await?;

        let result_raw =
            read_limited_result_json(&result_path, limits.max_result_json_bytes).await?;
        let mut result: ScorerRunResult = serde_json::from_str(&result_raw)
            .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
        validate_scorer_result(&mut result, eval_type, &spec.metric_schema, limits)?;

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
) -> Result<()> {
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

    Ok(())
}

/// Handles run setup and build bounded for this module.
async fn run_setup_and_build_bounded(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    let phases = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .filter(|phase| phase.name != ZipProjectPhaseName::Run)
        .collect::<Vec<_>>();

    if phases.is_empty() {
        replace_dir_all(request.source_root, request.build_root).await?;
        return Ok(());
    }

    let mut source_workspace = request.source_root.to_path_buf();
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
        copy_dir_all(&source_workspace, workspace.path()).await?;
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
        replace_dir_all(workspace.path(), request.build_root).await?;
        source_workspace = request.build_root.to_path_buf();
    }

    Ok(())
}

/// Handles run solution invocations for this module.
async fn run_solution_invocations(
    runner: RunnerContext<'_>,
    request: SolutionRunRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    let run_phase = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .find(|phase| phase.name == ZipProjectPhaseName::Run)
        .ok_or_else(|| AppError::Runner("zip_project manifest has no run phase".to_string()))?;

    for (run_index, run) in request.run_manifest.runs.iter().enumerate() {
        let run_alias = run_alias(run_index)?;
        let solution_io_root = request.run_work_root.join(run_alias.as_str());
        let scorer_run_root = request.runs_root.join(run.run_name.as_str());
        cleanup_paths([solution_io_root.clone(), scorer_run_root.clone()]).await?;
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
        let io_root = io_mount.path();
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
            io_root,
            &input_dir,
        )
        .await?;
        make_container_writable_tree(io_root).await?;

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
                    bind_mount(request.build_root, "/workspace", true),
                    bind_mount(io_root, "/io", false),
                    bind_mount(&input_dir, "/io/input", true),
                ],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
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
        write_run_metadata(io_root, run, run_alias.as_str(), &outcome).await?;
        ensure_disk_limit(io_root, limits.disk_limit_mb, ZipProjectPhaseName::Run).await?;
        ensure_declared_outputs_exist(run, run_alias.as_str(), &output_dir).await?;
        copy_scorer_visible_run_tree(
            io_root,
            &scorer_run_root,
            run_alias.as_str(),
            request.output_limits,
        )
        .await?;
        make_container_readable_tree(&scorer_run_root).await?;
        cleanup_paths([solution_io_root]).await?;
    }

    Ok(())
}

/// Handles run scorer for this module.
async fn run_scorer(
    runner: RunnerContext<'_>,
    request: ScorerRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<()> {
    make_container_readable_tree(request.bundle_dir).await?;
    make_container_readable_tree(request.runs_root).await?;
    let limits = scorer_limits(request.profile);
    let output_mount = runner
        .storage
        .writable_mount(
            runner.docker,
            request.scorer_output_root,
            WritablePhase::ScorerScore,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(output_mount.path()).await?;

    let mut cmd = request.spec.scorer.command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--solution-runs-dir".to_string(),
        "/solution-runs".to_string(),
        "--output-path".to_string(),
        format!("/output/{}", request.spec.scorer.result_file),
        "--mode".to_string(),
        request.eval_type.scorer_mode_arg().to_string(),
        "--runs-file".to_string(),
        request.run_manifest_container_path.to_string(),
    ]);

    let mut mounts = vec![
        bind_mount(request.bundle_dir, "/challenge", true),
        bind_mount(request.runs_root, "/solution-runs", true),
        bind_mount(output_mount.path(), "/output", false),
    ];
    if let Some(prepared_root) = request.prepared_root {
        mounts.push(bind_mount(prepared_root, "/prepared", true));
    }
    let outcome = run_container(
        runner.docker,
        ContainerRequest {
            name: container_name(runner.attempt, "scorer"),
            image: request.profile.scorer_image.docker_reference().to_string(),
            cmd,
            env: vec!["AGENTICS_PHASE=scorer".to_string()],
            mounts,
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            limits: limits.clone(),
            docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
            labels: runner.container_labels("scorer", Some(&output_mount)),
        },
    )
    .await?;
    append_named_logs(
        logs,
        "scorer",
        visible_log_content(request.eval_type, &outcome.logs),
    );
    if outcome.timed_out || outcome.exit_code != 0 {
        return Err(AppError::Runner(format!(
            "scorer container failed: exit_code={}, timed_out={}",
            outcome.exit_code, outcome.timed_out
        )));
    }
    replace_dir_all_if_separate(output_mount.path(), request.scorer_output_root).await?;

    Ok(())
}

/// Validates scorer result invariants for this contract.
fn validate_scorer_result(
    result: &mut ScorerRunResult,
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
        .normalize_metrics(metric_schema, eval_type)
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

/// Read scorer result JSON only after proving its raw byte size is bounded.
async fn read_limited_result_json(path: &Path, max_bytes: u64) -> Result<String> {
    let metadata = tokio::fs::metadata(path)
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
            run_prepare_phase(
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
            let manifest_path = request
                .prepared_root
                .join(prepare.result_runs_file.as_path());
            let manifest = crate::challenge_bundle::read_challenge_run_manifest_file(
                &manifest_path,
                &format!("prepared run manifest {}", manifest_path.display()),
            )
            .await?;
            crate::challenge_bundle::validate_challenge_run_manifest_sources(
                request.prepared_root,
                &manifest,
            )
            .await?;
            Ok(ResolvedRunPlan {
                manifest,
                input_source_root: request.prepared_root.to_path_buf(),
                run_manifest_container_path: format!("/prepared/{}", prepare.result_runs_file),
                prepared_root: Some(request.prepared_root.to_path_buf()),
            })
        }
    }
}

/// Handles run prepare phase for this module.
async fn run_prepare_phase(request: PrepareRequest<'_>, logs: &mut EvaluationLogs) -> Result<()> {
    let limits = prepare_limits(request.profile, request.prepare);
    let prepared_mount = request
        .runner
        .storage
        .writable_mount(
            request.runner.docker,
            request.prepared_root,
            WritablePhase::ScorerPrepare,
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
        request.eval_type.scorer_mode_arg().to_string(),
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
                &format!("prepare-{}", request.eval_type.scorer_mode_arg()),
            ),
            image: request.profile.scorer_image.docker_reference().to_string(),
            cmd,
            env: vec![
                "AGENTICS_PHASE=prepare".to_string(),
                format!("AGENTICS_MODE={}", request.eval_type.scorer_mode_arg()),
            ],
            mounts: vec![
                bind_mount(request.bundle_dir, "/challenge", true),
                bind_mount(prepared_mount.path(), "/prepared", false),
            ],
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
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
        &format!("prepare-{}", request.eval_type.scorer_mode_arg()),
        visible_log_content(request.eval_type, &outcome.logs),
    );
    ensure_prepare_succeeded(&outcome, include_log_excerpts(request.eval_type))?;
    ensure_prepare_disk_limit(prepared_mount.path(), limits.disk_limit_mb).await?;
    replace_dir_all_if_separate(prepared_mount.path(), request.prepared_root).await?;

    Ok(())
}

/// Handles run manifest source for this module.
fn run_manifest_source(
    spec: &ChallengeBundleSpec,
    eval_type: ScoringMode,
) -> Result<RunManifestSource<'_>> {
    match eval_type {
        ScoringMode::Validation => {
            if let Some(path) = spec.execution.validation_runs.as_ref() {
                Ok(RunManifestSource::Static(path))
            } else if let Some(prepare) = spec.execution.validation_prepare.as_ref() {
                Ok(RunManifestSource::Prepared(prepare))
            } else {
                Err(AppError::Runner(
                    "challenge does not declare validation runs or validation prepare".to_string(),
                ))
            }
        }
        ScoringMode::Official => {
            if let Some(path) = spec.execution.official_runs.as_ref() {
                Ok(RunManifestSource::Static(path))
            } else if let Some(prepare) = spec.execution.official_prepare.as_ref() {
                Ok(RunManifestSource::Prepared(prepare))
            } else {
                Err(AppError::Runner(
                    "challenge does not declare official runs or official prepare".to_string(),
                ))
            }
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

/// Handles scorer limits for this module.
fn scorer_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    ZipProjectPhaseLimits {
        timeout_sec: profile.timeout_sec,
        memory_limit_mb: profile.memory_limit_mb,
        cpu_limit_millis: profile.cpu_limit_millis,
        disk_limit_mb: profile.disk_limit_mb,
        network_access: profile.scorer_network_access,
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

#[cfg(unix)]
/// Handles make container writable tree for this module.
async fn make_container_writable_tree(root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut pending = vec![root];
        while let Some(path) = pending.pop() {
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if !metadata.is_dir() && !metadata.is_file() {
                continue;
            }

            let mut permissions = metadata.permissions();
            let writable_bits = if metadata.is_dir() { 0o777 } else { 0o666 };
            permissions.set_mode(permissions.mode() | writable_bits);
            std::fs::set_permissions(&path, permissions)?;

            if metadata.is_dir() {
                for entry in std::fs::read_dir(&path)? {
                    let entry = entry?;
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("container writable chmod task failed: {e}")))?
}

#[cfg(unix)]
/// Handles make container readable tree for read-only Docker bind mounts.
async fn make_container_readable_tree(root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut pending = vec![root];
        while let Some(path) = pending.pop() {
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if !metadata.is_dir() && !metadata.is_file() {
                continue;
            }

            let mut permissions = metadata.permissions();
            let current_mode = permissions.mode();
            let readable_bits = if metadata.is_dir() {
                0o755
            } else if current_mode & 0o111 != 0 {
                0o555
            } else {
                0o444
            };
            permissions.set_mode(current_mode | readable_bits);
            std::fs::set_permissions(&path, permissions)?;

            if metadata.is_dir() {
                for entry in std::fs::read_dir(&path)? {
                    let entry = entry?;
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("container readable chmod task failed: {e}")))?
}

#[cfg(not(unix))]
/// Handles make container writable tree for this module.
async fn make_container_writable_tree(_root: &Path) -> Result<()> {
    Ok(())
}

#[cfg(not(unix))]
/// Handles make container readable tree for read-only Docker bind mounts.
async fn make_container_readable_tree(_root: &Path) -> Result<()> {
    Ok(())
}

/// Build an opaque solution-visible run name for one invocation index.
fn run_alias(index: usize) -> Result<RunName> {
    let display_index = index
        .checked_add(1)
        .ok_or_else(|| AppError::Internal("run alias index overflowed".to_string()))?;
    RunName::try_new(format!("run-{display_index:04}"))
        .map_err(|e| AppError::Internal(format!("generated invalid run alias: {e}")))
}

/// Copy a solution run tree into the scorer-visible area while rejecting symlinks and devices.
async fn copy_scorer_visible_run_tree(
    source: &Path,
    destination: &Path,
    visible_run_name: &str,
    limits: OutputTreeLimits,
) -> Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    let visible_run_name = visible_run_name.to_string();
    tokio::task::spawn_blocking(move || {
        copy_scorer_visible_run_tree_blocking(&source, &destination, &visible_run_name, limits)
    })
    .await
    .map_err(|e| AppError::Internal(format!("scorer output copy task failed: {e}")))?
}

/// Blocking implementation for scorer-visible run tree sanitization and copy.
fn copy_scorer_visible_run_tree_blocking(
    source: &Path,
    destination: &Path,
    visible_run_name: &str,
    limits: OutputTreeLimits,
) -> Result<()> {
    validate_scorer_visible_output_tree(source, visible_run_name, limits)?;

    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    while let Some((current_source, current_destination)) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&current_source)?;
        if metadata.file_type().is_symlink() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!("run `{visible_run_name}` produced a symlink in its output tree"),
                None,
            ));
        }
        if metadata.is_dir() {
            std::fs::create_dir_all(&current_destination)?;
            for entry in std::fs::read_dir(&current_source)? {
                let entry = entry?;
                pending.push((entry.path(), current_destination.join(entry.file_name())));
            }
        } else if metadata.is_file() {
            if let Some(parent) = current_destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&current_source, &current_destination)?;
        } else {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!("run `{visible_run_name}` produced a non-regular file in its output tree"),
                None,
            ));
        }
    }

    Ok(())
}

/// Handles materialize run io for this module.
async fn materialize_run_io(
    run: &ChallengeRunSpec,
    visible_run_name: &str,
    eval_type: ScoringMode,
    input_source_root: &Path,
    io_root: &Path,
    input_dir: &Path,
) -> Result<()> {
    let stdin = match (&run.stdin_json, &run.stdin_text) {
        (Some(value), None) => serde_json::to_string(value)
            .map_err(|e| AppError::Internal(format!("serialize stdin_json failed: {e}")))?,
        (None, Some(value)) => value.clone(),
        _ => String::new(),
    };
    tokio::fs::write(io_root.join("stdin.txt"), stdin).await?;
    for input in &run.input_files {
        write_run_input_file(
            input_source_root,
            input_dir,
            input,
            visible_run_name,
            eval_type,
        )
        .await?;
    }
    Ok(())
}

/// Writes run input file to the target path.
async fn write_run_input_file(
    input_source_root: &Path,
    input_dir: &Path,
    input: &ChallengeRunInputFile,
    visible_run_name: &str,
    eval_type: ScoringMode,
) -> Result<()> {
    let path = input_dir.join(input.path.as_path());
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(source_path) = &input.source_path {
        tokio::fs::copy(input_source_root.join(source_path.as_path()), path)
            .await
            .map_err(|e| {
                let source = match eval_type {
                    ScoringMode::Validation => format!(" source `{source_path}`"),
                    ScoringMode::Official => String::new(),
                };
                AppError::Runner(format!(
                    "copy run `{visible_run_name}` input{source} failed: {e}"
                ))
            })?;
        return Ok(());
    }

    let content = if let Some(value) = &input.content {
        value.clone()
    } else if let Some(value) = &input.content_json {
        serde_json::to_string(value)
            .map_err(|e| AppError::Internal(format!("serialize content_json failed: {e}")))?
    } else {
        String::new()
    };
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Writes run metadata to the target path.
async fn write_run_metadata(
    io_root: &Path,
    run: &ChallengeRunSpec,
    visible_run_name: &str,
    outcome: &ContainerOutcome,
) -> Result<()> {
    let metadata = SolutionRunMetadata {
        run_name: run.run_name.to_string(),
        interface: run.interface,
        exit_code: outcome.exit_code,
        timed_out: outcome.timed_out,
        wall_time_ms: outcome.wall_time_ms,
        stdout_path: format!("/solution-runs/{}/stdout.txt", run.run_name),
        stderr_path: format!("/solution-runs/{}/stderr.txt", run.run_name),
        output_dir: format!("/solution-runs/{}/output", run.run_name),
    };
    let bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|e| AppError::Internal(format!("serialize run metadata failed: {e}")))?;
    let metadata_path = io_root.join("agentics-run.json");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&metadata_path)
        .await
        .map_err(|e| {
            phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{visible_run_name}` used reserved metadata path `agentics-run.json`: {e}"
                ),
                None,
            )
        })?;
    file.write_all(&bytes).await?;
    Ok(())
}

/// Ensures declared outputs exist before continuing.
async fn ensure_declared_outputs_exist(
    run: &ChallengeRunSpec,
    visible_run_name: &str,
    output_dir: &Path,
) -> Result<()> {
    for output in &run.output_files {
        let output_path = output_dir.join(output.as_path());
        let metadata = tokio::fs::symlink_metadata(&output_path)
            .await
            .map_err(|_| {
                phase_error(
                    ZipProjectPhaseName::Run,
                    ZipProjectPhaseFailureReason::RunnerError,
                    format!(
                        "run `{visible_run_name}` did not produce declared output file `{output}`"
                    ),
                    None,
                )
            })?;
        if metadata.file_type().is_symlink() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!("run `{visible_run_name}` declared output file `{output}` is a symlink"),
                None,
            ));
        }
        if !metadata.is_file() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{visible_run_name}` declared output path `{output}` is not a regular file"
                ),
                None,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod run_metadata_tests {
    use std::fs;

    use uuid::Uuid;

    use super::{
        ContainerOutcome, OutputTreeLimits, copy_scorer_visible_run_tree, write_run_metadata,
    };
    use crate::models::challenge::{ChallengeRunInterface, ChallengeRunSpec};
    use crate::models::names::RunName;

    /// Return generous test output tree limits.
    fn test_output_limits() -> OutputTreeLimits {
        OutputTreeLimits {
            max_files: 8192,
            max_dirs: 1024,
            max_depth: 32,
        }
    }

    /// Verifies that solution-created symlinks cannot redirect worker metadata writes.
    #[cfg(unix)]
    #[tokio::test]
    async fn write_run_metadata_rejects_reserved_symlink() {
        let root =
            std::env::temp_dir().join(format!("agentics-run-metadata-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("test root should be created");
        let target = root.join("outside.json");
        std::os::unix::fs::symlink(&target, root.join("agentics-run.json"))
            .expect("reserved symlink should be created");

        let run = ChallengeRunSpec {
            run_name: RunName::try_new("case1").expect("run name should parse"),
            interface: ChallengeRunInterface::FileSystem,
            stdin_json: None,
            stdin_text: None,
            input_files: Vec::new(),
            output_files: Vec::new(),
        };
        let outcome = ContainerOutcome {
            exit_code: 0,
            logs: String::new(),
            timed_out: false,
            wall_time_ms: 1,
        };

        let error = write_run_metadata(&root, &run, "run-0001", &outcome)
            .await
            .expect_err("metadata write should reject a pre-existing symlink");
        assert!(
            error.to_string().contains("reserved metadata path"),
            "unexpected error: {error}"
        );
        assert!(!target.exists());

        fs::remove_dir_all(root).expect("test root should clean up");
    }

    /// Verifies that scorer-facing copies reject undeclared symlinks anywhere in the run tree.
    #[cfg(unix)]
    #[tokio::test]
    async fn scorer_visible_run_tree_rejects_extra_symlink() {
        let root =
            std::env::temp_dir().join(format!("agentics-run-tree-symlink-test-{}", Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("output")).expect("source output should be created");
        std::os::unix::fs::symlink("/challenge/private", source.join("output/extra"))
            .expect("extra symlink should be created");

        let error =
            copy_scorer_visible_run_tree(&source, &destination, "run-0001", test_output_limits())
                .await
                .expect_err("scorer-facing copy should reject symlinks");
        assert!(
            error.to_string().contains("produced a symlink"),
            "unexpected error: {error}"
        );
        assert!(!destination.join("output/extra").exists());

        fs::remove_dir_all(root).expect("test root should clean up");
    }

    /// Verifies output tree limits are checked before scorer-facing copy starts.
    #[tokio::test]
    async fn scorer_visible_run_tree_limit_rejects_before_copying() {
        let root =
            std::env::temp_dir().join(format!("agentics-run-tree-limit-test-{}", Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("output")).expect("source output should be created");
        fs::write(source.join("stdout.txt"), b"ok").expect("stdout should be created");
        fs::write(source.join("output/result.txt"), b"ok").expect("output should be created");

        let error = copy_scorer_visible_run_tree(
            &source,
            &destination,
            "run-0001",
            OutputTreeLimits {
                max_files: 1,
                max_dirs: 32,
                max_depth: 32,
            },
        )
        .await
        .expect_err("scorer-facing copy should reject excessive files");

        assert!(
            error.to_string().contains("output file limit"),
            "unexpected error: {error}"
        );
        assert!(!destination.exists());

        fs::remove_dir_all(root).expect("test root should clean up");
    }
}

/// Handles run interface for this module.
fn run_interface(interface: ChallengeRunInterface) -> &'static str {
    match interface {
        ChallengeRunInterface::Stdio => "stdio",
        ChallengeRunInterface::FileSystem => "file_system",
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

#[cfg(test)]
mod tests {
    use super::{
        RunnerAttempt, container_name, effective_phase_limits, prepare_limits, scorer_limits,
    };
    use crate::models::challenge::{ChallengePrepareSpec, ResourceProfileSpec};
    use crate::models::images::{ChallengeImageReference, LocalAgenticsImageReference};
    use crate::models::names::ResourceProfileName;
    use crate::models::paths::{BundleRelativePath, ScriptPath};
    use crate::zip_project::ZipProjectNetworkAccess;
    use crate::zip_project::{ZipProjectPhaseName, ZipProjectResolvedPhase};

    /// Verifies that network policy clamps to resource profile.
    #[test]
    fn network_policy_clamps_to_resource_profile() {
        assert_eq!(
            ZipProjectNetworkAccess::Enabled.clamp_to(ZipProjectNetworkAccess::Disabled),
            ZipProjectNetworkAccess::Disabled
        );
        assert_eq!(
            ZipProjectNetworkAccess::Loopback.docker_network_mode(),
            "none"
        );
    }

    /// Verifies that solution phase limits come directly from the resource profile.
    #[test]
    fn solution_phase_limits_come_from_resource_profile() {
        let profile = resource_profile();
        let phase = ZipProjectResolvedPhase {
            name: ZipProjectPhaseName::Run,
            command: ScriptPath::try_new("run.sh").expect("script path"),
        };

        let limits = effective_phase_limits(&profile, &phase);

        assert_eq!(limits.timeout_sec, 42);
        assert_eq!(limits.memory_limit_mb, 2048);
        assert_eq!(limits.cpu_limit_millis, 2500);
        assert_eq!(limits.disk_limit_mb, 4096);
        assert_eq!(limits.network_access, ZipProjectNetworkAccess::Loopback);
    }

    /// Verifies scorer and prepare phases use challenge-owned network policy.
    #[test]
    fn scorer_and_prepare_limits_use_challenge_owned_policy() {
        let profile = resource_profile();
        let prepare = ChallengePrepareSpec {
            command: vec!["python".to_string(), "prepare.py".to_string()],
            result_runs_file: BundleRelativePath::try_new("prepared/runs.json").expect("runs path"),
            network_access: ZipProjectNetworkAccess::Enabled,
            reproducibility_notes: None,
        };

        let scorer = scorer_limits(&profile);
        let prepare_limits = prepare_limits(&profile, &prepare);

        assert_eq!(scorer.timeout_sec, profile.timeout_sec);
        assert_eq!(scorer.network_access, ZipProjectNetworkAccess::Disabled);
        assert_eq!(prepare_limits.timeout_sec, profile.timeout_sec);
        assert_eq!(
            prepare_limits.network_access,
            ZipProjectNetworkAccess::Enabled
        );
    }

    /// Verifies retry attempts use distinct transient container identities.
    #[test]
    fn retry_attempts_have_distinct_container_names() {
        let first = RunnerAttempt::new("job/1", "worker a", 1);
        let second = RunnerAttempt::new("job/1", "worker a", 2);

        assert_ne!(
            container_name(&first, "run"),
            container_name(&second, "run")
        );
        assert!(container_name(&first, "run").contains("attempt-1"));
        assert!(container_name(&second, "run").contains("attempt-2"));
    }

    /// Build a resource profile for runner limit tests.
    fn resource_profile() -> ResourceProfileSpec {
        let image = ChallengeImageReference::Local {
            reference: LocalAgenticsImageReference::try_new(
                "agentics-linux-arm64-cpu:ubuntu26.04-local",
            )
            .expect("test image"),
        };
        ResourceProfileSpec {
            name: ResourceProfileName::try_new("python-cpu").expect("profile name"),
            resource_description: None,
            solution_image: image.clone(),
            scorer_image: image,
            timeout_sec: 42,
            memory_limit_mb: 2048,
            cpu_limit_millis: 2500,
            disk_limit_mb: 4096,
            setup_network_access: ZipProjectNetworkAccess::Enabled,
            build_network_access: ZipProjectNetworkAccess::Disabled,
            run_network_access: ZipProjectNetworkAccess::Loopback,
            scorer_network_access: ZipProjectNetworkAccess::Disabled,
            hardware_metadata: None,
        }
    }
}
