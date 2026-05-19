use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{
    ContainerCreateBody, ContainerSummaryStateEnum, DeviceRequest, HostConfig, HostConfigLogConfig,
    Mount, MountTypeEnum, ResourcesUlimits,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, KillContainerOptionsBuilder, ListContainersOptionsBuilder,
    LogsOptionsBuilder, RemoveContainerOptionsBuilder, StartContainerOptions,
    WaitContainerOptionsBuilder,
};
use futures::StreamExt;
use sqlx::PgPool;
use tokio::time::timeout;

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::challenge::{DockerPlatform, TargetAccelerator};
use crate::models::ids::EvaluationJobId;
use crate::zip_project::ZipProjectPhaseLimits;

const STALE_RUNNER_CONTAINER_MIN_AGE_SECS: i64 = 600;
const PERMISSION_FIX_TIMEOUT_SECS: u64 = 30;
const PLATFORM_CONTAINER_LOG_LIMIT_BYTES: u64 = 1024 * 1024;
const PERMISSION_FIX_LOG_LIMIT_BYTES: u64 = 4 * 1024;

#[derive(Debug)]
/// Carries container request data across this module boundary.
pub(super) struct ContainerRequest {
    pub(super) name: String,
    pub(super) image: String,
    pub(super) cmd: Vec<String>,
    pub(super) env: Vec<String>,
    pub(super) mounts: Vec<Mount>,
    pub(super) working_dir: String,
    pub(super) docker_platform: DockerPlatform,
    pub(super) accelerator: TargetAccelerator,
    pub(super) limits: ZipProjectPhaseLimits,
    pub(super) docker_layer_quota_mb: Option<u64>,
    pub(super) labels: HashMap<String, String>,
}

#[derive(Debug)]
/// Carries container outcome data across this module boundary.
pub(super) struct ContainerOutcome {
    pub(super) exit_code: i64,
    pub(super) logs: String,
    pub(super) timed_out: bool,
    pub(super) wall_time_ms: u64,
}

/// Summary of Agentics runner container reconciliation work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunnerContainerCleanupSummary {
    pub removed_stopped: u64,
    pub removed_running: u64,
}

impl RunnerContainerCleanupSummary {
    /// Return the total number of containers removed during reconciliation.
    pub fn total_removed(self) -> u64 {
        self.removed_stopped.saturating_add(self.removed_running)
    }
}

