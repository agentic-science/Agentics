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
use crate::error::{Result, ServiceError};
use crate::models::challenge::{
    ChallengeBundleSpec, ChallengeExecutionSpec, ChallengePrepareSpec, ChallengeRunManifest,
    CoexecutedBenchmarkPrepareSpec, DockerPlatform, MetricSchemaSpec, PipedStdioPrepareSpec,
    PipedStdioSessionManifest, ResourceProfileSpec, StageResourceProfile, TargetAccelerator,
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
mod execution;
mod filesystem;
mod logs;
mod run_io;
mod storage;
#[cfg(test)]
mod tests;
mod topologies;

pub use docker::{
    RunnerContainerCleanupSummary, connect_docker, reconcile_runner_containers,
    remove_stale_local_validation_containers, remove_stopped_runner_containers,
};
pub use execution::execute_evaluation_job;

use docker::{
    ContainerOutcome, ContainerRequest, bind_mount, pre_pull_image, run_container,
    run_interactive_stdio_session,
};
use errors::{ensure_container_succeeded, ensure_prepare_succeeded};
use filesystem::{
    OutputTreeLimits, cleanup_paths, copy_dir_all, create_private_host_dir, ensure_disk_limit,
    ensure_prepare_disk_limit, extract_zip_safe, validate_evaluator_visible_output_tree,
};
use logs::{
    EVALUATION_LOG_BYTES_PER_RUN, EvaluationLogs, append_named_logs, append_phase_logs,
    append_run_logs, include_log_excerpts, phase_name, visible_log_content,
};
use run_io::{
    copy_evaluator_visible_run_tree, ensure_declared_outputs_exist, make_container_readable_tree,
    make_container_writable_tree, materialize_input_files, materialize_run_io, run_alias,
    run_interface, write_run_metadata,
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

/// Carries piped-stdio session request data across this module boundary.
struct PipedStdioRequest<'a> {
    eval_type: ScoringMode,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    manifest: &'a ZipProjectManifest,
    bundle_dir: &'a Path,
    prepared_root: &'a Path,
    session_root: &'a Path,
    build_root: &'a RetainedRunnerTree,
    run_work_root: &'a Path,
    evaluator_output_root: &'a Path,
    max_interaction_bytes_per_direction: u64,
    interaction_shutdown_grace_secs: u64,
}

/// Carries co-executed benchmark request data across this module boundary.
struct CoexecutedBenchmarkRequest<'a> {
    eval_type: ScoringMode,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    bundle_dir: &'a Path,
    prepared_root: &'a Path,
    build_root: &'a RetainedRunnerTree,
    evaluator_output_root: &'a Path,
}

/// Carries resolved run plan data across this module boundary.
struct ResolvedRunPlan {
    manifest: ChallengeRunManifest,
    input_source_root: PathBuf,
    run_manifest_container_path: String,
    prepared_root: Option<RetainedRunnerTree>,
}

/// Carries resolved interactive session data across this module boundary.
struct ResolvedSessionPlan {
    manifest: PipedStdioSessionManifest,
    input_source_root: PathBuf,
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

/// Carries interactive session plan request data across this module boundary.
struct SessionPlanRequest<'a> {
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

/// Carries piped-stdio prepare request data across this module boundary.
struct PipedStdioPrepareRequest<'a> {
    runner: RunnerContext<'a>,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    prepare: &'a PipedStdioPrepareSpec,
    bundle_dir: &'a Path,
    prepared_root: &'a Path,
}

/// Carries co-executed benchmark prepare request data across this module boundary.
struct CoexecutedBenchmarkPrepareRequest<'a> {
    runner: RunnerContext<'a>,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    prepare: &'a CoexecutedBenchmarkPrepareSpec,
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

/// Return the durable storage key used for one runner log.
pub fn evaluation_runner_log_key(job_id: &str, attempt_count: i32) -> Result<StorageKey> {
    Ok(StorageKey::try_new(format!(
        "eval-artifacts/{job_id}/attempt-{attempt_count}/runner.log"
    ))?)
}

