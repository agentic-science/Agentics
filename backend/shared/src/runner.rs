//! Docker-backed `zip_project` evaluation runner.
//!
//! v0.2 uses one build solution container for setup/build, fresh no-egress run
//! solution containers for benchmark invocations, and a separate scorer
//! container. Private benchmark data is only mounted into the scorer container.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{
    ContainerCreateBody, HostConfig, HostConfigLogConfig, Mount, MountTypeEnum, ResourcesUlimits,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, KillContainerOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions, WaitContainerOptionsBuilder,
};
use futures::StreamExt;
use tokio::time::timeout;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::challenge::{
    ChallengeBundleSpec, ChallengeRunInputFile, ChallengeRunInterface, ChallengeRunManifest,
    ChallengeRunSpec, MetricSchemaSpec, ResourceProfileSpec,
};
use crate::models::evaluation::{EvaluationJobPayload, ScorerRunResult, ScoringMode};
use crate::storage::Storage;
use crate::zip_project::{
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, ZipProjectPhaseFailureReason,
    ZipProjectPhaseLimits, ZipProjectPhaseName, ZipProjectResolvedPhase,
    parse_zip_project_manifest,
};

const MAX_RUNNER_ARTIFACT_BYTES: u64 = 20 * 1024 * 1024;
const MAX_RUNNER_FILE_COUNT: usize = 256;
const MAX_RUNNER_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024;

/// Validated scorer result plus the persisted runner log location.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Parsed and normalized `result.json` emitted by the scorer.
    pub result: ScorerRunResult,
    /// Storage-relative path to stdout and stderr captured from runner containers.
    pub log_path: String,
}

#[derive(Debug)]
struct ContainerRequest {
    name: String,
    image: String,
    cmd: Vec<String>,
    env: Vec<String>,
    mounts: Vec<Mount>,
    working_dir: String,
    limits: ZipProjectPhaseLimits,
}

#[derive(Debug)]
struct ContainerOutcome {
    exit_code: i64,
    logs: String,
    timed_out: bool,
}

#[derive(Clone, Copy)]
struct RunnerContext<'a> {
    docker: &'a Docker,
    job_id: &'a str,
}

struct SolutionRunRequest<'a> {
    profile: &'a ResourceProfileSpec,
    manifest: &'a ZipProjectManifest,
    run_manifest: &'a ChallengeRunManifest,
    build_root: &'a Path,
    runs_root: &'a Path,
}

