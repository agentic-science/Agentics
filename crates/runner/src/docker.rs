use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::models::{
    ContainerCreateBody, ContainerSummaryStateEnum, HostConfig, Mount, MountType, ResourcesUlimits,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, KillContainerOptionsBuilder, ListContainersOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions, WaitContainerOptionsBuilder,
};
use futures::StreamExt;
use sqlx::PgPool;
use tokio::time::timeout;

use agentics_config::{Config, RunnerNamespace};
use agentics_contracts::zip_project::{DockerNetworkMode, ZipProjectPhaseLimits};
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::challenge::{DockerPlatform, TargetAccelerator};
use agentics_domain::models::ids::EvaluationJobId;

mod interactive;
mod options;
use interactive::run_attached_interactive_pair;
use options::{
    accelerator_device_requests, collect_container_logs, docker_log_config, docker_storage_opt,
};

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
    pub(super) accelerator_count: Option<u32>,
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

#[derive(Debug)]
/// Carries two-container interactive session outcome data.
pub(super) struct InteractiveSessionOutcome {
    pub(super) participant: ContainerOutcome,
    pub(super) interactive_evaluator: ContainerOutcome,
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
    let timeout_sec = request.limits.timeout_sec;
    let log_cap_bytes = PLATFORM_CONTAINER_LOG_LIMIT_BYTES;
    let container_id = create_container(docker, request, false).await?;

    let run_result = run_created_container(docker, &container_id, timeout_sec, log_cap_bytes).await;
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
        (Ok(_), Err(permission_err), Err(cleanup_err)) => Err(ServiceError::Docker(format!(
            "{permission_err}; additionally failed to remove runner container: {cleanup_err}"
        ))),
        (Err(run_err), Ok(()), Ok(())) => Err(run_err),
        (Err(run_err), Err(permission_err), Ok(())) => Err(ServiceError::Docker(format!(
            "{run_err}; additionally failed to repair bind mount permissions: {permission_err}"
        ))),
        (Err(run_err), Ok(()), Err(cleanup_err)) => Err(ServiceError::Docker(format!(
            "{run_err}; additionally failed to remove runner container: {cleanup_err}"
        ))),
        (Err(run_err), Err(permission_err), Err(cleanup_err)) => {
            Err(ServiceError::Docker(format!(
                "{run_err}; additionally failed to repair bind mount permissions: {permission_err}; additionally failed to remove runner container: {cleanup_err}"
            )))
        }
    }
}

/// Run one participant container and one trusted interactive-evaluator container with crossed stdio.
pub(super) async fn run_interactive_stdio_session(
    docker: &Docker,
    participant: ContainerRequest,
    interactive_evaluator: ContainerRequest,
    max_interaction_bytes_per_direction: u64,
    shutdown_grace_secs: u64,
) -> Result<InteractiveSessionOutcome> {
    let participant_fix = PermissionRepairRequest::from_container_request(&participant);
    let interactive_evaluator_fix =
        PermissionRepairRequest::from_container_request(&interactive_evaluator);
    let timeout_sec = participant
        .limits
        .timeout_sec
        .max(interactive_evaluator.limits.timeout_sec);
    let participant_id = create_container(docker, participant, true).await?;
    let interactive_evaluator_id = match create_container(docker, interactive_evaluator, true).await
    {
        Ok(container_id) => container_id,
        Err(create_error) => {
            return match remove_container(docker, &participant_id).await {
                Ok(()) => Err(create_error),
                Err(cleanup_error) => Err(ServiceError::Docker(format!(
                    "{create_error}; additionally failed to remove participant container: {cleanup_error}"
                ))),
            };
        }
    };

    let run_result = run_attached_interactive_pair(
        docker,
        &participant_id,
        &interactive_evaluator_id,
        timeout_sec,
        max_interaction_bytes_per_direction,
        shutdown_grace_secs,
    )
    .await;
    let participant_permission = participant_fix.repair(docker).await;
    let interactive_evaluator_permission = interactive_evaluator_fix.repair(docker).await;
    let participant_remove = remove_container(docker, &participant_id).await;
    let interactive_evaluator_remove = remove_container(docker, &interactive_evaluator_id).await;

    combine_interactive_cleanup_results(
        run_result,
        participant_permission,
        interactive_evaluator_permission,
        participant_remove,
        interactive_evaluator_remove,
    )
}

/// Information needed to repair writable bind mount permissions after a container exits.
struct PermissionRepairRequest {
    image: String,
    docker_platform: DockerPlatform,
    mounts: Vec<Mount>,
    labels: HashMap<String, String>,
}