/// Remove private official benchmark identifiers from runner errors crossing trust boundaries.
fn sanitize_runner_error(eval_type: ScoringMode, error: ServiceError) -> ServiceError {
    match eval_type {
        ScoringMode::Validation => error,
        ScoringMode::Official => ServiceError::Runner(
            "official evaluation failed; runner details are redacted for private benchmark execution"
                .to_string(),
        ),
    }
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
        .map_err(|e| ServiceError::Runner(format!("invalid result.json: {e}")))?;
    result
        .validate_for_mode(eval_type)
        .map_err(|e| ServiceError::Runner(format!("invalid result.json: {e}")))?;
    result
        .complete_metric_result(metric_schema, eval_type)
        .map_err(|e| ServiceError::Runner(format!("invalid result.json: {e}")))?;
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
        .map_err(|_| ServiceError::Runner("run count exceeds supported range".to_string()))?;
    if run_count == 0 {
        return Err(ServiceError::Runner(
            "run manifest must declare at least one run".to_string(),
        ));
    }
    if run_count > limits.max_runs {
        return Err(ServiceError::Runner(format!(
            "run manifest exceeded runner run limit: {run_count} > {} runs",
            limits.max_runs
        )));
    }
    let log_limit = run_count
        .checked_mul(EVALUATION_LOG_BYTES_PER_RUN)
        .ok_or_else(|| ServiceError::Runner("evaluation log limit overflow".to_string()))?;
    logs.set_limit(log_limit);
    Ok(())
}

/// Read evaluator result JSON only after proving its raw byte size is bounded.
async fn read_limited_result_json(path: &Path, max_bytes: u64) -> Result<String> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|e| ServiceError::Runner(format!("missing result.json: {e}")))?;
    if !metadata.is_file() {
        return Err(ServiceError::Runner(
            "result.json is not a regular file".to_string(),
        ));
    }
    let size = metadata.len();
    if size > max_bytes {
        return Err(ServiceError::Runner(format!(
            "result.json exceeded size limit: {size} > {max_bytes} bytes"
        )));
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| ServiceError::Runner(format!("invalid result.json bytes: {e}")))
}

