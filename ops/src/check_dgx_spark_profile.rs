//! Rust-native DGX Spark hosted profile checker.
//!
//! This executable oxidizes `scripts/ops/check-dgx-spark-profile.sh`. It keeps
//! config modes, phases, Docker pull policies, paths, and quota-slot metadata
//! typed internally. It uses native filesystem/proc parsing for mount checks,
//! `serde_json` for slot metadata, Bollard for Docker probes, and direct
//! `xfs_quota` process calls only for XFS quota state.
//!
//! Cancellation: `run_from_process` races the whole check set against Ctrl-C.
//! Read-only checks are idempotent. Mutating canary probes run only when
//! `AGENTICS_DGX_RUN_MUTATING_PROBES=1`; they create temporary paths and
//! containers, then clean them up best-effort. There is no dry-run because this
//! is a checker; rootful mutation belongs to the storage/profile commands.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{ContainerCreateBody, HostConfig, HostConfigLogConfig, Mount, MountType};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, LogsOptionsBuilder, RemoveContainerOptionsBuilder,
    StartContainerOptions, WaitContainerOptionsBuilder,
};
use clap::Parser;
use futures::StreamExt;
use shared::config::{HostProbeMode, RunnerSecurityProfile, RunnerWritableStorageMode};
use shared::zip_project::DockerNetworkMode;
use uuid::Uuid;

use crate::dgx::{
    DEFAULT_DOCKER_HOST_URI, DgxPhase, DgxProfileCheckConfig, DockerPullPolicy,
    ENV_DGX_RUN_MUTATING_PROBES, SlotMetadata, phase_slot_path,
};
use crate::support::{
    DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, SupportError, append_bounded_bytes, run_process,
    run_with_ctrl_c,
};

const PREFIX: &str = "agentics-dgx-check";
const DOCKER_PROBE_TIMEOUT_SECS: u64 = 120;

/// CLI for DGX hosted profile checks.
#[derive(Debug, Parser)]
#[command(
    about = "Checks the DGX Spark hosted runner profile.",
    long_about = "Checks the Agentics-owned Docker daemon, XFS project quota mounts, root-prepared writable slots, runtime-root visibility, and optional quota canaries for the DGX Spark hosted profile."
)]
pub struct Cli {
    /// Override AGENTICS_HOST_PROBE_MODE for this invocation.
    #[arg(long)]
    host_probe_mode: Option<HostProbeModeArg>,
    /// Run mutating canary probes. Falls back to AGENTICS_DGX_RUN_MUTATING_PROBES.
    #[arg(long)]
    run_mutating_probes: bool,
}

/// Clap adapter for shared host probe mode.
#[derive(Debug, Clone, Copy)]
pub struct HostProbeModeArg(HostProbeMode);