struct ScorerRequest<'a> {
    eval_type: ScoringMode,
    spec: &'a ChallengeBundleSpec,
    run_manifest_path: &'a str,
    bundle_dir: &'a Path,
    runs_root: &'a Path,
    scorer_output_root: &'a Path,
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
    let scorer_output_root = working_root.join("scorer-output");
    let log_path_rel = format!("eval-artifacts/{job_id}/runner.log");

    tokio::fs::create_dir_all(&working_root).await?;
    tokio::fs::create_dir_all(&source_root).await?;
    tokio::fs::create_dir_all(&build_root).await?;
    tokio::fs::create_dir_all(&runs_root).await?;
    tokio::fs::create_dir_all(&scorer_output_root).await?;

    let bundle_dir = Path::new(&payload.bundle_path);
    let spec = crate::challenge_bundle::read_challenge_bundle_spec(bundle_dir).await?;
    let result_path = scorer_output_root.join(&spec.scorer.result_file);
    let mut logs = String::new();
    let runner_context = RunnerContext { docker, job_id };

    let execution = async {
        pre_pull_image(docker, &spec.resource_profile.solution_image).await?;
        pre_pull_image(docker, &spec.resource_profile.scorer_image).await?;

        extract_zip_safe(&payload.artifact_path, &source_root).await?;
        let manifest = read_solution_manifest(&source_root, &spec).await?;
        copy_dir_all(&source_root, &build_root).await?;

        run_setup_and_build(
            runner_context,
            &spec.resource_profile,
            &manifest,
            &build_root,
            &mut logs,
        )
        .await?;

        let run_manifest_path = run_manifest_path(&spec, eval_type)?;
        let run_manifest =
            crate::challenge_bundle::read_challenge_run_manifest(bundle_dir, run_manifest_path)
                .await?;
        run_solution_invocations(
            runner_context,
            SolutionRunRequest {
                profile: &spec.resource_profile,
                manifest: &manifest,
                run_manifest: &run_manifest,
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
                run_manifest_path,
                bundle_dir,
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
            log_path: log_path_rel.clone(),
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
    let manifest_path = source_root.join(&spec.solution.manifest_file);
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
    profile: &ResourceProfileSpec,
    manifest: &ZipProjectManifest,
    build_root: &Path,
    logs: &mut String,
) -> Result<()> {
    for phase in manifest
        .phase_execution_plan()
        .into_iter()
        .filter(|phase| phase.name != ZipProjectPhaseName::Run)
    {
        let limits = effective_phase_limits(profile, &phase);
        let cmd = vec!["sh".to_string(), format!("/workspace/{}", phase.command)];
        let outcome = run_container(
            runner.docker,
            ContainerRequest {
                name: container_name(runner.job_id, &format!("{:?}", phase.name).to_lowercase()),
                image: profile.solution_image.clone(),
                cmd,
                env: vec![format!("AGENTICS_PHASE={}", phase_name(&phase.name))],
                mounts: vec![bind_mount(build_root, "/workspace", false)],
                working_dir: "/workspace".to_string(),
                limits: limits.clone(),
            },
        )
        .await?;
        append_phase_logs(logs, phase.name, &outcome.logs);
        ensure_container_succeeded(phase.name, &outcome)?;
        ensure_disk_limit(build_root, limits.disk_limit_mb, phase.name).await?;
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
        let run_workspace = request.runs_root.join(&run.run_id).join("workspace");
        let io_root = request.runs_root.join(&run.run_id);
        let input_dir = io_root.join("input");
        let output_dir = io_root.join("output");
        tokio::fs::create_dir_all(&input_dir).await?;
        tokio::fs::create_dir_all(&output_dir).await?;
        copy_dir_all(request.build_root, &run_workspace).await?;
        materialize_run_io(run, &io_root, &input_dir).await?;

        let limits = effective_phase_limits(request.profile, &run_phase);
        let outcome = run_container(
            runner.docker,
            ContainerRequest {
                name: container_name(runner.job_id, &format!("run-{}", run.run_id)),
                image: request.profile.solution_image.clone(),
                cmd: vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "mkdir -p /io/output; if [ -f /io/stdin.txt ]; then sh \"$1\" < /io/stdin.txt > /io/stdout.txt; else sh \"$1\" > /io/stdout.txt; fi"
                        .to_string(),
                    "agentics-run".to_string(),
                    format!("/workspace/{}", run_phase.command),
                ],
                env: vec![
                    "AGENTICS_PHASE=run".to_string(),
                    format!("AGENTICS_RUN_ID={}", run.run_id),
                    format!("AGENTICS_INTERFACE={}", run_interface(run.interface)),
                    "AGENTICS_INPUT_DIR=/io/input".to_string(),
                    "AGENTICS_OUTPUT_DIR=/io/output".to_string(),
                ],
                mounts: vec![
                    bind_mount(&run_workspace, "/workspace", false),
                    bind_mount(&io_root, "/io", false),
                ],
                working_dir: "/workspace".to_string(),
                limits: limits.clone(),
            },
        )
        .await?;
        append_run_logs(logs, &run.run_id, &outcome.logs);
        ensure_container_succeeded(ZipProjectPhaseName::Run, &outcome)?;
        ensure_disk_limit(
            &run_workspace,
            limits.disk_limit_mb,
            ZipProjectPhaseName::Run,
        )
        .await?;
        ensure_declared_outputs_exist(run, &output_dir).await?;
    }

    Ok(())
}

async fn run_scorer(
    runner: RunnerContext<'_>,
    request: ScorerRequest<'_>,
    logs: &mut String,
) -> Result<()> {
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
        format!("/challenge/{}", request.run_manifest_path),
    ]);

    let limits = scorer_limits(&request.spec.resource_profile);
    let outcome = run_container(
        runner.docker,
        ContainerRequest {
            name: container_name(runner.job_id, "scorer"),
            image: request.spec.resource_profile.scorer_image.clone(),
            cmd,
            env: vec!["AGENTICS_PHASE=scorer".to_string()],
            mounts: vec![
                bind_mount(request.bundle_dir, "/challenge", true),
                bind_mount(request.runs_root, "/solution-runs", true),
                bind_mount(request.scorer_output_root, "/output", false),
            ],
            working_dir: "/challenge".to_string(),
            limits,
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

async fn run_container(docker: &Docker, request: ContainerRequest) -> Result<ContainerOutcome> {
    let memory_bytes = request.limits.memory_limit_mb * 1024 * 1024;
    let nano_cpus = i64::from(request.limits.cpu_limit_millis) * 1_000_000;
    let log_limit_bytes = request.limits.log_limit_bytes;
    let host_config = HostConfig {
        network_mode: Some(
            request
                .limits
                .network_access
                .docker_network_mode()
                .to_string(),
        ),
        mounts: Some(request.mounts),
        auto_remove: Some(false),
        memory: Some(memory_bytes as i64),
        memory_swap: Some(memory_bytes as i64),
        nano_cpus: Some(nano_cpus),
        pids_limit: Some(256),
        ulimits: Some(vec![
            ResourcesUlimits {
                name: Some("nofile".to_string()),
                soft: Some(1024),
                hard: Some(1024),
            },
            ResourcesUlimits {
                name: Some("nproc".to_string()),
                soft: Some(256),
                hard: Some(256),
            },
        ]),
        cap_drop: Some(vec!["ALL".to_string()]),
        security_opt: Some(vec!["no-new-privileges:true".to_string()]),
        privileged: Some(false),
        publish_all_ports: Some(false),
        init: Some(true),
        oom_kill_disable: Some(false),
        log_config: Some(docker_log_config(log_limit_bytes)),
        ..Default::default()
    };
    let container_config = ContainerCreateBody {
        image: Some(request.image),
        cmd: Some(request.cmd),
        env: Some(request.env),
        working_dir: Some(request.working_dir),
        host_config: Some(host_config),
        labels: Some({
            let mut labels = std::collections::HashMap::new();
            labels.insert("agentics.runner".to_string(), "zip_project".to_string());
            labels
        }),
        ..Default::default()
    };

    let create_opts = CreateContainerOptionsBuilder::default()
        .name(&request.name)
        .build();
    let create_resp = docker
        .create_container(Some(create_opts), container_config)
        .await
        .map_err(|e| AppError::Docker(format!("create container failed: {e}")))?;
    let container_id = create_resp.id;

    let run_result = run_created_container(
        docker,
        &container_id,
        request.limits.timeout_sec,
        log_limit_bytes,
    )
    .await;
    let remove_result = remove_container(docker, &container_id).await;
    match (run_result, remove_result) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(_), Err(cleanup_err)) => Err(cleanup_err),
        (Err(run_err), Ok(())) => Err(run_err),
        (Err(run_err), Err(cleanup_err)) => Err(AppError::Docker(format!(
            "{run_err}; additionally failed to remove runner container: {cleanup_err}"
        ))),
    }
}