impl PermissionRepairRequest {
    /// Capture permission-repair inputs before the container request is consumed.
    fn from_container_request(request: &ContainerRequest) -> Self {
        Self {
            image: request.image.clone(),
            docker_platform: request.docker_platform,
            mounts: writable_bind_mounts(&request.mounts),
            labels: request.labels.clone(),
        }
    }

    /// Run the permission-repair helper for this request.
    async fn repair(self, docker: &Docker) -> Result<()> {
        repair_bind_mount_permissions(
            docker,
            self.image,
            self.docker_platform,
            self.mounts,
            self.labels,
        )
        .await
    }
}

/// Return writable bind mounts that may need host-side permission repair.
fn writable_bind_mounts(mounts: &[Mount]) -> Vec<Mount> {
    mounts
        .iter()
        .filter(|mount| {
            mount.typ == Some(MountType::BIND)
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
    labels.insert(
        super::RUNNER_PHASE_LABEL.to_string(),
        "permission-fix".to_string(),
    );

    let host_config = permission_repair_host_config(mounts);
    let container_config = ContainerCreateBody {
        image: Some(image),
        entrypoint: Some(Vec::<String>::new()),
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
        .map_err(|e| {
            ServiceError::Docker(format!("create permission repair container failed: {e}"))
        })?;
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
            Err(ServiceError::Docker(format!(
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
        (Err(run_err), Err(cleanup_err)) => Err(ServiceError::Docker(format!(
            "{run_err}; additionally failed to remove permission repair container: {cleanup_err}"
        ))),
    }
}

/// Reconcile Docker runner containers against live database job claims.
pub(crate) async fn reconcile_runner_containers(
    docker: &Docker,
    pool: &PgPool,
    stale_minutes: i32,
    runner_namespace: &RunnerNamespace,
) -> Result<RunnerContainerCleanupSummary> {
    let containers = list_agentics_runner_containers(
        docker,
        super::RUNNER_SCOPE_HOSTED_WORKER,
        runner_namespace,
    )
    .await?;
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
                        ServiceError::Internal(
                            "removed stopped container count overflow".to_string(),
                        )
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
                        ServiceError::Internal(
                            "removed running container count overflow".to_string(),
                        )
                    })?;
            }
        }
    }

    Ok(summary)
}

/// Remove old stopped Agentics runner containers left by earlier worker attempts.
pub(crate) async fn remove_stopped_runner_containers(
    docker: &Docker,
    runner_namespace: &RunnerNamespace,
) -> Result<u64> {
    let containers = list_agentics_runner_containers(
        docker,
        super::RUNNER_SCOPE_HOSTED_WORKER,
        runner_namespace,
    )
    .await?;
    remove_stopped_runner_containers_from_list(docker, containers).await
}

/// Remove stale local-validation containers left by interrupted CLI runs.
pub(crate) async fn remove_stale_local_validation_containers(
    docker: &Docker,
    runner_namespace: &RunnerNamespace,
) -> Result<RunnerContainerCleanupSummary> {
    let containers = list_agentics_runner_containers(
        docker,
        super::RUNNER_SCOPE_LOCAL_VALIDATION,
        runner_namespace,
    )
    .await?;
    let now_secs = current_unix_timestamp_secs();
    let mut summary = RunnerContainerCleanupSummary::default();
    for container in containers {
        if !is_stale_runner_container(container.created, now_secs) {
            continue;
        }
        let Some(container_id) = container.id else {
            continue;
        };
        if matches!(container.state, Some(ContainerSummaryStateEnum::RUNNING)) {
            kill_and_remove_container(docker, &container_id).await?;
            summary.removed_running = summary.removed_running.checked_add(1).ok_or_else(|| {
                ServiceError::Internal(
                    "removed local validation container count overflow".to_string(),
                )
            })?;
        } else {
            remove_container(docker, &container_id).await?;
            summary.removed_stopped = summary.removed_stopped.checked_add(1).ok_or_else(|| {
                ServiceError::Internal(
                    "removed local validation container count overflow".to_string(),
                )
            })?;
        }
    }
    Ok(summary)
}

/// List every Docker container carrying the Agentics runner label for one scope.
async fn list_agentics_runner_containers(
    docker: &Docker,
    scope: &str,
    runner_namespace: &RunnerNamespace,
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
                super::RUNNER_NAMESPACE_LABEL,
                runner_namespace.as_str()
            ),
            format!("{}={}", super::RUNNER_SCOPE_LABEL, scope),
        ],
    );
    let options = ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&filters)
        .build();
    let containers = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| ServiceError::Docker(format!("list runner containers failed: {e}")))?;
    Ok(containers
        .into_iter()
        .filter(|container| {
            container_has_runner_scope(container, scope)
                && container_has_runner_namespace(container, runner_namespace)
        })
        .collect())
}