/// Handles run container for this module.
pub(super) async fn run_container(
    docker: &Docker,
    request: ContainerRequest,
) -> Result<ContainerOutcome> {
    let permission_fix_image = request.image.clone();
    let permission_fix_platform = request.docker_platform;
    let permission_fix_mounts = writable_bind_mounts(&request.mounts);
    let permission_fix_labels = request.labels.clone();
    let memory_bytes = request
        .limits
        .memory_limit_mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| AppError::Runner("memory limit overflow".to_string()))?;
    let memory = i64::try_from(memory_bytes)
        .map_err(|_| AppError::Runner("memory limit exceeds Docker API range".to_string()))?;
    let nano_cpus = i64::from(request.limits.cpu_limit_millis)
        .checked_mul(1_000_000)
        .ok_or_else(|| AppError::Runner("CPU limit overflow".to_string()))?;
    let log_cap_bytes = PLATFORM_CONTAINER_LOG_LIMIT_BYTES;
    let mut host_config = hardened_container_host_config(
        request
            .limits
            .network_access
            .docker_network_mode()
            .to_string(),
        request.mounts,
        log_cap_bytes,
        false,
    );
    host_config.memory = Some(memory);
    host_config.memory_swap = Some(memory);
    host_config.nano_cpus = Some(nano_cpus);
    host_config.storage_opt = docker_storage_opt(request.docker_layer_quota_mb);
    host_config.runtime = gpu_runtime(request.accelerator);
    host_config.device_requests = gpu_device_requests(request.accelerator);
    let container_config = ContainerCreateBody {
        image: Some(request.image),
        cmd: Some(request.cmd),
        env: Some(request.env),
        working_dir: Some(request.working_dir),
        host_config: Some(host_config),
        labels: Some({
            let mut labels = request.labels;
            labels.insert(
                super::RUNNER_KIND_LABEL.to_string(),
                super::RUNNER_KIND_ZIP_PROJECT.to_string(),
            );
            labels
        }),
        ..Default::default()
    };

    let create_opts = CreateContainerOptionsBuilder::default()
        .name(&request.name)
        .platform(request.docker_platform.as_str())
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
        log_cap_bytes,
    )
    .await;
    let permission_result = repair_bind_mount_permissions(
        docker,
        permission_fix_image,
        permission_fix_platform,
        permission_fix_mounts,
        permission_fix_labels,
    )
    .await;
    let remove_result = remove_container(docker, &container_id).await;
    match (run_result, permission_result, remove_result) {
        (Ok(result), Ok(()), Ok(())) => Ok(result),
        (Ok(_), Err(permission_err), Ok(())) => Err(permission_err),
        (Ok(_), Ok(()), Err(cleanup_err)) => Err(cleanup_err),
        (Ok(_), Err(permission_err), Err(cleanup_err)) => Err(AppError::Docker(format!(
            "{permission_err}; additionally failed to remove runner container: {cleanup_err}"
        ))),
        (Err(run_err), Ok(()), Ok(())) => Err(run_err),
        (Err(run_err), Err(permission_err), Ok(())) => Err(AppError::Docker(format!(
            "{run_err}; additionally failed to repair bind mount permissions: {permission_err}"
        ))),
        (Err(run_err), Ok(()), Err(cleanup_err)) => Err(AppError::Docker(format!(
            "{run_err}; additionally failed to remove runner container: {cleanup_err}"
        ))),
        (Err(run_err), Err(permission_err), Err(cleanup_err)) => Err(AppError::Docker(format!(
            "{run_err}; additionally failed to repair bind mount permissions: {permission_err}; additionally failed to remove runner container: {cleanup_err}"
        ))),
    }
}

/// Return writable bind mounts that may need host-side permission repair.
fn writable_bind_mounts(mounts: &[Mount]) -> Vec<Mount> {
    mounts
        .iter()
        .filter(|mount| {
            mount.typ == Some(MountTypeEnum::BIND)
                && mount.read_only != Some(true)
                && mount.target.is_some()
        })
        .cloned()
        .collect()
}

/// Use a short sidecar to make files created by root-running images host-cleanable.
async fn repair_bind_mount_permissions(
    docker: &Docker,
    image: String,
    docker_platform: DockerPlatform,
    mounts: Vec<Mount>,
    mut labels: HashMap<String, String>,
) -> Result<()> {
    if mounts.is_empty() {
        return Ok(());
    }
    let mut cmd = vec![
        "sh".to_string(),
        "-c".to_string(),
        "for path do if [ -e \"$path\" ]; then chmod -R ugo+rwX \"$path\" || exit 1; fi; done"
            .to_string(),
        "agentics-permission-fix".to_string(),
    ];
    cmd.extend(
        mounts
            .iter()
            .filter_map(|mount| mount.target.as_ref())
            .cloned(),
    );
    labels.insert(
        super::RUNNER_KIND_LABEL.to_string(),
        super::RUNNER_KIND_ZIP_PROJECT.to_string(),
    );
    labels.insert("agentics.phase".to_string(), "permission-fix".to_string());

    let host_config = permission_repair_host_config(mounts);
    let container_config = ContainerCreateBody {
        image: Some(image),
        cmd: Some(cmd),
        working_dir: Some("/".to_string()),
        host_config: Some(host_config),
        labels: Some(labels),
        ..Default::default()
    };
    let name = format!("agentics-permission-fix-{}", uuid::Uuid::new_v4());
    let create_opts = CreateContainerOptionsBuilder::default()
        .name(&name)
        .platform(docker_platform.as_str())
        .build();
    let create_resp = docker
        .create_container(Some(create_opts), container_config)
        .await
        .map_err(|e| AppError::Docker(format!("create permission repair container failed: {e}")))?;
    let container_id = create_resp.id;
    let run_result = run_created_container(
        docker,
        &container_id,
        PERMISSION_FIX_TIMEOUT_SECS,
        PERMISSION_FIX_LOG_LIMIT_BYTES,
    )
    .await
    .and_then(|outcome| {
        if outcome.exit_code == 0 && !outcome.timed_out {
            Ok(())
        } else {
            Err(AppError::Docker(format!(
                "permission repair container failed: exit_code={}, timed_out={}, logs={}",
                outcome.exit_code, outcome.timed_out, outcome.logs
            )))
        }
    });
    let remove_result = remove_container(docker, &container_id).await;
    match (run_result, remove_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(cleanup_err)) => Err(cleanup_err),
        (Err(run_err), Ok(())) => Err(run_err),
        (Err(run_err), Err(cleanup_err)) => Err(AppError::Docker(format!(
            "{run_err}; additionally failed to remove permission repair container: {cleanup_err}"
        ))),
    }
}