impl std::str::FromStr for HostProbeModeArg {
    type Err = crate::dgx::DgxConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        crate::dgx::parse_host_probe_mode(value).map(Self)
    }
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok((mode, reports)) => {
                for report in &reports {
                    report.print(PREFIX);
                }
                if reports.iter().any(ReportLine::is_failure) && mode != HostProbeMode::Warn {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> Result<(HostProbeMode, Vec<ReportLine>), ProfileCheckError> {
    let mut config = DgxProfileCheckConfig::from_env()?;
    if let Some(mode) = cli.host_probe_mode {
        config.host_probe_mode = mode.0;
    }
    if cli.run_mutating_probes {
        config.run_mutating_probes = true;
    }

    if config.host_probe_mode == HostProbeMode::Off {
        return Ok((
            config.host_probe_mode,
            vec![ReportLine::skip(
                "DGX profile",
                "AGENTICS_HOST_PROBE_MODE=off; skipping DGX profile checks",
            )],
        ));
    }

    let mut reports = vec![
        linux_gate(),
        expected_profile_modes(&config),
        expected_docker_host(&config),
        check_xfs_mount(&config.docker_data_root, "Agentics Docker data root"),
        check_runtime_root(&config.runner_runtime_root),
    ];
    for phase in &config.phases {
        reports.push(check_xfs_mount(
            &config.phase_mount_root.join(phase.as_str()),
            &format!("phase mount {phase}"),
        ));
    }
    reports.extend(check_slots(&config).await);
    reports.extend(check_docker_daemon(&config).await);
    reports.extend(check_mutating_probe_policy(&config).await);
    Ok((config.host_probe_mode, reports))
}

fn linux_gate() -> ReportLine {
    if cfg!(target_os = "linux") {
        ReportLine::pass("Linux gate", "running on Linux")
    } else {
        ReportLine::fail(
            "Linux gate",
            format!(
                "DGX Spark profile checks are Linux-only; detected {}",
                std::env::consts::OS
            ),
        )
    }
}

fn expected_profile_modes(config: &DgxProfileCheckConfig) -> ReportLine {
    let mut failures = Vec::new();
    if config.runner_security_profile != RunnerSecurityProfile::Production {
        failures.push("AGENTICS_RUNNER_SECURITY_PROFILE must be production");
    }
    if config.runner_storage_mode != RunnerWritableStorageMode::XfsProjectQuotaSlots {
        failures.push("AGENTICS_RUNNER_WRITABLE_STORAGE_MODE must be xfs-project-quota-slots");
    }
    if !config.runner_docker_layer_quota {
        failures.push("AGENTICS_RUNNER_DOCKER_LAYER_QUOTA must be true");
    }
    if config.runner_phase_mount_root != config.phase_mount_root {
        failures.push("AGENTICS_RUNNER_PHASE_MOUNT_ROOT must match AGENTICS_DGX_PHASE_MOUNT_ROOT");
    }
    if failures.is_empty() {
        ReportLine::pass(
            "runner profile modes",
            "production quota-backed profile configured",
        )
    } else {
        ReportLine::fail("runner profile modes", failures.join("; "))
    }
}

fn expected_docker_host(config: &DgxProfileCheckConfig) -> ReportLine {
    if config.docker_host_uri == DEFAULT_DOCKER_HOST_URI {
        ReportLine::pass("Agentics Docker host", DEFAULT_DOCKER_HOST_URI)
    } else {
        ReportLine::fail(
            "Agentics Docker host",
            format!(
                "AGENTICS_DOCKER_HOST should be {DEFAULT_DOCKER_HOST_URI}; got {}",
                config.docker_host_uri
            ),
        )
    }
}

fn check_xfs_mount(path: &Path, label: &str) -> ReportLine {
    if !path.try_exists().unwrap_or(false) || !path.is_dir() {
        return ReportLine::fail(label, format!("{} is missing", path.display()));
    }
    match find_mount(path) {
        Some(mount) if mount.fstype == "xfs" && mount.has_project_quota() => ReportLine::pass(
            label,
            format!("{} is xfs with project quotas", path.display()),
        ),
        Some(mount) if mount.fstype == "xfs" => ReportLine::fail(
            label,
            format!("{} is xfs but missing prjquota/pquota", path.display()),
        ),
        Some(mount) => ReportLine::fail(
            label,
            format!("{} is {}, expected xfs", path.display(), mount.fstype),
        ),
        None => ReportLine::fail(label, format!("no mount covers {}", path.display())),
    }
}

fn check_runtime_root(path: &Path) -> ReportLine {
    if !path.is_absolute() {
        return ReportLine::fail(
            "runner runtime root",
            format!("{} must be absolute", path.display()),
        );
    }
    if !path.is_dir() {
        return ReportLine::fail(
            "runner runtime root",
            format!("{} is missing", path.display()),
        );
    }
    if writable_probe(path) {
        ReportLine::pass(
            "runner runtime root",
            format!("{} is writable", path.display()),
        )
    } else {
        ReportLine::fail(
            "runner runtime root",
            format!("{} is not writable by this user", path.display()),
        )
    }
}

async fn check_slots(config: &DgxProfileCheckConfig) -> Vec<ReportLine> {
    let mut reports = Vec::new();
    for phase in &config.phases {
        for class_mb in &config.runner_slot_classes_mb {
            for slot_index in 1..=config.phase_slots_per_class {
                let slot_path = phase_slot_path(
                    &config.runner_phase_mount_root,
                    *phase,
                    *class_mb,
                    slot_index,
                );
                reports.push(check_slot(config, *phase, *class_mb, slot_index, &slot_path).await);
            }
        }
    }
    reports
}

async fn check_slot(
    config: &DgxProfileCheckConfig,
    phase: DgxPhase,
    class_mb: u64,
    slot_index: u64,
    slot_path: &Path,
) -> ReportLine {
    if !slot_path.is_dir() {
        return ReportLine::fail("quota slot", format!("missing {}", slot_path.display()));
    }
    let metadata_path = slot_path.join(".agentics-slot.json");
    let metadata = match tokio::fs::read_to_string(&metadata_path).await {
        Ok(text) => match serde_json::from_str::<SlotMetadata>(&text) {
            Ok(metadata) => metadata,
            Err(error) => {
                return ReportLine::fail(
                    "quota slot metadata",
                    format!("{} is invalid JSON: {error}", metadata_path.display()),
                );
            }
        },
        Err(error) => {
            return ReportLine::fail(
                "quota slot metadata",
                format!("cannot read {}: {error}", metadata_path.display()),
            );
        }
    };
    let expected_inode_limit = class_mb.saturating_mul(config.phase_slot_inodes_per_mb);
    if metadata.phase != phase
        || metadata.slot_class_mb != class_mb
        || metadata.slot_index != slot_index
        || metadata.inodes_per_mb != config.phase_slot_inodes_per_mb
        || metadata.inode_hard_limit != expected_inode_limit
    {
        return ReportLine::fail(
            "quota slot metadata",
            format!(
                "{} does not match expected phase/class/index/limits",
                metadata_path.display()
            ),
        );
    }
    let quota = check_project_inode_quota(
        &config.runner_phase_mount_root.join(phase.as_str()),
        metadata.project_id,
        expected_inode_limit,
    )
    .await;
    match quota {
        Ok(()) if writable_probe(slot_path) => {
            ReportLine::pass("quota slot", format!("{} ready", slot_path.display()))
        }
        Ok(()) => ReportLine::fail(
            "quota slot",
            format!("{} is not writable by this user", slot_path.display()),
        ),
        Err(error) => ReportLine::fail("quota slot", format!("{}: {error}", slot_path.display())),
    }
}

async fn check_project_inode_quota(
    mount_path: &Path,
    project_id: u64,
    expected_inode_limit: u64,
) -> Result<(), ProfileCheckError> {
    let output = run_process(
        "xfs_quota",
        vec![
            "-x".to_string(),
            "-c".to_string(),
            format!("quota -p -i -n -N {project_id}"),
            mount_path.to_string_lossy().to_string(),
        ],
        Some(Duration::from_secs(10)),
        DEFAULT_OUTPUT_LIMIT_BYTES,
    )
    .await?;
    if !output.success() {
        return Err(ProfileCheckError::Probe(output.combined()));
    }
    let hard = parse_project_inode_hard_limit(&output.stdout, project_id)
        .ok_or_else(|| ProfileCheckError::Probe("missing project quota row".to_string()))?;
    if hard == expected_inode_limit {
        Ok(())
    } else {
        Err(ProfileCheckError::Probe(format!(
            "inode hard limit is {hard}; expected {expected_inode_limit}"
        )))
    }
}

async fn check_docker_daemon(config: &DgxProfileCheckConfig) -> Vec<ReportLine> {
    let docker = match Docker::connect_with_host(&config.docker_host_uri) {
        Ok(docker) => docker,
        Err(error) => {
            return vec![ReportLine::fail(
                "Agentics Docker daemon",
                format!("cannot connect: {error}"),
            )];
        }
    };
    match docker.info().await {
        Ok(info) => {
            let driver = info.driver.unwrap_or_default();
            let mut reports = vec![if driver == "overlay2" {
                ReportLine::pass("Docker storage driver", "overlay2")
            } else {
                ReportLine::fail(
                    "Docker storage driver",
                    format!("expected overlay2, got {}", empty_unknown(&driver)),
                )
            }];
            let runtimes = format!("{:?}", info.runtimes);
            if runtimes.contains("nvidia") {
                reports.push(ReportLine::pass("NVIDIA Docker runtime", "visible"));
            } else {
                reports.push(ReportLine::skip(
                    "NVIDIA Docker runtime",
                    "not visible; acceptable while GPU execution remains disabled",
                ));
            }
            reports
        }
        Err(error) => vec![ReportLine::fail(
            "Agentics Docker daemon",
            error.to_string(),
        )],
    }
}

async fn check_mutating_probe_policy(config: &DgxProfileCheckConfig) -> Vec<ReportLine> {
    if !config.run_mutating_probes {
        let mut report = ReportLine::skip(
            "mutating probes",
            format!("set {ENV_DGX_RUN_MUTATING_PROBES}=1 to run canary probes"),
        );
        if config.host_probe_mode == HostProbeMode::Require {
            report = ReportLine::fail(
                "mutating probes",
                format!("{ENV_DGX_RUN_MUTATING_PROBES}=1 is required in require mode"),
            );
        }
        return vec![report];
    }
    vec![
        runtime_visibility_probe(config).await,
        docker_layer_quota_probe(config).await,
    ]
    .into_iter()
    .chain(slot_quota_probes(config).await)
    .collect()
}

async fn runtime_visibility_probe(config: &DgxProfileCheckConfig) -> ReportLine {
    let probe_dir = config
        .runner_runtime_root
        .join(format!("agentics-dgx-runtime-probe-{}", Uuid::new_v4()));
    if let Err(error) = tokio::fs::create_dir_all(&probe_dir).await {
        return ReportLine::fail("runtime bind probe", error.to_string());
    }
    let canary = probe_dir.join("canary.txt");
    if let Err(error) = tokio::fs::write(&canary, "agentics-runtime\n").await {
        let _ignored = tokio::fs::remove_dir_all(&probe_dir).await;
        return ReportLine::fail("runtime bind probe", error.to_string());
    }
    let result = run_busybox(
        config,
        vec![bind_mount(&probe_dir, "/probe", true)],
        vec!["cat".to_string(), "/probe/canary.txt".to_string()],
        None,
    )
    .await;
    let _ignored = tokio::fs::remove_dir_all(&probe_dir).await;
    match result {
        Ok(logs) if logs.trim() == "agentics-runtime" => ReportLine::pass(
            "runtime bind probe",
            "Docker can read worker-created runtime files",
        ),
        Ok(logs) => ReportLine::fail("runtime bind probe", format!("unexpected output: {logs}")),
        Err(error) => ReportLine::fail("runtime bind probe", error.to_string()),
    }
}

async fn docker_layer_quota_probe(config: &DgxProfileCheckConfig) -> ReportLine {
    let result = run_busybox(
        config,
        Vec::new(),
        vec![
            "sh".to_string(),
            "-c".to_string(),
            "dd if=/dev/zero of=/agentics-quota-probe bs=1M count=64".to_string(),
        ],
        Some(16),
    )
    .await;
    match result {
        Ok(_) => ReportLine::fail(
            "Docker writable-layer quota probe",
            "unexpectedly succeeded",
        ),
        Err(error)
            if error.to_string().contains("No space") || error.to_string().contains("quota") =>
        {
            ReportLine::pass(
                "Docker writable-layer quota probe",
                "failed with expected quota exhaustion",
            )
        }
        Err(error) => ReportLine::fail("Docker writable-layer quota probe", error.to_string()),
    }
}

async fn slot_quota_probes(config: &DgxProfileCheckConfig) -> Vec<ReportLine> {
    if config.phases.is_empty() {
        return vec![ReportLine::fail(
            "bounded slot quota probe",
            "no phases configured",
        )];
    }
    let mut reports = Vec::new();
    for phase in &config.phases {
        reports.push(slot_quota_probe(config, *phase).await);
    }
    reports
}

async fn slot_quota_probe(config: &DgxProfileCheckConfig, phase: DgxPhase) -> ReportLine {
    let slot = phase_slot_path(
        &config.runner_phase_mount_root,
        phase,
        config.slot_probe_class_mb,
        1,
    );
    let probe_path = slot.join(format!("agentics-dgx-slot-probe-{}", Uuid::new_v4()));
    if let Err(error) = tokio::fs::create_dir_all(&probe_path).await {
        return ReportLine::fail("bounded slot quota probe", error.to_string());
    }
    let result = run_busybox(
        config,
        vec![bind_mount(&probe_path, "/probe", false)],
        vec![
            "sh".to_string(),
            "-c".to_string(),
            format!(
                "dd if=/dev/zero of=/probe/quota-probe bs=1M count={}",
                config.slot_probe_class_mb.saturating_add(1)
            ),
        ],
        None,
    )
    .await;
    let _ignored = tokio::fs::remove_dir_all(&probe_path).await;
    match result {
        Ok(_) => ReportLine::fail("bounded slot quota probe", "unexpectedly succeeded"),
        Err(error)
            if error.to_string().contains("No space") || error.to_string().contains("quota") =>
        {
            ReportLine::pass(
                "bounded slot quota probe",
                format!("{phase} failed with expected quota exhaustion"),
            )
        }
        Err(error) => ReportLine::fail("bounded slot quota probe", error.to_string()),
    }
}

async fn run_busybox(
    config: &DgxProfileCheckConfig,
    mounts: Vec<Mount>,
    cmd: Vec<String>,
    storage_limit_mb: Option<u64>,
) -> Result<String, ProfileCheckError> {
    let docker = Docker::connect_with_host(&config.docker_host_uri)?;
    if config.pull_policy == DockerPullPolicy::Always
        || (config.pull_policy == DockerPullPolicy::Missing
            && docker.inspect_image(&config.probe_image).await.is_err())
    {
        use bollard::query_parameters::CreateImageOptionsBuilder;
        let opts = CreateImageOptionsBuilder::default()
            .from_image(&config.probe_image)
            .build();
        let mut stream = docker.create_image(Some(opts), None, None);
        while let Some(item) = stream.next().await {
            item?;
        }
    }
    let name = format!("agentics-dgx-profile-probe-{}", Uuid::new_v4());
    let mut host_config = HostConfig {
        network_mode: Some(DockerNetworkMode::None.as_str().to_string()),
        mounts: Some(mounts),
        auto_remove: Some(false),
        log_config: Some(probe_log_config()),
        ..Default::default()
    };
    if let Some(limit_mb) = storage_limit_mb {
        host_config.storage_opt = Some(std::collections::HashMap::from([(
            "size".to_string(),
            format!("{limit_mb}m"),
        )]));
    }
    let body = ContainerCreateBody {
        image: Some(config.probe_image.clone()),
        cmd: Some(cmd),
        host_config: Some(host_config),
        ..Default::default()
    };
    let opts = CreateContainerOptionsBuilder::default().name(&name).build();
    let response = docker.create_container(Some(opts), body).await?;
    let container_id = response.id;
    let result = async {
        docker
            .start_container(&container_id, None::<StartContainerOptions>)
            .await?;
        let mut wait = docker.wait_container(
            &container_id,
            Some(
                WaitContainerOptionsBuilder::default()
                    .condition("not-running")
                    .build(),
            ),
        );
        let status = tokio::time::timeout(Duration::from_secs(DOCKER_PROBE_TIMEOUT_SECS), async {
            let mut code = 1;
            while let Some(item) = wait.next().await {
                code = item?.status_code;
            }
            Ok::<i64, bollard::errors::Error>(code)
        })
        .await
        .map_err(|_| ProfileCheckError::Probe("Docker probe timed out".to_string()))??;
        let logs = collect_container_logs(&docker, &container_id).await?;
        Ok::<(i64, String), ProfileCheckError>((status, logs))
    }
    .await;
    let cleanup = docker
        .remove_container(
            &container_id,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    match (result, cleanup) {
        (Ok((0, logs)), Ok(())) => Ok(logs),
        (Ok((status, logs)), Ok(())) => Err(ProfileCheckError::Probe(format!(
            "container exited with {status}: {}",
            logs.trim()
        ))),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(ProfileCheckError::Docker(error)),
        (Err(error), Err(cleanup_error)) => Err(ProfileCheckError::Probe(format!(
            "{error}; additionally failed to remove Docker probe container: {cleanup_error}"
        ))),
    }
}

async fn collect_container_logs(
    docker: &Docker,
    id: &str,
) -> Result<String, bollard::errors::Error> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();
    let mut logs = docker.logs(id, Some(opts));
    let mut bytes = Vec::new();
    let mut truncated = false;
    while let Some(item) = logs.next().await {
        match item? {
            LogOutput::StdOut { message }
            | LogOutput::StdErr { message }
            | LogOutput::Console { message } => append_bounded_bytes(
                &mut bytes,
                &message,
                DEFAULT_OUTPUT_LIMIT_BYTES,
                &mut truncated,
            ),
            _ => {}
        }
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text.push_str("\n[agentics] Docker probe logs truncated\n");
    }
    Ok(text)
}

fn probe_log_config() -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert(
        "max-size".to_string(),
        format!("{}b", DEFAULT_OUTPUT_LIMIT_BYTES),
    );
    config.insert("max-file".to_string(), "1".to_string());
    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

fn bind_mount(source: &Path, target: &str, read_only: bool) -> Mount {
    Mount {
        target: Some(target.to_string()),
        source: Some(source.to_string_lossy().to_string()),
        typ: Some(MountType::BIND),
        read_only: Some(read_only),
        ..Default::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MountInfo {
    target: PathBuf,
    fstype: String,
    options: String,
    super_options: String,
}

impl MountInfo {
    fn has_project_quota(&self) -> bool {
        self.options
            .split(',')
            .chain(self.super_options.split(','))
            .any(|option| matches!(option.trim(), "prjquota" | "pquota"))
    }
}

fn find_mount(path: &Path) -> Option<MountInfo> {
    let text = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    parse_mountinfo(&text)
        .into_iter()
        .filter(|mount| path.starts_with(&mount.target))
        .max_by_key(|mount| mount.target.as_os_str().len())
}

fn parse_mountinfo(text: &str) -> Vec<MountInfo> {
    text.lines().filter_map(parse_mountinfo_line).collect()
}

fn parse_mountinfo_line(line: &str) -> Option<MountInfo> {
    let (pre, post) = line.split_once(" - ")?;
    let pre_fields = pre.split_whitespace().collect::<Vec<_>>();
    let post_fields = post.split_whitespace().collect::<Vec<_>>();
    let target = decode_mount_path(pre_fields.get(4)?);
    let options = pre_fields.get(5)?.to_string();
    let fstype = post_fields.first()?.to_string();
    let super_options = post_fields.get(2).copied().unwrap_or("").to_string();
    Some(MountInfo {
        target,
        fstype,
        options,
        super_options,
    })
}

fn decode_mount_path(value: &str) -> PathBuf {
    PathBuf::from(value.replace("\\040", " "))
}

fn parse_project_inode_hard_limit(text: &str, project_id: u64) -> Option<u64> {
    let hash_id = format!("#{project_id}");
    let plain_id = project_id.to_string();
    text.lines().find_map(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first().copied() == Some(hash_id.as_str())
            || fields.first().copied() == Some(plain_id.as_str())
        {
            fields.get(3)?.parse::<u64>().ok()
        } else {
            None
        }
    })
}

fn writable_probe(path: &Path) -> bool {
    let probe = path.join(format!(".agentics-write-probe-{}", Uuid::new_v4()));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ignored = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn empty_unknown(value: &str) -> &str {
    if value.is_empty() { "<unknown>" } else { value }
}

#[derive(Debug, thiserror::Error)]
enum ProfileCheckError {
    #[error(transparent)]
    Config(#[from] crate::dgx::DgxConfigError),
    #[error(transparent)]
    Support(#[from] SupportError),
    #[error(transparent)]
    Docker(#[from] bollard::errors::Error),
    #[error("{0}")]
    Probe(String),
}

#[cfg(test)]
mod tests {
    use super::{parse_mountinfo_line, parse_project_inode_hard_limit};

    /// Verifies mountinfo parsing extracts target, fstype, and quota options.
    #[test]
    fn parses_mountinfo_line() {
        let line = "26 23 0:22 / /srv/agentics rw,relatime - xfs /dev/loop0 rw,prjquota";
        let mount = parse_mountinfo_line(line).unwrap();
        assert_eq!(mount.target.to_string_lossy(), "/srv/agentics");
        assert_eq!(mount.fstype, "xfs");
        assert!(mount.has_project_quota());
    }

    /// Verifies xfs_quota report parsing uses the hard inode column.
    #[test]
    fn parses_project_inode_hard_limit() {
        let report = "#100001      0      0  16384      0      0";
        assert_eq!(parse_project_inode_hard_limit(report, 100001), Some(16384));
        assert_eq!(parse_project_inode_hard_limit(report, 7), None);
    }
}