/// Return true only for containers owned by the requested runner scope.
fn container_has_runner_scope(container: &bollard::models::ContainerSummary, scope: &str) -> bool {
    container
        .labels
        .as_ref()
        .and_then(|labels| labels.get(super::RUNNER_SCOPE_LABEL))
        .is_some_and(|value| value == scope)
}

/// Return true only for containers owned by the requested runner namespace.
fn container_has_runner_namespace(
    container: &bollard::models::ContainerSummary,
    runner_namespace: &RunnerNamespace,
) -> bool {
    container
        .labels
        .as_ref()
        .and_then(|labels| labels.get(super::RUNNER_NAMESPACE_LABEL))
        .is_some_and(|value| value == runner_namespace.as_str())
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
        removed = removed.checked_add(1).ok_or_else(|| {
            ServiceError::Internal("removed container count overflow".to_string())
        })?;
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
        let job_id = EvaluationJobId::try_new(labels.get(super::RUNNER_JOB_ID_LABEL)?).ok()?;
        let worker_id = labels.get(super::RUNNER_WORKER_ID_LABEL)?.to_string();
        if worker_id.trim().is_empty() {
            return None;
        }
        let attempt_count = labels
            .get(super::RUNNER_ATTEMPT_COUNT_LABEL)?
            .parse::<i32>()
            .ok()?;
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
        Some(host) => Docker::connect_with_host(host).map_err(|e| {
            ServiceError::Docker(format!("failed to connect to Docker host {host}: {e}"))
        }),
        None => Docker::connect_with_defaults()
            .map_err(|e| ServiceError::Docker(format!("failed to connect to Docker: {e}"))),
    }
}