/// Reconcile Docker runner containers against live database job claims.
pub async fn reconcile_runner_containers(
    docker: &Docker,
    pool: &PgPool,
    stale_minutes: i32,
) -> Result<RunnerContainerCleanupSummary> {
    let containers = list_agentics_runner_containers(docker).await?;
    let now_secs = current_unix_timestamp_secs();
    let mut summary = RunnerContainerCleanupSummary::default();
    for container in containers {
        let Some(container_id) = container.id else {
            continue;
        };
        if !matches!(container.state, Some(ContainerSummaryStateEnum::RUNNING)) {
            if is_stale_runner_container(container.created, now_secs) {
                remove_container(docker, &container_id).await?;
                summary.removed_stopped =
                    summary.removed_stopped.checked_add(1).ok_or_else(|| {
                        AppError::Internal("removed stopped container count overflow".to_string())
                    })?;
            }
            continue;
        }

        let labels = container
            .labels
            .as_ref()
            .and_then(RunnerContainerLabels::parse);
        let action = if let Some(labels) = labels {
            let db_claim = load_runner_job_claim(pool, &labels.job_id, stale_minutes).await?;
            runner_container_action(&labels, db_claim.as_ref())
        } else {
            RunnerContainerAction::RemoveRunning
        };

        match action {
            RunnerContainerAction::Keep => {}
            RunnerContainerAction::RemoveRunning => {
                kill_and_remove_container(docker, &container_id).await?;
                summary.removed_running =
                    summary.removed_running.checked_add(1).ok_or_else(|| {
                        AppError::Internal("removed running container count overflow".to_string())
                    })?;
            }
        }
    }

    Ok(summary)
}

/// Remove old stopped Agentics runner containers left by earlier worker attempts.
pub async fn remove_stopped_runner_containers(docker: &Docker) -> Result<u64> {
    let containers = list_agentics_runner_containers(docker).await?;
    remove_stopped_runner_containers_from_list(docker, containers).await
}

/// List every Docker container carrying the Agentics runner label.
async fn list_agentics_runner_containers(
    docker: &Docker,
) -> Result<Vec<bollard::models::ContainerSummary>> {
    let mut filters = HashMap::new();
    filters.insert(
        "label",
        vec![
            format!(
                "{}={}",
                super::RUNNER_KIND_LABEL,
                super::RUNNER_KIND_ZIP_PROJECT
            ),
            format!(
                "{}={}",
                super::RUNNER_SCOPE_LABEL,
                super::RUNNER_SCOPE_HOSTED_WORKER
            ),
        ],
    );
    let options = ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&filters)
        .build();
    let containers = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| AppError::Docker(format!("list runner containers failed: {e}")))?;
    Ok(containers
        .into_iter()
        .filter(container_has_hosted_runner_scope)
        .collect())
}

