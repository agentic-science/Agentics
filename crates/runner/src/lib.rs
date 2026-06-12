#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::enum_glob_use,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::wildcard_imports,
        reason = "unit tests use direct assertions and fixture indexing for concise failure diagnostics"
    )
)]

//! Docker-backed `zip_project` evaluation runner.
//!
//! v0.2 uses one build solution container for setup/build, fresh no-egress run
//! solution containers that mount the build workspace read-only for benchmark
//! invocations, and a separate evaluator container. Run containers receive only the
//! current invocation's input files, while evaluator-only reference data stays in
//! the evaluator container.

use std::path::{Path, PathBuf};

use bollard::Docker;
use serde::Serialize;

use agentics_config::Config;
use agentics_contracts::zip_project::{
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, ZipProjectPhaseName,
};
use agentics_domain::models::challenge::{
    ChallengeBundleSpec, ChallengeExecutionSpec, ChallengeRunManifest, ChallengeSetupSpec,
    CoexecutedBenchmarkSetupSpec, DockerPlatform, MetricSchemaSpec, PipedStdioSessionManifest,
    PipedStdioSetupSpec, ResourceProfileSpec, TargetAccelerator,
};
use agentics_domain::models::evaluation::{
    EvaluationJobPayload, EvaluatorRunResult, ScoringMode, SolutionArtifactMetadata,
};
use agentics_domain::models::ids::SolutionSubmissionId;
use agentics_domain::models::paths::BundleRelativePath;
use agentics_error::{Result, ServiceError};
use agentics_storage::{Storage, StorageKey};

mod backend;
mod context;
mod docker;
mod errors;
mod execution;
mod filesystem;
mod labels;
mod limits;
mod logs;
mod run_io;
mod storage;
#[cfg(test)]
mod tests;
mod topologies;

pub use context::RunnerContainerScope;
pub use docker::{
    RunnerContainerCleanupSummary, RunnerContainerIdentity, RunnerContainerRuntimeState,
    RunnerContainerSnapshot, connect_docker,
};
pub use labels::{
    RUNNER_ATTEMPT_COUNT_LABEL, RUNNER_JOB_ID_LABEL, RUNNER_KIND_LABEL, RUNNER_KIND_ZIP_PROJECT,
    RUNNER_NAMESPACE_LABEL, RUNNER_PHASE_LABEL, RUNNER_SCOPE_HOSTED_WORKER, RUNNER_SCOPE_LABEL,
    RUNNER_SCOPE_LOCAL_VALIDATION, RUNNER_WORKER_ID_LABEL,
};

use backend::{DockerRunnerBackend, RunnerBackend};
use context::{
    JobRequirement, RetainedRunTree, RetainedRunnerTree, RunnerAttempt, RunnerContext,
    container_name,
};
use docker::{ContainerOutcome, ContainerRequest, bind_mount};
use errors::{ensure_container_succeeded, ensure_setup_succeeded};
use filesystem::{
    OutputTreeLimits, cleanup_paths, copy_dir_all, create_private_host_dir, ensure_disk_limit,
    ensure_setup_disk_limit, extract_zip_safe, validate_evaluator_visible_output_tree,
};
use limits::{
    EvaluationLimitConfig, effective_accelerator_count, effective_phase_limits, evaluator_limits,
    evaluator_setup_limits, writable_phase_for_solution_phase,
};
use logs::{
    EVALUATION_LOG_BYTES_PER_RUN, EvaluationLogPolicy, EvaluationLogs, append_named_logs,
    append_phase_logs, append_run_logs, include_log_excerpts, phase_name, visible_log_content,
};
use run_io::{
    copy_evaluator_visible_run_tree, ensure_declared_outputs_exist, make_container_readable_tree,
    make_container_writable_tree, materialize_input_files, materialize_run_io, run_alias,
    run_interface, write_run_metadata,
};
use storage::{RunnerStorage, WritablePhase};

/// Validated evaluator result plus the persisted runner log location.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Parsed and completed `result.json` emitted by the evaluator.
    pub result: EvaluatorRunResult,
    /// Storage-relative path to stdout and stderr captured from runner containers.
    pub runner_log_storage_key: StorageKey,
}

