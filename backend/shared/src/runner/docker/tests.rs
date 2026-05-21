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

/// Verifies accelerator requests enforce the declared profile count.
#[test]
fn accelerator_device_requests_use_declared_count() {
    let requests = accelerator_device_requests(TargetAccelerator::Gpu, Some(2))
        .expect("declared accelerator count should build device request")
        .expect("gpu accelerator should request devices");

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].count, Some(2));
    assert_eq!(requests[0].driver.as_deref(), Some("nvidia"));
    assert_eq!(
        requests[0].capabilities.as_deref(),
        Some(&[vec!["gpu".to_string()]][..])
    );

    let error = accelerator_device_requests(TargetAccelerator::Gpu, None)
        .expect_err("gpu accelerator requires a declared count");
    assert!(error.to_string().contains("gpu_count"));
    assert!(
        accelerator_device_requests(TargetAccelerator::None, Some(2))
            .expect("no accelerator should ignore accelerator count")
            .is_none()
    );
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

/// Verifies scope filtering separates hosted workers from local validation.
#[test]
fn runner_container_scope_filter_matches_requested_scope() {
    let container = bollard::models::ContainerSummary {
        labels: Some(HashMap::from([(
            crate::runner::RUNNER_SCOPE_LABEL.to_string(),
            crate::runner::RUNNER_SCOPE_LOCAL_VALIDATION.to_string(),
        )])),
        ..Default::default()
    };

    assert!(container_has_runner_scope(
        &container,
        crate::runner::RUNNER_SCOPE_LOCAL_VALIDATION,
    ));
    assert!(!container_has_runner_scope(
        &container,
        crate::runner::RUNNER_SCOPE_HOSTED_WORKER,
    ));
}

/// Build valid runner labels for classification tests.
fn runner_labels(worker_id: &str, attempt_count: i32) -> RunnerContainerLabels {
    RunnerContainerLabels {
        job_id: EvaluationJobId::generate(),
        worker_id: worker_id.to_string(),
        attempt_count,
    }
}