async fn run_created_container(
    docker: &Docker,
    container_id: &str,
    timeout_sec: u64,
    log_limit_bytes: u64,
) -> Result<ContainerOutcome> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| AppError::Docker(format!("start container failed: {e}")))?;

    let wait_opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();
    let wait_result = timeout(
        Duration::from_secs(timeout_sec),
        docker
            .wait_container(container_id, Some(wait_opts))
            .collect::<Vec<_>>(),
    )
    .await;

    let (exit_code, timed_out) = match wait_result {
        Ok(results) => {
            let exit_code = results
                .into_iter()
                .flatten()
                .last()
                .map(|status| status.status_code)
                .unwrap_or(1);
            (exit_code, false)
        }
        Err(_) => {
            let kill_opts = KillContainerOptionsBuilder::default()
                .signal("SIGKILL")
                .build();
            docker
                .kill_container(container_id, Some(kill_opts))
                .await
                .map_err(|e| AppError::Docker(format!("kill timed out container failed: {e}")))?;
            (124, true)
        }
    };
    let (logs, _logs_truncated) =
        collect_container_logs(docker, container_id, log_limit_bytes).await?;
    Ok(ContainerOutcome {
        exit_code,
        logs,
        timed_out,
    })
}

async fn remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker
        .remove_container(container_id, Some(remove_opts))
        .await
        .map_err(|e| AppError::Docker(format!("remove runner container failed: {e}")))?;
    Ok(())
}

fn docker_log_config(limit_bytes: u64) -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert("max-size".to_string(), format!("{}b", limit_bytes.max(1)));
    config.insert("max-file".to_string(), "1".to_string());

    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