/// Docker-backed runner handle used by workers and local validation.
#[derive(Debug, Clone, Copy)]
pub struct DockerRunner<'a> {
    docker: &'a Docker,
}

impl<'a> DockerRunner<'a> {
    /// Wrap a Docker client in the MVP runner backend.
    pub const fn new(docker: &'a Docker) -> Self {
        Self { docker }
    }

    /// Execute one evaluation job and return the validated evaluator result.
    pub async fn execute_evaluation_job(
        &self,
        request: EvaluationJobExecution<'_>,
    ) -> Result<ExecutionResult> {
        execution::execute_evaluation_job(self.docker, request).await
    }

    /// List hosted-worker containers for service-owned reconciliation.
    pub async fn list_hosted_worker_containers(
        &self,
        config: &Config,
    ) -> Result<Vec<RunnerContainerSnapshot>> {
        DockerRunnerBackend::new(self.docker, &config.runner.namespace)
            .list_hosted_worker_containers()
            .await
    }

    /// Remove one stopped or already-stopped runner container.
    pub async fn remove_runner_container(&self, config: &Config, container_id: &str) -> Result<()> {
        DockerRunnerBackend::new(self.docker, &config.runner.namespace)
            .remove_runner_container(container_id)
            .await
    }

    /// Kill and remove one running runner container.
    pub async fn kill_runner_container(&self, config: &Config, container_id: &str) -> Result<()> {
        DockerRunnerBackend::new(self.docker, &config.runner.namespace)
            .kill_runner_container(container_id)
            .await
    }

    /// Remove stopped Agentics runner containers.
    pub async fn remove_stopped_runner_containers(&self, config: &Config) -> Result<u64> {
        DockerRunnerBackend::new(self.docker, &config.runner.namespace)
            .remove_stopped_runner_containers()
            .await
    }

    /// Remove stale local-validation containers.
    pub async fn remove_stale_local_validation_containers(
        &self,
        config: &Config,
    ) -> Result<RunnerContainerCleanupSummary> {
        DockerRunnerBackend::new(self.docker, &config.runner.namespace)
            .remove_stale_local_validation_containers()
            .await
    }
}

/// Remove stopped Agentics runner containers.
pub async fn remove_stopped_runner_containers(docker: &Docker, config: &Config) -> Result<u64> {
    DockerRunner::new(docker)
        .remove_stopped_runner_containers(config)
        .await
}

/// Remove stale local-validation containers.
pub async fn remove_stale_local_validation_containers(
    docker: &Docker,
    config: &Config,
) -> Result<RunnerContainerCleanupSummary> {
    DockerRunner::new(docker)
        .remove_stale_local_validation_containers(config)
        .await
}

/// Carries solution run request data across this module boundary.
struct SolutionRunRequest<'a> {
    eval_type: ScoringMode,
    log_policy: EvaluationLogPolicy,
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
    log_policy: EvaluationLogPolicy,
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
    log_policy: EvaluationLogPolicy,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    run_manifest: &'a ChallengeRunManifest,
    bundle_dir: &'a Path,
    setup_root: Option<&'a Path>,
    runs_root: &'a Path,
    retained_run_trees: &'a [RetainedRunTree],
    metadata_root: &'a Path,
    evaluator_output_root: &'a Path,
}

/// Carries piped-stdio session request data across this module boundary.
struct PipedStdioRequest<'a> {
    eval_type: ScoringMode,
    log_policy: EvaluationLogPolicy,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    manifest: &'a ZipProjectManifest,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
    session_root: &'a Path,
    build_root: &'a RetainedRunnerTree,
    run_work_root: &'a Path,
    metadata_root: &'a Path,
    evaluator_output_root: &'a Path,
    max_interaction_bytes_per_direction: u64,
    interaction_shutdown_grace_secs: u64,
}

