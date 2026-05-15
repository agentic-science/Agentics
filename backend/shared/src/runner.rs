//! Docker-backed `zip_project` evaluation runner.
//!
//! v0.2 uses one build solution container for setup/build, fresh no-egress run
//! solution containers that mount the build workspace read-only for benchmark
//! invocations, and a separate scorer container. Run containers receive only the
//! current invocation's input files, while scorer-only reference data stays in
//! the scorer container.

use std::path::{Path, PathBuf};

use bollard::Docker;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::challenge::{
    ChallengeBundleSpec, ChallengePrepareSpec, ChallengeRunInputFile, ChallengeRunInterface,
    ChallengeRunManifest, ChallengeRunSpec, DockerPlatform, MetricSchemaSpec, ResourceProfileSpec,
    TargetAccelerator,
};
use crate::models::evaluation::{EvaluationJobPayload, ScorerRunResult, ScoringMode};
use crate::models::paths::BundleRelativePath;
use crate::storage::{Storage, StorageKey};
use crate::zip_project::{
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, ZipProjectPhaseFailureReason,
    ZipProjectPhaseLimits, ZipProjectPhaseName, ZipProjectResolvedPhase,
    parse_zip_project_manifest,
};

mod docker;
mod errors;
mod filesystem;
mod logs;
mod storage;

pub use docker::connect_docker;

use docker::{ContainerOutcome, ContainerRequest, bind_mount, pre_pull_image, run_container};
use errors::{ensure_container_succeeded, ensure_prepare_succeeded, phase_error};
use filesystem::{
    cleanup_paths, copy_dir_all, ensure_disk_limit, ensure_prepare_disk_limit, extract_zip_safe,
};
use logs::{append_named_logs, append_phase_logs, append_run_logs, phase_name};
use storage::{RunnerStorage, WritablePhase};

/// Validated scorer result plus the persisted runner log location.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Parsed and normalized `result.json` emitted by the scorer.
    pub result: ScorerRunResult,
    /// Storage-relative path to stdout and stderr captured from runner containers.
    pub log_path: String,
}

#[derive(Clone, Copy)]
struct RunnerContext<'a> {
    docker: &'a Docker,
    storage: &'a RunnerStorage,
    job_id: &'a str,
}

struct SolutionRunRequest<'a> {
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    manifest: &'a ZipProjectManifest,
    run_manifest: &'a ChallengeRunManifest,
    input_source_root: &'a Path,
    build_root: &'a Path,
    runs_root: &'a Path,
}

#[derive(Clone, Copy)]
struct SetupBuildRequest<'a> {
    profile: &'a ResourceProfileSpec,
    docker_platform: DockerPlatform,
    accelerator: TargetAccelerator,
    manifest: &'a ZipProjectManifest,
    source_root: &'a Path,
    build_root: &'a Path,
}

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

struct ResolvedRunPlan {
    manifest: ChallengeRunManifest,
    input_source_root: PathBuf,
    run_manifest_container_path: String,
    prepared_root: Option<PathBuf>,
}

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

