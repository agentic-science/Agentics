use super::options::append_bounded_log_bytes;
use super::*;
use test_that::prelude::*;

/// Verifies that bounded log append truncates by byte limit.
#[test]
fn bounded_log_append_truncates_by_byte_limit() {
    let mut output = Vec::new();
    let mut truncated = false;

    append_bounded_log_bytes(&mut output, b"abcdef", 4, &mut truncated);

    assert_that!(output, eq(b"abcd".to_vec()));
    assert_that!(truncated, eq(true));
}

/// Verifies that Docker logging uses the platform-owned runner cap.
#[test]
fn docker_log_config_uses_platform_log_cap() {
    let config = docker_log_config(PLATFORM_CONTAINER_LOG_LIMIT_BYTES);

    assert_that!(config.typ.as_deref(), some(eq("json-file")));
    assert_that!(
        config
            .config
            .as_ref()
            .and_then(|values| values.get("max-size"))
            .map(String::as_str),
        some(eq("1048576b"))
    );
    assert_that!(
        config
            .config
            .as_ref()
            .and_then(|values| values.get("max-file"))
            .map(String::as_str),
        some(eq("1"))
    );
}

/// Verifies permission repair only targets writable bind mounts.
#[test]
fn writable_bind_mounts_skip_read_only_mounts() {
    let writable = bind_mount(std::path::Path::new("/tmp/write"), "/workspace", false);
    let read_only = bind_mount(std::path::Path::new("/tmp/read"), "/challenge", true);
    let selected = writable_bind_mounts(&[writable, read_only]);

    assert_that!(selected, len(eq(1)));
    assert_that!(selected[0].target.as_deref(), some(eq("/workspace")));
}

/// Verifies accelerator requests enforce the declared profile count.
#[test]
fn accelerator_device_requests_use_declared_count() {
    let requests = accelerator_device_requests(TargetAccelerator::Gpu, Some(2))
        .expect("declared accelerator count should build device request")
        .expect("gpu accelerator should request devices");

    assert_that!(requests, len(eq(1)));
    assert_that!(requests[0].count, some(eq(2)));
    assert_that!(requests[0].driver.as_deref(), some(eq("nvidia")));
    assert_that!(
        requests[0].capabilities.as_deref(),
        some(points_to(
            contains_exactly![eq(vec!["gpu".to_string()])].in_order()
        ))
    );

    let error = accelerator_device_requests(TargetAccelerator::Gpu, None)
        .expect_err("gpu accelerator requires a declared count");
    assert_that!(error.to_string(), contains_substring("gpu_count"));
    assert_that!(
        accelerator_device_requests(TargetAccelerator::None, Some(2))
            .expect("no accelerator should ignore accelerator count")
            .is_none(),
        eq(true)
    );
}

/// Verifies permission-repair sidecars use the runner hardening baseline.
#[test]
fn permission_repair_host_config_is_hardened() {
    let mount = bind_mount(std::path::Path::new("/tmp/write"), "/workspace", false);
    let config = permission_repair_host_config(vec![mount]);

    assert_that!(config.network_mode.as_deref(), some(eq("none")));
    assert_that!(config.auto_remove, some(eq(false)));
    assert_that!(config.pids_limit, some(eq(256)));
    assert_that!(
        config.cap_drop.as_deref(),
        some(points_to(
            contains_exactly![eq("ALL".to_string())].in_order()
        ))
    );
    assert_that!(
        config.security_opt.as_deref(),
        some(points_to(
            contains_exactly![eq("no-new-privileges:true".to_string())].in_order()
        ))
    );
    assert_that!(config.privileged, some(eq(false)));
    assert_that!(config.publish_all_ports, some(eq(false)));
    assert_that!(config.init, some(eq(true)));
    assert_that!(config.oom_kill_disable, some(eq(false)));
    assert_that!(config.readonly_rootfs, some(eq(true)));
    assert_that!(
        config.cap_add.as_deref(),
        some(points_to(
            contains_exactly![eq("FOWNER".to_string())].in_order()
        ))
    );
    assert_that!(
        config
            .log_config
            .as_ref()
            .and_then(|log_config| log_config.config.as_ref())
            .and_then(|values| values.get("max-size"))
            .map(String::as_str),
        some(eq("4096b"))
    );
}

/// Verifies runner labels reject malformed claim identities.
#[test]
fn runner_container_labels_reject_malformed_identity() {
    let mut labels = HashMap::from([
        (
            crate::RUNNER_SCOPE_LABEL.to_string(),
            crate::RUNNER_SCOPE_HOSTED_WORKER.to_string(),
        ),
        (
            "agentics.job_id".to_string(),
            uuid::Uuid::new_v4().to_string(),
        ),
        ("agentics.worker_id".to_string(), "worker-a".to_string()),
        ("agentics.attempt_count".to_string(), "0".to_string()),
    ]);
    assert_that!(RunnerContainerLabels::parse(&labels), none());

    labels.insert("agentics.attempt_count".to_string(), "1".to_string());
    labels.insert("agentics.job_id".to_string(), "not-a-uuid".to_string());
    assert_that!(RunnerContainerLabels::parse(&labels), none());

    labels.insert(
        "agentics.job_id".to_string(),
        uuid::Uuid::new_v4().to_string(),
    );
    labels.insert(
        crate::RUNNER_SCOPE_LABEL.to_string(),
        crate::RUNNER_SCOPE_LOCAL_VALIDATION.to_string(),
    );
    assert_that!(RunnerContainerLabels::parse(&labels), none());
}

/// Verifies scope filtering separates hosted workers from local validation.
#[test]
fn runner_container_scope_filter_matches_requested_scope() {
    let namespace = test_namespace("compose-dev");
    let container = bollard::models::ContainerSummary {
        labels: Some(HashMap::from([
            (
                crate::RUNNER_NAMESPACE_LABEL.to_string(),
                namespace.as_str().to_string(),
            ),
            (
                crate::RUNNER_SCOPE_LABEL.to_string(),
                crate::RUNNER_SCOPE_LOCAL_VALIDATION.to_string(),
            ),
        ])),
        ..Default::default()
    };

    assert_that!(
        container_has_runner_scope(&container, crate::RUNNER_SCOPE_LOCAL_VALIDATION),
        eq(true)
    );
    assert_that!(
        container_has_runner_namespace(&container, &namespace),
        eq(true)
    );
    assert_that!(
        container_has_runner_scope(&container, crate::RUNNER_SCOPE_HOSTED_WORKER),
        eq(false)
    );
    assert_that!(
        container_has_runner_namespace(&container, &test_namespace("compose-test")),
        eq(false)
    );
}

fn test_namespace(value: &str) -> agentics_config::RunnerNamespace {
    agentics_config::RunnerNamespace::try_new(value).expect("test namespace should be valid")
}