/// Carries coexecuted-evaluator request data across this module boundary.
struct CoexecutedBenchmarkRequest<'a> {
    eval_type: ScoringMode,
    log_policy: EvaluationLogPolicy,
    spec: &'a ChallengeBundleSpec,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
    build_root: &'a RetainedRunnerTree,
    metadata_root: &'a Path,
    evaluator_output_root: &'a Path,
}

/// Carries resolved run plan data across this module boundary.
struct ResolvedRunPlan {
    manifest: ChallengeRunManifest,
    input_source_root: PathBuf,
    setup_root: Option<RetainedRunnerTree>,
}

/// Carries resolved interactive session data across this module boundary.
struct ResolvedSessionPlan {
    manifest: PipedStdioSessionManifest,
    input_source_root: PathBuf,
    setup_root: Option<RetainedRunnerTree>,
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
    log_policy: EvaluationLogPolicy,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
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
    log_policy: EvaluationLogPolicy,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
}

/// Carries setup request data across this module boundary.
struct EvaluatorSetupRequest<'a> {
    runner: RunnerContext<'a>,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    log_policy: EvaluationLogPolicy,
    setup: &'a ChallengeSetupSpec,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
}

/// Carries piped-stdio setup request data across this module boundary.
struct PipedStdioSetupRequest<'a> {
    runner: RunnerContext<'a>,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    log_policy: EvaluationLogPolicy,
    setup: &'a PipedStdioSetupSpec,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
}

/// Carries coexecuted-evaluator setup request data across this module boundary.
struct CoexecutedBenchmarkSetupRequest<'a> {
    runner: RunnerContext<'a>,
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    target: &'a str,
    eval_type: ScoringMode,
    log_policy: EvaluationLogPolicy,
    setup: &'a CoexecutedBenchmarkSetupSpec,
    bundle_dir: &'a Path,
    setup_root: &'a Path,
}

/// Carries all boundary inputs required to execute one evaluation job.
pub struct EvaluationJobExecution<'a> {
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
    /// Persistent solution submission identifier for evaluator metadata.
    pub solution_submission_id: &'a SolutionSubmissionId,
    /// Immutable metadata measured from the submitted solution artifact.
    pub artifact_metadata: &'a SolutionArtifactMetadata,
    /// Validated job payload produced by the API.
    pub payload: &'a EvaluationJobPayload,
    /// Durable artifact storage for inputs and bounded logs.
    pub storage: &'a dyn Storage,
}

/// Evaluator-facing metadata file written under `/metadata/submission.json`.
#[derive(Debug, Serialize)]
struct SubmissionMetadataFile<'a> {
    schema_version: u8,
    solution_submission_id: &'a SolutionSubmissionId,
    artifact_zip_bytes: u64,
    artifact_uncompressed_bytes: u64,
    artifact_file_count: u64,
    artifact_sha256: String,
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
pub fn evaluation_runner_log_storage_key(job_id: &str, attempt_count: i32) -> Result<StorageKey> {
    Ok(StorageKey::try_new(format!(
        "eval-artifacts/{job_id}/attempt-{attempt_count}/runner.log"
    ))?)
}

/// Write trusted evaluator-only submission metadata under a runner-owned root.
async fn write_submission_metadata(
    metadata_root: &Path,
    solution_submission_id: &SolutionSubmissionId,
    artifact_metadata: &SolutionArtifactMetadata,
) -> Result<()> {
    cleanup_paths([metadata_root.to_path_buf()]).await?;
    tokio::fs::create_dir_all(metadata_root).await?;
    let payload = SubmissionMetadataFile {
        schema_version: 1,
        solution_submission_id,
        artifact_zip_bytes: artifact_metadata.artifact_zip_bytes,
        artifact_uncompressed_bytes: artifact_metadata.artifact_uncompressed_bytes,
        artifact_file_count: artifact_metadata.artifact_file_count,
        artifact_sha256: format!("sha256:{}", artifact_metadata.artifact_sha256),
    };
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| {
        ServiceError::Internal(format!("serialize submission metadata failed: {e}"))
    })?;
    tokio::fs::write(metadata_root.join("submission.json"), bytes).await?;
    make_container_readable_tree(metadata_root).await?;
    Ok(())
}