/// Connect to Docker using `AGENTICS_DOCKER_HOST` when configured, otherwise the local default.
pub fn connect_docker(config: &Config) -> Result<Docker> {
    match config
        .docker_host
        .as_deref()
        .map(str::trim)
        .filter(|host| !host.is_empty())
    {
        Some(host) => Docker::connect_with_host(host)
            .map_err(|e| AppError::Docker(format!("failed to connect to Docker host {host}: {e}"))),
        None => Docker::connect_with_defaults()
            .map_err(|e| AppError::Docker(format!("failed to connect to Docker: {e}"))),
    }
}

async fn collect_container_logs(
    docker: &Docker,
    container_id: &str,
    limit_bytes: u64,
) -> Result<(String, bool)> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();
    let mut logs = docker.logs(container_id, Some(opts));
    let mut output = Vec::new();
    let mut truncated = false;
    let limit = usize::try_from(limit_bytes).unwrap_or(usize::MAX);

    while let Some(chunk) = logs.next().await {
        match chunk {
            Ok(LogOutput::StdOut { message })
            | Ok(LogOutput::StdErr { message })
            | Ok(LogOutput::Console { message }) => {
                append_bounded_log_bytes(&mut output, &message, limit, &mut truncated);
                if output.len() >= limit {
                    truncated = true;
                    break;
                }
            }
            Err(e) => {
                return Err(AppError::Docker(format!(
                    "collect container logs failed: {e}"
                )));
            }
            _ => {}
        }
    }

    let mut output = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        output.push_str(&format!(
            "\n[agentics] container logs truncated at {limit_bytes} bytes\n"
        ));
    }

    Ok((output, truncated))
}

fn append_bounded_log_bytes(
    output: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    truncated: &mut bool,
) {
    if output.len() >= limit {
        *truncated = !chunk.is_empty();
        return;
    }

    let remaining = limit - output.len();
    if chunk.len() > remaining {
        output.extend_from_slice(&chunk[..remaining]);
        *truncated = true;
    } else {
        output.extend_from_slice(chunk);
    }
}

async fn extract_zip_safe(artifact_path: &str, target_dir: &Path) -> Result<()> {
    let artifact_size = tokio::fs::metadata(artifact_path).await?.len();
    if artifact_size > MAX_RUNNER_ARTIFACT_BYTES {
        return Err(AppError::Validation(format!(
            "solution archive must be at most {} bytes",
            MAX_RUNNER_ARTIFACT_BYTES
        )));
    }

    let artifact_path = artifact_path.to_string();
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_zip_safe_blocking(&artifact_path, &target_dir))
        .await
        .map_err(|e| AppError::Internal(format!("zip extraction task failed: {e}")))?
}

fn extract_zip_safe_blocking(artifact_path: &str, target_dir: &Path) -> Result<()> {
    let reader = std::fs::File::open(artifact_path)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > MAX_RUNNER_FILE_COUNT {
        return Err(AppError::Validation(format!(
            "solution archive must contain at most {} entries",
            MAX_RUNNER_FILE_COUNT
        )));
    }

    let mut total_uncompressed_size = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        total_uncompressed_size = total_uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| AppError::Validation("solution archive is too large".to_string()))?;
        if total_uncompressed_size > MAX_RUNNER_UNCOMPRESSED_BYTES {
            return Err(AppError::Validation(format!(
                "solution archive must expand to at most {} bytes",
                MAX_RUNNER_UNCOMPRESSED_BYTES
            )));
        }

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

/// Pull an image before creating a runner container.
pub async fn pre_pull_image(docker: &Docker, image: &str) -> Result<()> {
    use bollard::query_parameters::CreateImageOptionsBuilder;

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .build();
    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        if let Err(e) = item {
            return Err(AppError::Docker(format!(
                "failed to pull image {image}: {e}"
            )));
        }
    }

    Ok(())
}