/// Execute one evaluation job in Docker and return the validated scorer result.
pub async fn execute_evaluation_job(
    docker: &Docker,
    config: &Config,
    job_id: &str,
    eval_type: ScoringMode,
    payload: &EvaluationJobPayload,
    storage: &dyn Storage,
) -> Result<ExecutionResult> {
    let working_root = Path::new(&config.storage_root)
        .join("eval-artifacts")
        .join(job_id);
    let source_root = std::env::temp_dir()
        .join("agentics-solutions")
        .join(job_id)
        .join("source");
    let build_root = working_root.join("build-workspace");
    let runs_root = working_root.join("solution-runs");
    let prepared_root = working_root.join("prepared");
    let scorer_output_root = working_root.join("scorer-output");
    let log_path_rel = StorageKey::try_new(format!("eval-artifacts/{job_id}/runner.log"))?;

    tokio::fs::create_dir_all(&working_root).await?;
    tokio::fs::create_dir_all(&source_root).await?;
    tokio::fs::create_dir_all(&build_root).await?;
    tokio::fs::create_dir_all(&runs_root).await?;
    tokio::fs::create_dir_all(&scorer_output_root).await?;

    let bundle_dir = Path::new(&payload.bundle_path);
    let spec = crate::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
    if config.require_digest_pinned_images {
        crate::challenge_bundle::validate_digest_pinned_images(&spec)?;
    }
    let result_path = scorer_output_root.join(spec.scorer.result_file.as_path());
    let mut logs = String::new();
    let runner_storage = RunnerStorage::from_config(config)?;
    let runner_context = RunnerContext {
        docker,
        storage: &runner_storage,
        job_id,
    };

    let execution = async {
        let target = spec.target(&payload.target).ok_or_else(|| {
            AppError::Runner(format!(
                "challenge contract does not declare target `{}`",
                payload.target
            ))
        })?;
        let profile = &target.resource_profile;
        pre_pull_image(docker, &profile.solution_image, target.docker_platform).await?;
        pre_pull_image(docker, &profile.scorer_image, target.docker_platform).await?;

        let artifact_key = StorageKey::try_new(&payload.artifact_path)?;
        let artifact_bytes = storage.get(&artifact_key).await?;
        let artifact_path = working_root.join("solution.zip");
        tokio::fs::write(&artifact_path, artifact_bytes).await?;
        extract_zip_safe(&artifact_path, &source_root).await?;
        let manifest = read_solution_manifest(&source_root, &spec).await?;
        run_setup_and_build(
            runner_context,
            SetupBuildRequest {
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
        run_solution_invocations(
            runner_context,
            SolutionRunRequest {
                profile,
                docker_platform: target.docker_platform,
                accelerator: target.accelerator,
                manifest: &manifest,
                run_manifest: &run_plan.manifest,
                input_source_root: &run_plan.input_source_root,
                build_root: &build_root,
                runs_root: &runs_root,
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

        let result_raw = tokio::fs::read_to_string(&result_path)
            .await
            .map_err(|e| AppError::Runner(format!("missing result.json: {e}")))?;
        let mut result: ScorerRunResult = serde_json::from_str(&result_raw)
            .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
        validate_scorer_result(&mut result, eval_type, &spec.metric_schema)?;

        Ok(ExecutionResult {
            result,
            log_path: log_path_rel.to_string(),
        })
    }
    .await;

    storage.put(&log_path_rel, logs.as_bytes()).await?;
    let cleanup = cleanup_paths([source_root]).await;
    match (execution, cleanup) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(_), Err(cleanup_err)) => Err(cleanup_err),
        (Err(run_err), Ok(())) => Err(run_err),
        (Err(run_err), Err(cleanup_err)) => Err(AppError::Runner(format!(
            "{run_err}; additionally failed to clean runner workspace: {cleanup_err}"
        ))),
    }
}

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
    parse_zip_project_manifest(&raw)
}

async fn run_setup_and_build(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut String,
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
                name: container_name(runner.job_id, &format!("{:?}", phase.name).to_lowercase()),
                image: request.profile.solution_image.clone(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(request.build_root, "/workspace", false)],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
            },
        )
        .await?;
        append_phase_logs(logs, phase.name, &outcome.logs);
        ensure_container_succeeded(phase.name, &outcome)?;
        ensure_disk_limit(request.build_root, limits.disk_limit_mb, phase.name).await?;
    }

    Ok(())
}

async fn run_setup_and_build_bounded(
    runner: RunnerContext<'_>,
    request: SetupBuildRequest<'_>,
    logs: &mut String,
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
                name: container_name(runner.job_id, &format!("{:?}", phase.name).to_lowercase()),
                image: request.profile.solution_image.clone(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(workspace.path(), "/workspace", false)],
                working_dir: "/workspace".to_string(),
                docker_platform: request.docker_platform,
                accelerator: request.accelerator,
                limits: limits.clone(),
                docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
            },
        )
        .await?;
        append_phase_logs(logs, phase.name, &outcome.logs);
        ensure_container_succeeded(phase.name, &outcome)?;
        ensure_disk_limit(workspace.path(), limits.disk_limit_mb, phase.name).await?;
        replace_dir_all(workspace.path(), request.build_root).await?;
        source_workspace = request.build_root.to_path_buf();
    }

    Ok(())
}