/// Return true only for containers owned by hosted worker reconciliation.
fn container_has_hosted_runner_scope(container: &bollard::models::ContainerSummary) -> bool {
    container
        .labels
        .as_ref()
        .and_then(|labels| labels.get(super::RUNNER_SCOPE_LABEL))
        .is_some_and(|scope| scope == super::RUNNER_SCOPE_HOSTED_WORKER)
}

/// Remove stale stopped containers from a pre-fetched Docker container list.
async fn remove_stopped_runner_containers_from_list(
    docker: &Docker,
    containers: Vec<bollard::models::ContainerSummary>,
) -> Result<u64> {
    let now_secs = current_unix_timestamp_secs();
    let mut removed = 0u64;
    for container in containers {
        if matches!(container.state, Some(ContainerSummaryStateEnum::RUNNING)) {
            continue;
        }
        if !is_stale_runner_container(container.created, now_secs) {
            continue;
        }
        let Some(container_id) = container.id else {
            continue;
        };
        remove_container(docker, &container_id).await?;
        removed = removed
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("removed container count overflow".to_string()))?;
    }

    Ok(removed)
}

/// Parsed labels that bind a runner container to one database claim.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerContainerLabels {
    job_id: EvaluationJobId,
    worker_id: String,
    attempt_count: i32,
}

impl RunnerContainerLabels {
    /// Parse required runner labels, rejecting malformed or incomplete identities.
    fn parse(labels: &HashMap<String, String>) -> Option<Self> {
        if labels.get(super::RUNNER_SCOPE_LABEL).map(String::as_str)
            != Some(super::RUNNER_SCOPE_HOSTED_WORKER)
        {
            return None;
        }
        let job_id = EvaluationJobId::try_new(labels.get("agentics.job_id")?).ok()?;
        let worker_id = labels.get("agentics.worker_id")?.to_string();
        if worker_id.trim().is_empty() {
            return None;
        }
        let attempt_count = labels.get("agentics.attempt_count")?.parse::<i32>().ok()?;
        if attempt_count <= 0 {
            return None;
        }
        Some(Self {
            job_id,
            worker_id,
            attempt_count,
        })
    }
}

/// Current database claim state for a runner job.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerJobClaim {
    status: String,
    worker_id: Option<String>,
    attempt_count: i32,
    claim_is_fresh: bool,
}

/// Cleanup action for one running Agentics runner container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunnerContainerAction {
    Keep,
    RemoveRunning,
}

/// Determine whether a running container still matches the durable job claim.
fn runner_container_action(
    labels: &RunnerContainerLabels,
    claim: Option<&RunnerJobClaim>,
) -> RunnerContainerAction {
    let Some(claim) = claim else {
        return RunnerContainerAction::RemoveRunning;
    };
    if claim.status == "running"
        && claim.worker_id.as_deref() == Some(labels.worker_id.as_str())
        && claim.attempt_count == labels.attempt_count
        && claim.claim_is_fresh
    {
        RunnerContainerAction::Keep
    } else {
        RunnerContainerAction::RemoveRunning
    }
}

/// Load the database claim corresponding to one runner container label set.
async fn load_runner_job_claim(
    pool: &PgPool,
    job_id: &EvaluationJobId,
    stale_minutes: i32,
) -> Result<Option<RunnerJobClaim>> {
    let row: Option<(String, Option<String>, i32, bool)> = sqlx::query_as(
        r#"
        SELECT
            status,
            worker_id,
            attempt_count,
            claimed_at IS NOT NULL
              AND claimed_at >= NOW() - INTERVAL '1 minute' * $2 AS claim_is_fresh
        FROM evaluation_jobs
        WHERE id = $1::uuid
        "#,
    )
    .bind(job_id.as_str())
    .bind(stale_minutes.max(1))
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(status, worker_id, attempt_count, claim_is_fresh)| RunnerJobClaim {
            status,
            worker_id,
            attempt_count,
            claim_is_fresh,
        },
    ))
}