fn run_manifest_path(spec: &ChallengeBundleSpec, eval_type: ScoringMode) -> Result<&str> {
    match eval_type {
        ScoringMode::Validation => spec.execution.validation_runs.as_deref().ok_or_else(|| {
            AppError::Runner("challenge does not declare validation runs".to_string())
        }),
        ScoringMode::Official => spec.execution.official_runs.as_deref().ok_or_else(|| {
            AppError::Runner("challenge does not declare official runs".to_string())
        }),
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

fn bind_mount(path: &Path, target: &str, read_only: bool) -> Mount {
    Mount {
        target: Some(target.to_string()),
        source: Some(path.to_string_lossy().to_string()),
        typ: Some(MountTypeEnum::BIND),
        read_only: Some(read_only),
        ..Default::default()
    }
}

async fn materialize_run_io(
    run: &ChallengeRunSpec,
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
        write_run_input_file(input_dir, input).await?;
    }
    Ok(())
}

async fn write_run_input_file(input_dir: &Path, input: &ChallengeRunInputFile) -> Result<()> {
    let path = input_dir.join(&input.path);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let content = match (&input.content, &input.content_json) {
        (Some(value), None) => value.clone(),
        (None, Some(value)) => serde_json::to_string(value)
            .map_err(|e| AppError::Internal(format!("serialize content_json failed: {e}")))?,
        _ => String::new(),
    };
    tokio::fs::write(path, content).await?;
    Ok(())
}

async fn ensure_declared_outputs_exist(run: &ChallengeRunSpec, output_dir: &Path) -> Result<()> {
    for output in &run.output_files {
        if !output_dir.join(output).is_file() {
            return Err(phase_error(
                ZipProjectPhaseName::Run,
                ZipProjectPhaseFailureReason::RunnerError,
                format!(
                    "run `{}` did not produce declared output file `{output}`",
                    run.run_id
                ),
                None,
            ));
        }
    }
    Ok(())
}

fn ensure_container_succeeded(
    phase: ZipProjectPhaseName,
    outcome: &ContainerOutcome,
) -> Result<()> {
    if outcome.timed_out {
        let message = append_log_excerpt("phase timed out", &outcome.logs);
        return Err(phase_error(
            phase,
            ZipProjectPhaseFailureReason::TimedOut,
            message,
            None,
        ));
    }
    if outcome.exit_code != 0 {
        let message = append_log_excerpt(
            &format!("phase exited with status {}", outcome.exit_code),
            &outcome.logs,
        );
        return Err(phase_error(
            phase,
            ZipProjectPhaseFailureReason::NonZeroExit,
            message,
            Some(outcome.exit_code as i32),
        ));
    }
    Ok(())
}

fn append_log_excerpt(message: &str, logs: &str) -> String {
    let trimmed = logs.trim();
    if trimmed.is_empty() {
        return message.to_string();
    }
    let excerpt: String = trimmed.chars().take(500).collect();
    format!("{message}; logs: {excerpt}")
}

fn phase_error(
    phase: ZipProjectPhaseName,
    reason: ZipProjectPhaseFailureReason,
    message: String,
    exit_code: Option<i32>,
) -> AppError {
    let report = crate::zip_project::ZipProjectPhaseFailureReport {
        phase,
        reason,
        message,
        exit_code,
        log_path: None,
    };
    AppError::Runner(format!(
        "zip_project phase failed: {}",
        serde_json::to_string(&report)
            .unwrap_or_else(|_| "unserializable phase failure".to_string())
    ))
}

async fn ensure_disk_limit(
    path: &Path,
    disk_limit_mb: u64,
    phase: ZipProjectPhaseName,
) -> Result<()> {
    let path = path.to_path_buf();
    let bytes = tokio::task::spawn_blocking(move || directory_size(&path))
        .await
        .map_err(|e| AppError::Internal(format!("disk usage task failed: {e}")))??;
    let limit_bytes = disk_limit_mb * 1024 * 1024;
    if bytes > limit_bytes {
        return Err(phase_error(
            phase,
            ZipProjectPhaseFailureReason::ResourceLimit,
            format!("phase exceeded disk limit: {bytes} > {limit_bytes} bytes"),
            None,
        ));
    }
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            total = total
                .checked_add(directory_size(&entry.path())?)
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
        } else {
            // Count symlink directory entries as links, never as their host targets.
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| AppError::Runner("directory size overflow".to_string()))?;
        }
    }
    Ok(total)
}

async fn copy_dir_all(source: &Path, destination: &Path) -> Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || copy_dir_all_blocking(&source, &destination))
        .await
        .map_err(|e| AppError::Internal(format!("copy task failed: {e}")))?
}