/// Remove private official benchmark identifiers from runner errors crossing trust boundaries.
fn sanitize_runner_error(log_policy: EvaluationLogPolicy, error: ServiceError) -> ServiceError {
    match log_policy {
        EvaluationLogPolicy::FullDiagnostics => error,
        EvaluationLogPolicy::RedactedOfficialPrivateBenchmark => ServiceError::Runner(
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
    SetupGenerated(&'a ChallengeSetupSpec),
}

/// Enumerates interactive session source variants supported by this module.
enum PipedStdioSessionSource<'a> {
    Static(&'a BundleRelativePath),
    SetupGenerated(&'a PipedStdioSetupSpec),
}

/// Handles resolve run plan for this module.
async fn resolve_run_plan(
    request: RunPlanRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<ResolvedRunPlan> {
    match run_manifest_source(request.spec, request.eval_type)? {
        RunManifestSource::Static(manifest_path) => {
            let manifest = agentics_contracts::challenge_bundle::read_challenge_run_manifest(
                request.bundle_dir,
                manifest_path,
            )
            .await?;
            Ok(ResolvedRunPlan {
                manifest,
                input_source_root: request.bundle_dir.to_path_buf(),
                setup_root: None,
            })
        }
        RunManifestSource::SetupGenerated(setup) => {
            let retained_setup_root = run_evaluator_setup_phase(
                EvaluatorSetupRequest {
                    runner: request.runner,
                    profile: request.profile,
                    docker_platform: request.docker_platform,
                    accelerator: request.accelerator,
                    target: request.target,
                    eval_type: request.eval_type,
                    log_policy: request.log_policy,
                    setup,
                    bundle_dir: request.bundle_dir,
                    setup_root: request.setup_root,
                },
                logs,
            )
            .await?;
            let manifest_path = retained_setup_root
                .path()
                .join(setup.result_runs_file.as_path());
            let manifest = agentics_contracts::challenge_bundle::read_challenge_run_manifest_file(
                &manifest_path,
                &format!("setup-generated run manifest {}", manifest_path.display()),
            )
            .await?;
            agentics_contracts::challenge_bundle::validate_challenge_run_manifest_sources(
                retained_setup_root.path(),
                &manifest,
            )
            .await?;
            Ok(ResolvedRunPlan {
                manifest,
                input_source_root: retained_setup_root.path().to_path_buf(),
                setup_root: Some(retained_setup_root),
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
            let manifest = agentics_contracts::challenge_bundle::read_piped_stdio_session_manifest(
                request.bundle_dir,
                manifest_path,
            )
            .await?;
            Ok(ResolvedSessionPlan {
                manifest,
                input_source_root: request.bundle_dir.to_path_buf(),
                setup_root: None,
            })
        }
        PipedStdioSessionSource::SetupGenerated(setup) => {
            let retained_setup_root = run_piped_stdio_setup_phase(
                PipedStdioSetupRequest {
                    runner: request.runner,
                    profile: request.profile,
                    docker_platform: request.docker_platform,
                    accelerator: request.accelerator,
                    target: request.target,
                    eval_type: request.eval_type,
                    log_policy: request.log_policy,
                    setup,
                    bundle_dir: request.bundle_dir,
                    setup_root: request.setup_root,
                },
                logs,
            )
            .await?;
            let manifest_path = retained_setup_root
                .path()
                .join(setup.result_session_file.as_path());
            let manifest =
                agentics_contracts::challenge_bundle::read_piped_stdio_session_manifest_file(
                    &manifest_path,
                    &format!(
                        "setup-generated session manifest {}",
                        manifest_path.display()
                    ),
                )
                .await?;
            agentics_contracts::challenge_bundle::validate_piped_stdio_session_manifest_sources(
                retained_setup_root.path(),
                &manifest,
            )
            .await?;
            Ok(ResolvedSessionPlan {
                manifest,
                input_source_root: retained_setup_root.path().to_path_buf(),
                setup_root: Some(retained_setup_root),
            })
        }
    }
}

/// Handles run setup phase for this module.
async fn run_evaluator_setup_phase(
    request: EvaluatorSetupRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let limits = evaluator_setup_limits(request.profile);
    let setup_mount = request
        .runner
        .storage
        .writable_mount(
            request.runner.docker,
            request.setup_root,
            WritablePhase::EvaluatorSetup,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(setup_mount.path()).await?;
    let mut cmd = request.setup.command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--setup-dir".to_string(),
        "/setup".to_string(),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--target".to_string(),
        request.target.to_string(),
        "--runs-file".to_string(),
        format!("/setup/{}", request.setup.result_runs_file),
    ]);

    let outcome = request
        .runner
        .backend
        .run_container(ContainerRequest {
            name: container_name(
                request.runner.attempt,
                &format!("setup-{}", request.eval_type.evaluator_mode_arg()),
            ),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec![
                "AGENTICS_PHASE=setup".to_string(),
                format!("AGENTICS_MODE={}", request.eval_type.evaluator_mode_arg()),
            ],
            mounts: vec![
                bind_mount(request.bundle_dir, "/challenge", true),
                bind_mount(setup_mount.path(), "/setup", false),
            ],
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: request.runner.storage.docker_layer_quota_mb(&limits),
            labels: request.runner.container_labels("setup", Some(&setup_mount)),
        })
        .await?;
    append_named_logs(
        logs,
        &format!("setup-{}", request.eval_type.evaluator_mode_arg()),
        visible_log_content(request.log_policy, &outcome.logs),
    );
    ensure_setup_succeeded(&outcome, include_log_excerpts(request.log_policy))?;
    ensure_setup_disk_limit(setup_mount.path(), limits.disk_limit_mb).await?;
    make_container_readable_tree(setup_mount.path()).await?;

    Ok(RetainedRunnerTree::leased(setup_mount))
}

/// Run a trusted setup command that emits one interactive session manifest.
async fn run_piped_stdio_setup_phase(
    request: PipedStdioSetupRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let limits = evaluator_setup_limits(request.profile);
    let setup_mount = request
        .runner
        .storage
        .writable_mount(
            request.runner.docker,
            request.setup_root,
            WritablePhase::EvaluatorSetup,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(setup_mount.path()).await?;
    let mut cmd = request.setup.command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--setup-dir".to_string(),
        "/setup".to_string(),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--target".to_string(),
        request.target.to_string(),
        "--session-file".to_string(),
        format!("/setup/{}", request.setup.result_session_file),
    ]);

    let outcome = request
        .runner
        .backend
        .run_container(ContainerRequest {
            name: container_name(
                request.runner.attempt,
                &format!("setup-{}", request.eval_type.evaluator_mode_arg()),
            ),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec![
                "AGENTICS_PHASE=setup".to_string(),
                format!("AGENTICS_MODE={}", request.eval_type.evaluator_mode_arg()),
            ],
            mounts: vec![
                bind_mount(request.bundle_dir, "/challenge", true),
                bind_mount(setup_mount.path(), "/setup", false),
            ],
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: request.runner.storage.docker_layer_quota_mb(&limits),
            labels: request.runner.container_labels("setup", Some(&setup_mount)),
        })
        .await?;
    append_named_logs(
        logs,
        &format!("setup-{}", request.eval_type.evaluator_mode_arg()),
        visible_log_content(request.log_policy, &outcome.logs),
    );
    ensure_setup_succeeded(&outcome, include_log_excerpts(request.log_policy))?;
    ensure_setup_disk_limit(setup_mount.path(), limits.disk_limit_mb).await?;
    make_container_readable_tree(setup_mount.path()).await?;

    Ok(RetainedRunnerTree::leased(setup_mount))
}

/// Run a trusted setup command for a coexecuted-evaluator.
async fn run_coexecuted_benchmark_setup_phase(
    request: CoexecutedBenchmarkSetupRequest<'_>,
    logs: &mut EvaluationLogs,
) -> Result<RetainedRunnerTree> {
    let limits = evaluator_setup_limits(request.profile);
    let setup_mount = request
        .runner
        .storage
        .writable_mount(
            request.runner.docker,
            request.setup_root,
            WritablePhase::EvaluatorSetup,
            limits.disk_limit_mb,
        )
        .await?;
    make_container_writable_tree(setup_mount.path()).await?;
    let mut cmd = request.setup.command.clone();
    cmd.extend([
        "--challenge-dir".to_string(),
        "/challenge".to_string(),
        "--setup-dir".to_string(),
        "/setup".to_string(),
        "--mode".to_string(),
        request.eval_type.evaluator_mode_arg().to_string(),
        "--target".to_string(),
        request.target.to_string(),
    ]);

    let outcome = request
        .runner
        .backend
        .run_container(ContainerRequest {
            name: container_name(
                request.runner.attempt,
                &format!("setup-{}", request.eval_type.evaluator_mode_arg()),
            ),
            image: request
                .profile
                .evaluator_image
                .docker_reference()
                .to_string(),
            cmd,
            env: vec![
                "AGENTICS_PHASE=setup".to_string(),
                "AGENTICS_EXECUTION_MODE=coexecuted_benchmark".to_string(),
                format!("AGENTICS_MODE={}", request.eval_type.evaluator_mode_arg()),
            ],
            mounts: vec![
                bind_mount(request.bundle_dir, "/challenge", true),
                bind_mount(setup_mount.path(), "/setup", false),
            ],
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            accelerator_count: effective_accelerator_count(request.profile, request.accelerator)?,
            limits: limits.clone(),
            docker_layer_quota_mb: request.runner.storage.docker_layer_quota_mb(&limits),
            labels: request.runner.container_labels("setup", Some(&setup_mount)),
        })
        .await?;
    append_named_logs(
        logs,
        &format!("setup-{}", request.eval_type.evaluator_mode_arg()),
        visible_log_content(request.log_policy, &outcome.logs),
    );
    ensure_setup_succeeded(&outcome, include_log_excerpts(request.log_policy))?;
    ensure_setup_disk_limit(setup_mount.path(), limits.disk_limit_mb).await?;
    make_container_readable_tree(setup_mount.path()).await?;

    Ok(RetainedRunnerTree::leased(setup_mount))
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
            } else if let Some(setup) = spec.execution.validation_setup() {
                Ok(RunManifestSource::SetupGenerated(setup))
            } else {
                Err(ServiceError::Runner(
                    "challenge does not declare validation runs or validation setup".to_string(),
                ))
            }
        }
        ScoringMode::Official => {
            if let Some(path) = spec.execution.official_runs() {
                Ok(RunManifestSource::Static(path))
            } else if let Some(setup) = spec.execution.official_evaluation_setup() {
                Ok(RunManifestSource::SetupGenerated(setup))
            } else {
                Err(ServiceError::Runner(
                    "challenge does not declare official runs or official setup".to_string(),
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
            } else if let Some(setup) = &execution.validation_setup {
                Ok(PipedStdioSessionSource::SetupGenerated(setup))
            } else {
                Err(ServiceError::Runner(
                    "challenge does not declare validation session or validation setup".to_string(),
                ))
            }
        }
        ScoringMode::Official => {
            if let Some(path) = &execution.official_session {
                Ok(PipedStdioSessionSource::Static(path))
            } else if let Some(setup) = &execution.official_evaluation_setup {
                Ok(PipedStdioSessionSource::SetupGenerated(setup))
            } else {
                Err(ServiceError::Runner(
                    "challenge does not declare official session or official setup".to_string(),
                ))
            }
        }
    }
}

/// Resolve the optional setup command for one coexecuted-evaluator pass.
fn coexecuted_benchmark_setup(
    execution: &agentics_domain::models::challenge::CoexecutedBenchmarkExecutionSpec,
    eval_type: ScoringMode,
) -> Option<&CoexecutedBenchmarkSetupSpec> {
    match eval_type {
        ScoringMode::Validation => execution.validation_setup.as_ref(),
        ScoringMode::Official => execution.official_evaluation_setup.as_ref(),
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