/// Returns whether a stopped runner container is old enough for startup cleanup.
fn is_stale_runner_container(created_secs: Option<i64>, now_secs: i64) -> bool {
    created_secs
        .and_then(|created| now_secs.checked_sub(created))
        .is_some_and(|age_secs| age_secs >= STALE_RUNNER_CONTAINER_MIN_AGE_SECS)
}

/// Reads the current Unix timestamp for stale-container age comparisons.
fn current_unix_timestamp_secs() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
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

/// Build the Docker hardening baseline shared by runner and helper containers.
fn hardened_container_host_config(
    network_mode: String,
    mounts: Vec<Mount>,
    log_cap_bytes: u64,
    readonly_rootfs: bool,
) -> HostConfig {
    HostConfig {
        network_mode: Some(network_mode),
        mounts: Some(mounts),
        auto_remove: Some(false),
        pids_limit: Some(256),
        ulimits: Some(container_ulimits()),
        cap_drop: Some(vec!["ALL".to_string()]),
        security_opt: Some(vec!["no-new-privileges:true".to_string()]),
        privileged: Some(false),
        publish_all_ports: Some(false),
        init: Some(true),
        oom_kill_disable: Some(false),
        log_config: Some(docker_log_config(log_cap_bytes)),
        readonly_rootfs: Some(readonly_rootfs),
        ..Default::default()
    }
}

/// Build permission-repair host config with only writable bind mounts exposed.
fn permission_repair_host_config(mounts: Vec<Mount>) -> HostConfig {
    let mut config = hardened_container_host_config(
        "none".to_string(),
        mounts,
        PERMISSION_FIX_LOG_LIMIT_BYTES,
        true,
    );
    config.cap_add = Some(vec!["FOWNER".to_string()]);
    config
}

/// Return resource ulimits shared across runner and helper containers.
fn container_ulimits() -> Vec<ResourcesUlimits> {
    vec![
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
    ]
}

/// Pull an image before creating a runner container.
pub(super) async fn pre_pull_image(
    docker: &Docker,
    image: &str,
    docker_platform: DockerPlatform,
) -> Result<()> {
    use bollard::query_parameters::CreateImageOptionsBuilder;

    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }

    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .platform(docker_platform.as_str())
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

/// Handles bind mount for this module.
pub(super) fn bind_mount(path: &std::path::Path, target: &str, read_only: bool) -> Mount {
    Mount {
        target: Some(target.to_string()),
        source: Some(path.to_string_lossy().to_string()),
        typ: Some(MountTypeEnum::BIND),
        read_only: Some(read_only),
        ..Default::default()
    }
}

/// Handles run created container for this module.
async fn run_created_container(
    docker: &Docker,
    container_id: &str,
    timeout_sec: u64,
    log_cap_bytes: u64,
) -> Result<ContainerOutcome> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await
        .map_err(|e| AppError::Docker(format!("start container failed: {e}")))?;
    let started = Instant::now();

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
    let wall_time_ms = duration_millis(started.elapsed());
    let (logs, _logs_truncated) =
        collect_container_logs(docker, container_id, log_cap_bytes).await?;
    Ok(ContainerOutcome {
        exit_code,
        logs,
        timed_out,
        wall_time_ms,
    })
}

/// Handles duration millis for this module.
fn duration_millis(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

/// Handles remove container for this module.
async fn remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    docker
        .remove_container(container_id, Some(remove_opts))
        .await
        .map_err(|e| AppError::Docker(format!("remove runner container failed: {e}")))?;
    Ok(())
}