async fn run_solution_invocations(
    runner: RunnerContext<'_>,
    request: SolutionRunRequest<'_>,
    logs: &mut String,
) -> Result<()> {
    let run_phase = request
        .manifest
        .phase_execution_plan()
        .into_iter()
        .find(|phase| phase.name == ZipProjectPhaseName::Run)
        .ok_or_else(|| AppError::Runner("zip_project manifest has no run phase".to_string()))?;

    for run in &request.run_manifest.runs {
        let durable_io_root = request.runs_root.join(run.run_name.as_str());
        let limits = effective_phase_limits(request.profile, &run_phase);
        let io_mount = runner
            .storage
            .writable_mount(
                &durable_io_root,
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
        materialize_run_io(run, request.input_source_root, io_root, &input_dir).await?;
        make_container_writable_tree(io_root).await?;

        let outcome = run_container(
            runner.docker,
            ContainerRequest {
                name: container_name(runner.job_id, &format!("run-{}", run.run_name)),
                image: request.profile.solution_image.clone(),
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
                    format!("AGENTICS_RUN_NAME={}", run.run_name),
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
            },
        )
        .await?;
        append_run_logs(logs, &run.run_name, &outcome.logs);
        ensure_container_succeeded(ZipProjectPhaseName::Run, &outcome)?;
        write_run_metadata(io_root, run, &outcome).await?;
        ensure_disk_limit(io_root, limits.disk_limit_mb, ZipProjectPhaseName::Run).await?;
        ensure_declared_outputs_exist(run, &output_dir).await?;
        replace_dir_all_if_separate(io_root, &durable_io_root).await?;
    }

    Ok(())
}

async fn run_scorer(
    runner: RunnerContext<'_>,
    request: ScorerRequest<'_>,
    logs: &mut String,
) -> Result<()> {
    let limits = scorer_limits(request.profile);
    let output_mount = runner
        .storage
        .writable_mount(
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
            name: container_name(runner.job_id, "scorer"),
            image: request.profile.scorer_image.clone(),
            cmd,
            env: vec!["AGENTICS_PHASE=scorer".to_string()],
            mounts,
            working_dir: "/challenge".to_string(),
            docker_platform: request.docker_platform,
            accelerator: request.accelerator,
            limits: limits.clone(),
            docker_layer_quota_mb: runner.storage.docker_layer_quota_mb(&limits),
        },
    )
    .await?;
    append_named_logs(logs, "scorer", &outcome.logs);
    if outcome.timed_out || outcome.exit_code != 0 {
        return Err(AppError::Runner(format!(
            "scorer container failed: exit_code={}, timed_out={}",
            outcome.exit_code, outcome.timed_out
        )));
    }
    replace_dir_all_if_separate(output_mount.path(), request.scorer_output_root).await?;

    Ok(())
}

fn validate_scorer_result(
    result: &mut ScorerRunResult,
    eval_type: ScoringMode,
    metric_schema: &MetricSchemaSpec,
) -> Result<()> {
    result
        .validate_for_mode(eval_type)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
    result
        .normalize_metrics(metric_schema, eval_type)
        .map_err(|e| AppError::Runner(format!("invalid result.json: {e}")))?;
    result.mode = Some(eval_type);
    Ok(())
}

enum RunManifestSource<'a> {
    Static(&'a BundleRelativePath),
    Prepared(&'a ChallengePrepareSpec),
}

async fn resolve_run_plan(
    request: RunPlanRequest<'_>,
    logs: &mut String,
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

async fn run_prepare_phase(request: PrepareRequest<'_>, logs: &mut String) -> Result<()> {
    let limits = prepare_limits(request.profile, request.prepare);
    let prepared_mount = request
        .runner
        .storage
        .writable_mount(
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
                request.runner.job_id,
                &format!("prepare-{}", request.eval_type.scorer_mode_arg()),
            ),
            image: request.profile.scorer_image.clone(),
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
        },
    )
    .await?;
    append_named_logs(
        logs,
        &format!("prepare-{}", request.eval_type.scorer_mode_arg()),
        &outcome.logs,
    );
    ensure_prepare_succeeded(&outcome)?;
    ensure_prepare_disk_limit(prepared_mount.path(), limits.disk_limit_mb).await?;
    replace_dir_all_if_separate(prepared_mount.path(), request.prepared_root).await?;

    Ok(())
}

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