/// Enumerates run manifest source variants supported by this module.
enum RunManifestSource<'a> {
    Static(&'a BundleRelativePath),
    Prepared(&'a ChallengePrepareSpec),
}

/// Enumerates interactive session source variants supported by this module.
enum PipedStdioSessionSource<'a> {
    Static(&'a BundleRelativePath),
    Prepared(&'a PipedStdioPrepareSpec),
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

/// Resolve the single interactive session manifest for a piped-stdio evaluation.
async fn resolve_piped_stdio_session_plan(
    request: SessionPlanRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<ResolvedSessionPlan> {
    match piped_stdio_session_source(request.spec, request.eval_type)? {
        PipedStdioSessionSource::Static(manifest_path) => {
            let manifest = crate::challenge_bundle::read_piped_stdio_session_manifest(
                request.bundle_dir,
                manifest_path,
            )
            .await?;
            Ok(ResolvedSessionPlan {
                manifest,
                input_source_root: request.bundle_dir.to_path_buf(),
                prepared_root: None,
            })
        }
        PipedStdioSessionSource::Prepared(prepare) => {
            let retained_prepared_root = run_piped_stdio_prepare_phase(
                PipedStdioPrepareRequest {
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
                .join(prepare.result_session_file.as_path());
            let manifest = crate::challenge_bundle::read_piped_stdio_session_manifest_file(
                &manifest_path,
                &format!("prepared session manifest {}", manifest_path.display()),
            )
            .await?;
            crate::challenge_bundle::validate_piped_stdio_session_manifest_sources(
                retained_prepared_root.path(),
                &manifest,
            )
            .await?;
            Ok(ResolvedSessionPlan {
                manifest,
                input_source_root: retained_prepared_root.path().to_path_buf(),
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
    let limits = prepare_limits(request.profile);
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

/// Run a trusted prepare command that emits one interactive session manifest.
async fn run_piped_stdio_prepare_phase(
    request: PipedStdioPrepareRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let limits = prepare_limits(request.profile);
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
        "--session-file".to_string(),
        format!("/prepared/{}", request.prepare.result_session_file),
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

/// Run a trusted prepare command for a co-executed benchmark.
async fn run_coexecuted_benchmark_prepare_phase(
    request: CoexecutedBenchmarkPrepareRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let limits = prepare_limits(request.profile);
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
                "AGENTICS_EXECUTION_MODE=coexecuted_benchmark".to_string(),
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
                Err(ServiceError::Runner(
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
                Err(ServiceError::Runner(
                    "challenge does not declare official runs or official prepare".to_string(),
                ))
            }
        }
    }
}

/// Resolve session manifest source for the current piped-stdio mode.
fn piped_stdio_session_source(
    spec: &ChallengeBundleSpec,
    eval_type: ScoringMode,
) -> Result<PipedStdioSessionSource<'_>> {
    let execution = spec.execution.piped_stdio().ok_or_else(|| {
        ServiceError::Runner("challenge execution is not piped_stdio".to_string())
    })?;
    match eval_type {
        ScoringMode::Validation => {
            if let Some(path) = &execution.validation_session {
                Ok(PipedStdioSessionSource::Static(path))
            } else if let Some(prepare) = &execution.validation_prepare {
                Ok(PipedStdioSessionSource::Prepared(prepare))
            } else {
                Err(ServiceError::Runner(
                    "challenge does not declare validation session or validation prepare"
                        .to_string(),
                ))
            }
        }
        ScoringMode::Official => {
            if let Some(path) = &execution.official_session {
                Ok(PipedStdioSessionSource::Static(path))
            } else if let Some(prepare) = &execution.official_prepare {
                Ok(PipedStdioSessionSource::Prepared(prepare))
            } else {
                Err(ServiceError::Runner(
                    "challenge does not declare official session or official prepare".to_string(),
                ))
            }
        }
    }
}

/// Resolve the optional prepare command for one co-executed benchmark pass.
fn coexecuted_benchmark_prepare(
    execution: &crate::models::challenge::CoexecutedBenchmarkExecutionSpec,
    eval_type: ScoringMode,
) -> Option<&CoexecutedBenchmarkPrepareSpec> {
    match eval_type {
        ScoringMode::Validation => execution.validation_prepare.as_ref(),
        ScoringMode::Official => execution.official_prepare.as_ref(),
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
                ServiceError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata".to_string(),
                )
            })?;
            let count = hardware.gpu_count.ok_or_else(|| {
                ServiceError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata.gpu_count"
                        .to_string(),
                )
            })?;
            if count == 0 {
                return Err(ServiceError::Runner(
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
) -> Result<ZipProjectPhaseLimits> {
    let stage = match phase.name {
        ZipProjectPhaseName::Setup => &profile.solution.setup,
        ZipProjectPhaseName::Build => &profile.solution.build,
        ZipProjectPhaseName::Run => profile.solution.run.as_ref().ok_or_else(|| {
            ServiceError::Runner(
                "resource_profile.solution.run is required for solution run".to_string(),
            )
        })?,
    };
    Ok(stage_limits(stage))
}

/// Handles evaluator limits for this module.
fn evaluator_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    stage_limits(&profile.evaluator.run)
}

/// Handles prepare limits for this module.
fn prepare_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    stage_limits(&profile.evaluator.setup)
}

/// Convert a stage resource profile into runner phase limits.
fn stage_limits(stage: &StageResourceProfile) -> ZipProjectPhaseLimits {
    ZipProjectPhaseLimits {
        timeout_sec: stage.timeout_sec,
        memory_limit_mb: stage.memory_limit_mb,
        cpu_limit_millis: stage.cpu_limit_millis,
        disk_limit_mb: stage.disk_limit_mb,
        network_access: stage.network_access,
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