/// Force-stop and remove one running runner container.
async fn kill_and_remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let kill_opts = KillContainerOptionsBuilder::default()
        .signal("SIGKILL")
        .build();
    if let Err(error) = docker.kill_container(container_id, Some(kill_opts)).await {
        let message = error.to_string();
        if !message.contains("is not running") && !message.contains("No such container") {
            return Err(AppError::Docker(format!(
                "kill orphaned runner container failed: {error}"
            )));
        }
    }
    remove_container(docker, container_id).await
}

/// Handles docker log config for this module.
fn docker_log_config(limit_bytes: u64) -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert("max-size".to_string(), format!("{}b", limit_bytes.max(1)));
    config.insert("max-file".to_string(), "1".to_string());

    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

/// Handles docker storage opt for this module.
fn docker_storage_opt(limit_mb: Option<u64>) -> Option<HashMap<String, String>> {
    limit_mb.map(|limit_mb| {
        let mut storage_opt = HashMap::new();
        storage_opt.insert("size".to_string(), format!("{limit_mb}m"));
        storage_opt
    })
}

/// Handles gpu runtime for this module.
fn gpu_runtime(accelerator: TargetAccelerator) -> Option<String> {
    match accelerator {
        TargetAccelerator::None => None,
        TargetAccelerator::Gpu => Some("nvidia".to_string()),
    }
}

/// Handles gpu device requests for this module.
fn gpu_device_requests(accelerator: TargetAccelerator) -> Option<Vec<DeviceRequest>> {
    match accelerator {
        TargetAccelerator::None => None,
        TargetAccelerator::Gpu => Some(vec![DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(-1),
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        }]),
    }
}

/// Handles collect container logs for this module.
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

