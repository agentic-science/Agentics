//! Mutating Docker canary probes for the DGX Spark profile checker.

use std::path::Path;
use std::time::Duration;

use agentics_contracts::zip_project::DockerNetworkMode;
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{ContainerCreateBody, HostConfig, HostConfigLogConfig, Mount, MountType};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, InspectContainerOptions, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions,
};
use futures::StreamExt;
use uuid::Uuid;

use crate::dgx::{DgxPhase, DgxProfileCheckConfig, DockerPullPolicy, phase_slot_path};
use crate::support::{DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, append_bounded_bytes};

use super::ProfileCheckError;

const DOCKER_PROBE_TIMEOUT_SECS: u64 = 120;

pub(super) async fn runtime_visibility_probe(config: &DgxProfileCheckConfig) -> ReportLine {
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

pub(super) async fn docker_layer_quota_probe(config: &DgxProfileCheckConfig) -> ReportLine {
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

pub(super) async fn slot_quota_probes(config: &DgxProfileCheckConfig) -> Vec<ReportLine> {
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
    let docker = connect_probe_docker(config).await?;
    let name = format!("agentics-dgx-profile-probe-{}", Uuid::new_v4());
    let body = busybox_container_body(config, mounts, cmd, storage_limit_mb);
    let opts = CreateContainerOptionsBuilder::default().name(&name).build();
    let response = docker.create_container(Some(opts), body).await?;
    let container_id = response.id;
    let result = wait_for_probe_container(&docker, &container_id).await;
    let cleanup = remove_probe_container(&docker, &container_id).await;
    finish_probe_result(result, cleanup)
}

async fn connect_probe_docker(config: &DgxProfileCheckConfig) -> Result<Docker, ProfileCheckError> {
    let docker = Docker::connect_with_host(&config.docker_host_uri)?;
    ensure_probe_image(&docker, &config.probe_image, config.pull_policy).await?;
    Ok(docker)
}

async fn ensure_probe_image(
    docker: &Docker,
    image: &str,
    pull_policy: DockerPullPolicy,
) -> Result<(), ProfileCheckError> {
    let should_pull = pull_policy == DockerPullPolicy::Always
        || (pull_policy == DockerPullPolicy::Missing && docker.inspect_image(image).await.is_err());
    if !should_pull {
        return Ok(());
    }

    use bollard::query_parameters::CreateImageOptionsBuilder;
    let opts = CreateImageOptionsBuilder::default()
        .from_image(image)
        .build();
    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(item) = stream.next().await {
        item?;
    }
    Ok(())
}

fn busybox_container_body(
    config: &DgxProfileCheckConfig,
    mounts: Vec<Mount>,
    cmd: Vec<String>,
    storage_limit_mb: Option<u64>,
) -> ContainerCreateBody {
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
    ContainerCreateBody {
        image: Some(config.probe_image.clone()),
        cmd: Some(cmd),
        host_config: Some(host_config),
        ..Default::default()
    }
}

async fn wait_for_probe_container(
    docker: &Docker,
    container_id: &str,
) -> Result<(i64, String), ProfileCheckError> {
    docker
        .start_container(container_id, None::<StartContainerOptions>)
        .await?;
    let status = wait_for_container_exit(docker, container_id).await?;
    let logs = collect_container_logs(docker, container_id).await?;
    Ok((status, logs))
}

async fn wait_for_container_exit(
    docker: &Docker,
    container_id: &str,
) -> Result<i64, ProfileCheckError> {
    tokio::time::timeout(Duration::from_secs(DOCKER_PROBE_TIMEOUT_SECS), async {
        loop {
            let container = docker
                .inspect_container(container_id, None::<InspectContainerOptions>)
                .await?;
            if let Some(state) = container.state
                && state.running != Some(true)
            {
                return Ok(state.exit_code.unwrap_or(1));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| ProfileCheckError::Probe("Docker probe timed out".to_string()))?
    .map_err(ProfileCheckError::Docker)
}

async fn remove_probe_container(
    docker: &Docker,
    container_id: &str,
) -> Result<(), bollard::errors::Error> {
    docker
        .remove_container(
            container_id,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await
}

fn finish_probe_result(
    result: Result<(i64, String), ProfileCheckError>,
    cleanup: Result<(), bollard::errors::Error>,
) -> Result<String, ProfileCheckError> {
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

#[cfg(test)]
mod tests {
    use super::finish_probe_result;
    use crate::check_dgx_spark_profile::ProfileCheckError;

    /// Verifies probe result classification preserves successful logs and exit failures.
    #[test]
    fn classifies_probe_container_result() {
        assert_eq!(
            finish_probe_result(Ok((0, "ok".to_string())), Ok(()))
                .expect("successful probe should return logs"),
            "ok",
        );
        let error = finish_probe_result(Ok((7, "bad".to_string())), Ok(()))
            .expect_err("nonzero probe exit should fail");

        assert!(matches!(error, ProfileCheckError::Probe(message) if message.contains("7")));
    }
}