/// Build the Docker hardening baseline shared by runner and helper containers.
fn hardened_container_host_config(
    network_mode: DockerNetworkMode,
    mounts: Vec<Mount>,
    log_cap_bytes: u64,
    readonly_rootfs: bool,
) -> HostConfig {
    HostConfig {
        network_mode: Some(network_mode.as_str().to_string()),
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
        DockerNetworkMode::None,
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
            return Err(ServiceError::Docker(format!(
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
        typ: Some(MountType::BIND),
        read_only: Some(read_only),
        ..Default::default()
    }
}

/// Create a runner container using the standard hardening and resource limits.
async fn create_container(
    docker: &Docker,
    request: ContainerRequest,
    attach_stdio: bool,
) -> Result<String> {
    let memory_bytes = request
        .limits
        .memory_limit_mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| ServiceError::Runner("memory limit overflow".to_string()))?;
    let memory = i64::try_from(memory_bytes)
        .map_err(|_| ServiceError::Runner("memory limit exceeds Docker API range".to_string()))?;
    let nano_cpus = i64::from(request.limits.cpu_limit_millis)
        .checked_mul(1_000_000)
        .ok_or_else(|| ServiceError::Runner("CPU limit overflow".to_string()))?;
    let mut host_config = hardened_container_host_config(
        request.limits.network_access.docker_network_mode(),
        request.mounts,
        PLATFORM_CONTAINER_LOG_LIMIT_BYTES,
        false,
    );
    host_config.memory = Some(memory);
    host_config.memory_swap = Some(memory);
    host_config.nano_cpus = Some(nano_cpus);
    host_config.storage_opt = docker_storage_opt(request.docker_layer_quota_mb);
    host_config.device_requests =
        accelerator_device_requests(request.accelerator, request.accelerator_count)?;

    let container_config = ContainerCreateBody {
        image: Some(request.image),
        entrypoint: Some(Vec::<String>::new()),
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
        attach_stdin: attach_stdio.then_some(true),
        attach_stdout: attach_stdio.then_some(true),
        attach_stderr: attach_stdio.then_some(true),
        open_stdin: attach_stdio.then_some(true),
        stdin_once: attach_stdio.then_some(false),
        tty: Some(false),
        ..Default::default()
    };

    let create_opts = CreateContainerOptionsBuilder::default()
        .name(&request.name)
        .platform(request.docker_platform.as_str())
        .build();
    let create_resp = docker
        .create_container(Some(create_opts), container_config)
        .await
        .map_err(|e| ServiceError::Docker(format!("create container failed: {e}")))?;
    Ok(create_resp.id)
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
        .map_err(|e| ServiceError::Docker(format!("start container failed: {e}")))?;
    let started = Instant::now();

    let wait_opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();
    let wait_result = timeout(Duration::from_secs(timeout_sec), async {
        let mut results = docker.wait_container(container_id, Some(wait_opts));
        let mut exit_code = None;
        while let Some(result) = results.next().await {
            match result {
                Ok(status) => exit_code = Some(status.status_code),
                Err(bollard::errors::Error::DockerContainerWaitError { code, .. }) => {
                    exit_code = Some(code);
                }
                Err(error) => {
                    return Err(ServiceError::Docker(format!(
                        "wait container failed: {error}"
                    )));
                }
            }
        }
        Ok::<i64, ServiceError>(exit_code.unwrap_or(1))
    })
    .await;

    let (exit_code, timed_out) = match wait_result {
        Ok(exit_code) => (exit_code?, false),
        Err(_) => {
            let kill_opts = KillContainerOptionsBuilder::default()
                .signal("SIGKILL")
                .build();
            docker
                .kill_container(container_id, Some(kill_opts))
                .await
                .map_err(|e| {
                    ServiceError::Docker(format!("kill timed out container failed: {e}"))
                })?;
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

/// Wait until one started container exits and return its exit code.
async fn wait_container_exit(docker: &Docker, container_id: &str) -> Result<i64> {
    let wait_opts = WaitContainerOptionsBuilder::default()
        .condition("not-running")
        .build();
    let mut results = docker.wait_container(container_id, Some(wait_opts));
    let mut exit_code = 1;
    while let Some(result) = results.next().await {
        match result {
            Ok(status) => exit_code = status.status_code,
            Err(bollard::errors::Error::DockerContainerWaitError { code, .. }) => {
                exit_code = code;
            }
            Err(error) => {
                return Err(ServiceError::Docker(format!(
                    "wait container failed: {error}"
                )));
            }
        }
    }
    Ok(exit_code)
}

/// Handles duration millis for this module.
fn duration_millis(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

/// Handles remove container for this module.
async fn remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
    if let Err(error) = docker
        .remove_container(container_id, Some(remove_opts))
        .await
    {
        let message = error.to_string();
        if !is_benign_remove_race(&message) {
            return Err(ServiceError::Docker(format!(
                "remove runner container failed: {error}"
            )));
        }
    }
    Ok(())
}

/// Docker may report an idempotent cleanup race when another cleanup path wins.
fn is_benign_remove_race(message: &str) -> bool {
    message.contains("No such container")
        || (message.contains("removal of container") && message.contains("already in progress"))
}

/// Force-stop and remove one running runner container.
async fn kill_and_remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    let kill_opts = KillContainerOptionsBuilder::default()
        .signal("SIGKILL")
        .build();
    if let Err(error) = docker.kill_container(container_id, Some(kill_opts)).await {
        let message = error.to_string();
        if !message.contains("is not running") && !message.contains("No such container") {
            return Err(ServiceError::Docker(format!(
                "kill orphaned runner container failed: {error}"
            )));
        }
    }
    remove_container(docker, container_id).await
}

/// Kill a started container, ignoring the benign case where it has already exited.
async fn kill_container_if_running(docker: &Docker, container_id: &str) -> Result<()> {
    let kill_opts = KillContainerOptionsBuilder::default()
        .signal("SIGKILL")
        .build();
    if let Err(error) = docker.kill_container(container_id, Some(kill_opts)).await {
        let message = error.to_string();
        if !message.contains("is not running") && !message.contains("No such container") {
            return Err(ServiceError::Docker(format!(
                "kill interactive runner container failed: {error}"
            )));
        }
    }
    Ok(())
}

/// Preserve the primary interactive run error while surfacing cleanup failures.
fn combine_interactive_cleanup_results(
    run_result: Result<InteractiveSessionOutcome>,
    participant_permission: Result<()>,
    interactive_evaluator_permission: Result<()>,
    participant_remove: Result<()>,
    interactive_evaluator_remove: Result<()>,
) -> Result<InteractiveSessionOutcome> {
    let mut cleanup_errors = Vec::new();
    for result in [
        participant_permission,
        interactive_evaluator_permission,
        participant_remove,
        interactive_evaluator_remove,
    ] {
        if let Err(error) = result {
            cleanup_errors.push(error.to_string());
        }
    }

    match (run_result, cleanup_errors.is_empty()) {
        (Ok(outcome), true) => Ok(outcome),
        (Ok(_), false) => Err(ServiceError::Docker(cleanup_errors.join("; additionally "))),
        (Err(error), true) => Err(error),
        (Err(error), false) => Err(ServiceError::Docker(format!(
            "{error}; additionally {}",
            cleanup_errors.join("; additionally ")
        ))),
    }
}

#[cfg(test)]
mod tests;