/// Handles append bounded log bytes for this module.
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

    let remaining = limit.saturating_sub(output.len());
    if chunk.len() > remaining {
        output.extend(chunk.iter().take(remaining).copied());
        *truncated = true;
    } else {
        output.extend_from_slice(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that bounded log append truncates by byte limit.
    #[test]
    fn bounded_log_append_truncates_by_byte_limit() {
        let mut output = Vec::new();
        let mut truncated = false;

        append_bounded_log_bytes(&mut output, b"abcdef", 4, &mut truncated);

        assert_eq!(output, b"abcd");
        assert!(truncated);
    }

    /// Verifies that Docker logging uses the platform-owned runner cap.
    #[test]
    fn docker_log_config_uses_platform_log_cap() {
        let config = docker_log_config(PLATFORM_CONTAINER_LOG_LIMIT_BYTES);

        assert_eq!(config.typ.as_deref(), Some("json-file"));
        assert_eq!(
            config
                .config
                .as_ref()
                .and_then(|values| values.get("max-size"))
                .map(String::as_str),
            Some("1048576b")
        );
        assert_eq!(
            config
                .config
                .as_ref()
                .and_then(|values| values.get("max-file"))
                .map(String::as_str),
            Some("1")
        );
    }

    /// Verifies permission repair only targets writable bind mounts.
    #[test]
    fn writable_bind_mounts_skip_read_only_mounts() {
        let writable = bind_mount(std::path::Path::new("/tmp/write"), "/workspace", false);
        let read_only = bind_mount(std::path::Path::new("/tmp/read"), "/challenge", true);
        let selected = writable_bind_mounts(&[writable, read_only]);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].target.as_deref(), Some("/workspace"));
    }

    /// Verifies permission-repair sidecars use the runner hardening baseline.
    #[test]
    fn permission_repair_host_config_is_hardened() {
        let mount = bind_mount(std::path::Path::new("/tmp/write"), "/workspace", false);
        let config = permission_repair_host_config(vec![mount]);

        assert_eq!(config.network_mode.as_deref(), Some("none"));
        assert_eq!(config.auto_remove, Some(false));
        assert_eq!(config.pids_limit, Some(256));
        assert_eq!(config.cap_drop.as_deref(), Some(&["ALL".to_string()][..]));
        assert_eq!(
            config.security_opt.as_deref(),
            Some(&["no-new-privileges:true".to_string()][..])
        );
        assert_eq!(config.privileged, Some(false));
        assert_eq!(config.publish_all_ports, Some(false));
        assert_eq!(config.init, Some(true));
        assert_eq!(config.oom_kill_disable, Some(false));
        assert_eq!(config.readonly_rootfs, Some(true));
        assert_eq!(config.cap_add.as_deref(), Some(&["FOWNER".to_string()][..]));
        assert_eq!(
            config
                .log_config
                .as_ref()
                .and_then(|log_config| log_config.config.as_ref())
                .and_then(|values| values.get("max-size"))
                .map(String::as_str),
            Some("4096b")
        );
    }

    /// Verifies fresh matching claims keep their runner containers.
    #[test]
    fn runner_container_action_keeps_fresh_matching_claim() {
        let labels = runner_labels("worker-a", 2);
        let claim = RunnerJobClaim {
            status: "running".to_string(),
            worker_id: Some("worker-a".to_string()),
            attempt_count: 2,
            claim_is_fresh: true,
        };

        assert_eq!(
            runner_container_action(&labels, Some(&claim)),
            RunnerContainerAction::Keep
        );
    }

    /// Verifies stale or superseded claims remove running runner containers.
    #[test]
    fn runner_container_action_removes_stale_or_superseded_claims() {
        let labels = runner_labels("worker-a", 2);

        for claim in [
            RunnerJobClaim {
                status: "queued".to_string(),
                worker_id: Some("worker-a".to_string()),
                attempt_count: 2,
                claim_is_fresh: true,
            },
            RunnerJobClaim {
                status: "running".to_string(),
                worker_id: Some("worker-b".to_string()),
                attempt_count: 2,
                claim_is_fresh: true,
            },
            RunnerJobClaim {
                status: "running".to_string(),
                worker_id: Some("worker-a".to_string()),
                attempt_count: 3,
                claim_is_fresh: true,
            },
            RunnerJobClaim {
                status: "running".to_string(),
                worker_id: Some("worker-a".to_string()),
                attempt_count: 2,
                claim_is_fresh: false,
            },
        ] {
            assert_eq!(
                runner_container_action(&labels, Some(&claim)),
                RunnerContainerAction::RemoveRunning
            );
        }
        assert_eq!(
            runner_container_action(&labels, None),
            RunnerContainerAction::RemoveRunning
        );
    }

    /// Verifies runner labels reject malformed claim identities.
    #[test]
    fn runner_container_labels_reject_malformed_identity() {
        let mut labels = HashMap::from([
            (
                crate::runner::RUNNER_SCOPE_LABEL.to_string(),
                crate::runner::RUNNER_SCOPE_HOSTED_WORKER.to_string(),
            ),
            (
                "agentics.job_id".to_string(),
                uuid::Uuid::new_v4().to_string(),
            ),
            ("agentics.worker_id".to_string(), "worker-a".to_string()),
            ("agentics.attempt_count".to_string(), "0".to_string()),
        ]);
        assert!(RunnerContainerLabels::parse(&labels).is_none());

        labels.insert("agentics.attempt_count".to_string(), "1".to_string());
        labels.insert("agentics.job_id".to_string(), "not-a-uuid".to_string());
        assert!(RunnerContainerLabels::parse(&labels).is_none());

        labels.insert(
            "agentics.job_id".to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        labels.insert(
            crate::runner::RUNNER_SCOPE_LABEL.to_string(),
            crate::runner::RUNNER_SCOPE_LOCAL_VALIDATION.to_string(),
        );
        assert!(RunnerContainerLabels::parse(&labels).is_none());
    }

    /// Build valid runner labels for classification tests.
    fn runner_labels(worker_id: &str, attempt_count: i32) -> RunnerContainerLabels {
        RunnerContainerLabels {
            job_id: EvaluationJobId::generate(),
            worker_id: worker_id.to_string(),
            attempt_count,
        }
    }
}