fn copy_dir_all_blocking(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all_blocking(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

async fn cleanup_paths<const N: usize>(paths: [PathBuf; N]) -> Result<()> {
    for path in paths {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AppError::Io(e)),
        }
    }
    Ok(())
}

fn append_phase_logs(logs: &mut String, phase: ZipProjectPhaseName, content: &str) {
    append_named_logs(logs, &format!("phase:{}", phase_name(&phase)), content);
}

fn append_run_logs(logs: &mut String, run_id: &str, content: &str) {
    append_named_logs(logs, &format!("run:{run_id}"), content);
}

fn append_named_logs(logs: &mut String, name: &str, content: &str) {
    logs.push_str("\n===== ");
    logs.push_str(name);
    logs.push_str(" =====\n");
    logs.push_str(content);
    if !content.ends_with('\n') {
        logs.push('\n');
    }
}

fn phase_name(phase: &ZipProjectPhaseName) -> &'static str {
    match phase {
        ZipProjectPhaseName::Setup => "setup",
        ZipProjectPhaseName::Build => "build",
        ZipProjectPhaseName::Run => "run",
    }
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
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::zip_project::ZipProjectNetworkAccess;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-runner-{name}-{}", uuid::Uuid::new_v4()))
    }

    fn write_zip(path: &Path, entries: Vec<(String, Vec<u8>)>) {
        let file = std::fs::File::create(path).expect("failed to create test zip");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            archive
                .start_file(name, options)
                .expect("failed to start zip entry");
            archive
                .write_all(&bytes)
                .expect("failed to write zip entry");
        }

        archive.finish().expect("failed to finish test zip");
    }

    #[tokio::test]
    async fn extract_zip_safe_skips_unsafe_entry_names() {
        let zip_path = temp_path("unsafe-entry.zip");
        let target_dir = temp_path("unsafe-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        write_zip(
            &zip_path,
            vec![
                ("../escape.py".to_string(), b"print('bad')\n".to_vec()),
                ("main.py".to_string(), b"print('ok')\n".to_vec()),
                ("scripts/setup.sh".to_string(), b"true\n".to_vec()),
            ],
        );

        extract_zip_safe(&zip_path.to_string_lossy(), &target_dir)
            .await
            .expect("extraction should succeed");

        let extracted_files = std::fs::read_dir(&target_dir)
            .expect("failed to read target dir")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("failed to collect target dir entries");
        assert_eq!(extracted_files.len(), 2);
        assert!(target_dir.join("main.py").is_file());
        assert!(target_dir.join("scripts/setup.sh").is_file());

        let _ = std::fs::remove_file(zip_path);
        let _ = std::fs::remove_dir_all(target_dir);
    }

    #[tokio::test]
    async fn extract_zip_safe_rejects_too_many_entries() {
        let zip_path = temp_path("too-many.zip");
        let target_dir = temp_path("too-many-target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        let entries = (0..=MAX_RUNNER_FILE_COUNT)
            .map(|i| (format!("file-{i}.txt"), Vec::new()))
            .collect();
        write_zip(&zip_path, entries);

        let result = extract_zip_safe(&zip_path.to_string_lossy(), &target_dir).await;

        assert!(
            matches!(result, Err(AppError::Validation(message)) if message.contains("at most"))
        );

        let _ = std::fs::remove_file(zip_path);
        let _ = std::fs::remove_dir_all(target_dir);
    }

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

    #[test]
    fn bounded_log_append_truncates_by_byte_limit() {
        let mut output = Vec::new();
        let mut truncated = false;

        append_bounded_log_bytes(&mut output, b"abcdef", 4, &mut truncated);

        assert_eq!(output, b"abcd");
        assert!(truncated);
    }

    #[cfg(unix)]
    #[test]
    fn directory_size_does_not_follow_symlinks() {
        let root = temp_path("symlink-size-root");
        let outside = temp_path("symlink-size-outside.txt");
        std::fs::create_dir_all(&root).expect("failed to create root");
        std::fs::write(&outside, vec![b'x'; 1024 * 1024]).expect("failed to write outside file");
        std::os::unix::fs::symlink(&outside, root.join("outside-link"))
            .expect("failed to create symlink");

        let bytes = directory_size(&root).expect("directory size should succeed");

        assert!(
            bytes < 1024 * 1024,
            "symlink target should not be counted: {bytes}"
        );

        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(root);
    }
}