fn effective_phase_limits(
    profile: &ResourceProfileSpec,
    phase: &ZipProjectResolvedPhase,
) -> ZipProjectPhaseLimits {
    let max_network = match phase.name {
        ZipProjectPhaseName::Setup => profile.setup_network_access,
        ZipProjectPhaseName::Build => profile.build_network_access,
        ZipProjectPhaseName::Run => profile.run_network_access,
    };
    ZipProjectPhaseLimits {
        timeout_sec: phase.limits.timeout_sec.min(profile.timeout_sec),
        memory_limit_mb: phase.limits.memory_limit_mb.min(profile.memory_limit_mb),
        cpu_limit_millis: phase.limits.cpu_limit_millis.min(profile.cpu_limit_millis),
        disk_limit_mb: phase.limits.disk_limit_mb.min(profile.disk_limit_mb),
        network_access: phase.limits.network_access.clamp_to(max_network),
        log_limit_bytes: phase.limits.log_limit_bytes,
    }
}

fn scorer_limits(profile: &ResourceProfileSpec) -> ZipProjectPhaseLimits {
    ZipProjectPhaseLimits {
        timeout_sec: profile.timeout_sec,
        memory_limit_mb: profile.memory_limit_mb,
        cpu_limit_millis: profile.cpu_limit_millis,
        disk_limit_mb: profile.disk_limit_mb,
        network_access: profile.scorer_network_access,
        log_limit_bytes: 1024 * 1024,
    }
}

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
        log_limit_bytes: 1024 * 1024,
    }
}

async fn replace_dir_all(source: &Path, destination: &Path) -> Result<()> {
    cleanup_paths([destination.to_path_buf()]).await?;
    copy_dir_all(source, destination).await
}

async fn replace_dir_all_if_separate(source: &Path, destination: &Path) -> Result<()> {
    if source == destination {
        return Ok(());
    }
    replace_dir_all(source, destination).await
}

fn writable_phase_for_solution_phase(phase: ZipProjectPhaseName) -> WritablePhase {
    match phase {
        ZipProjectPhaseName::Setup => WritablePhase::SolutionSetup,
        ZipProjectPhaseName::Build => WritablePhase::SolutionBuild,
        ZipProjectPhaseName::Run => WritablePhase::SolutionRun,
    }
}

#[cfg(unix)]
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

#[cfg(not(unix))]
async fn make_container_writable_tree(_root: &Path) -> Result<()> {
    Ok(())
}

async fn materialize_run_io(
    run: &ChallengeRunSpec,
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
        write_run_input_file(input_source_root, input_dir, input).await?;
    }
    Ok(())
}

async fn write_run_input_file(
    input_source_root: &Path,
    input_dir: &Path,
    input: &ChallengeRunInputFile,
) -> Result<()> {
    let path = input_dir.join(input.path.as_path());
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(source_path) = &input.source_path {
        tokio::fs::copy(input_source_root.join(source_path.as_path()), path)
            .await
            .map_err(|e| {
                AppError::Runner(format!("copy run input source `{source_path}` failed: {e}"))
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

async fn write_run_metadata(
    io_root: &Path,
    run: &ChallengeRunSpec,
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
    tokio::fs::write(io_root.join("agentics-run.json"), bytes).await?;
    Ok(())
}

async fn ensure_declared_outputs_exist(run: &ChallengeRunSpec, output_dir: &Path) -> Result<()> {
    for output in &run.output_files {
        let output_path = output_dir.join(output.as_path());
        let metadata = tokio::fs::symlink_metadata(&output_path)
            .await
            .map_err(|_| {
                phase_error(
                    ZipProjectPhaseName::Run,
                    ZipProjectPhaseFailureReason::RunnerError,
                    format!(
                        "run `{}` did not produce declared output file `{output}`",
                        run.run_name
                    ),
                    None,
                )
            })?;
        if metadata.file_type().is_symlink() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{}` declared output file `{output}` is a symlink",
                    run.run_name
                ),
                None,
            ));
        }
        if !metadata.is_file() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{}` declared output path `{output}` is not a regular file",
                    run.run_name
                ),
                None,
            ));
        }
    }
    Ok(())
}

fn run_interface(interface: ChallengeRunInterface) -> &'static str {
    match interface {
        ChallengeRunInterface::Stdio => "stdio",
        ChallengeRunInterface::FileSystem => "file_system",
    }
}

fn container_name(job_id: &str, suffix: &str) -> String {
    let safe_suffix = suffix
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("agentics-{job_id}-{safe_suffix}")
}

#[cfg(test)]
mod tests {
    use crate::zip_project::ZipProjectNetworkAccess;

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
}
