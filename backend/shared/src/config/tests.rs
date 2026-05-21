use super::Config;
use secrecy::SecretString;

/// Verifies that loopback bind allows local default credentials.
#[test]
fn loopback_bind_allows_local_default_credentials() {
    assert!(test_config().validate_api_security().is_ok());
}

/// Verifies that derived debug output redacts configured secrets.
#[test]
fn config_debug_redacts_secrets() {
    let mut config = test_config();
    config.database_url = SecretString::from("postgres://agentics:secret@localhost/agentics");
    config.admin_password = SecretString::from("secret-admin-password");
    config.github_oauth_client_secret = Some(SecretString::from("secret-oauth-client"));

    let debug = format!("{config:?}");

    assert!(!debug.contains("secret@localhost"));
    assert!(!debug.contains("secret-admin-password"));
    assert!(!debug.contains("secret-oauth-client"));
    assert!(debug.contains("[REDACTED"));
}

/// Verifies local base URL helpers use explicit hosts and ports.
#[test]
fn local_base_url_helpers_use_explicit_inputs() {
    assert_eq!(
        super::default_local_api_base_url(super::DEFAULT_API_HOST, super::DEFAULT_API_PORT),
        "http://127.0.0.1:3100"
    );
    assert_eq!(
        super::default_local_web_base_url(super::DEFAULT_API_HOST, super::DEFAULT_WEB_PORT),
        "http://127.0.0.1:3001"
    );
}

/// Verifies mode config values deserialize through typed boundary parsers.
#[test]
fn mode_config_values_deserialize_through_typed_parsers() {
    assert_eq!(
        serde_json::from_value::<super::AgentRegistrationMode>(serde_json::json!("pioneer_code"))
            .unwrap(),
        super::AgentRegistrationMode::PioneerCode
    );
    assert_eq!(
        serde_json::from_value::<super::RunnerWritableStorageMode>(serde_json::json!(
            "xfs-project-quota-slots"
        ))
        .unwrap(),
        super::RunnerWritableStorageMode::XfsProjectQuotaSlots
    );
    assert_eq!(
        super::RunnerWritableStorageMode::XfsProjectQuotaSlots.as_str(),
        "xfs-project-quota-slots"
    );
    assert!(
        serde_json::from_value::<super::RunnerWritableStorageMode>(serde_json::json!(
            "xfs_project_quota_slots"
        ))
        .is_err()
    );
}

/// Verifies that default admin credentials are rejected on wildcard bind.
#[test]
fn default_admin_credentials_are_rejected_on_wildcard_bind() {
    let mut config = test_config();
    config.api_host = "0.0.0.0".to_string();

    assert!(config.validate_api_security().is_err());

    config.admin_password = SecretString::from("changed");
    assert!(config.validate_api_security().is_err());

    config.agent_registration_mode = super::AgentRegistrationMode::PioneerCode;
    config.web_session_cookie_secure = true;
    assert!(config.validate_api_security().is_ok());

    config.agent_registration_mode = super::AgentRegistrationMode::Public;
    assert!(config.validate_api_security().is_err());
}

/// Verifies that hosted API binds reject public registration mode.
#[test]
fn hosted_bind_rejects_public_agent_registration_mode() {
    let mut config = test_config();
    config.api_host = "0.0.0.0".to_string();
    config.admin_password = SecretString::from("changed");
    config.web_session_cookie_secure = true;
    config.agent_registration_mode = super::AgentRegistrationMode::Public;

    let error = config
        .validate_api_security()
        .expect_err("public mode must stay local-only");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_AGENT_REGISTRATION_MODE=public")
    );
}

/// Verifies invalid configured CORS origins fail startup validation.
#[test]
fn invalid_cors_origin_is_rejected() {
    let mut config = test_config();
    config.cors_allowed_origins = "http://localhost:3001,http://bad\nsite".to_string();

    let error = config
        .validate_api_security()
        .expect_err("invalid CORS origins should fail startup validation");

    assert!(
        error
            .to_string()
            .contains("AGENTICS_CORS_ALLOWED_ORIGINS contains invalid origin")
    );
}

/// Verifies that parses runner writable slot classes.
#[test]
fn parses_runner_writable_slot_classes() {
    let config = Config {
        runner_writable_slot_classes_mb: "1024,64 256,1024".to_string(),
        ..test_config()
    };

    assert_eq!(
        config.runner_writable_slot_classes_mb().unwrap(),
        vec![64, 256, 1024]
    );
}

/// Verifies invalid runner output and result limits are rejected.
#[test]
fn runner_output_and_result_limits_must_be_valid() {
    for (mut config, expected) in [
        (
            Config {
                runner_max_output_files: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_OUTPUT_FILES",
        ),
        (
            Config {
                runner_max_output_dirs: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_OUTPUT_DIRS",
        ),
        (
            Config {
                runner_max_output_depth: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_OUTPUT_DEPTH",
        ),
        (
            Config {
                runner_max_runs: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_RUNS",
        ),
        (
            Config {
                runner_max_runs: 13,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_RUNS",
        ),
        (
            Config {
                runner_max_result_json_bytes: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES",
        ),
        (
            Config {
                runner_max_public_results: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_PUBLIC_RESULTS",
        ),
        (
            Config {
                runner_max_result_log_bytes: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES",
        ),
        (
            Config {
                runner_max_interaction_bytes_per_direction: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION",
        ),
        (
            Config {
                runner_interaction_shutdown_grace_secs: 0,
                ..test_config()
            },
            "AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS",
        ),
    ] {
        config.api_host = "127.0.0.1".to_string();
        let error = config
            .validate_runner_storage()
            .expect_err("zero limit should be rejected");
        assert!(error.to_string().contains(expected));
    }
}

/// Verifies that hosted workers must bound bind mounts and writable rootfs.
#[test]
fn production_runner_requires_bounded_mounts_layers_and_host_probes() {
    let mut config = test_config();
    config.runner_security_profile = super::RunnerSecurityProfile::Production;
    config.require_digest_pinned_images = true;

    let error = config
        .validate_runner_storage()
        .expect_err("production workers require a writable storage boundary");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_RUNNER_SECURITY_PROFILE=production")
    );

    config.runner_docker_layer_quota = true;
    assert!(
        config.validate_runner_storage().is_err(),
        "Docker layer quota does not bound phase bind mounts"
    );

    config.runner_writable_storage_mode = super::RunnerWritableStorageMode::XfsProjectQuotaSlots;
    config.runner_docker_layer_quota = false;
    config.api_host = "127.0.0.1".to_string();
    config.runner_runtime_root = Some("/agentics-runtime".to_string());
    config.runner_phase_mount_root = Some("/agentics-runner-slots".to_string());
    let error = config
        .validate_runner_storage()
        .expect_err("quota-backed writable rootfs also needs Docker layer quota");
    assert!(error.to_string().contains("xfs-project-quota-slots"));

    config.runner_docker_layer_quota = true;
    let error = config
        .validate_runner_storage()
        .expect_err("production workers require host probes");
    if cfg!(target_os = "linux") {
        assert!(
            error
                .to_string()
                .contains("AGENTICS_RUNNER_SECURITY_PROFILE=production")
        );
    } else {
        assert!(error.to_string().contains("Linux-only"));
    }

    config.host_probe_mode = super::HostProbeMode::Require;
    assert_eq!(
        config.validate_runner_storage().is_ok(),
        cfg!(target_os = "linux")
    );
}

/// Verifies quota-backed runner storage requires a host-visible runtime root.
#[test]
fn quota_backed_runner_requires_runtime_root() {
    let config = Config {
        runner_writable_storage_mode: super::RunnerWritableStorageMode::XfsProjectQuotaSlots,
        runner_docker_layer_quota: true,
        runner_phase_mount_root: Some("/agentics-runner-slots".to_string()),
        ..test_config()
    };
    let error = config
        .validate_runner_storage()
        .expect_err("quota-backed storage must require a runtime root");
    if cfg!(target_os = "linux") {
        assert!(error.to_string().contains("AGENTICS_RUNNER_RUNTIME_ROOT"));
    } else {
        assert!(error.to_string().contains("Linux-only"));
    }

    let config = Config {
        runner_writable_storage_mode: super::RunnerWritableStorageMode::XfsProjectQuotaSlots,
        runner_docker_layer_quota: true,
        runner_runtime_root: Some("relative-runtime".to_string()),
        runner_phase_mount_root: Some("/agentics-runner-slots".to_string()),
        ..test_config()
    };
    let error = config
        .validate_runner_storage()
        .expect_err("runtime root must be absolute");
    if cfg!(target_os = "linux") {
        assert!(error.to_string().contains("absolute"));
    } else {
        assert!(error.to_string().contains("Linux-only"));
    }
}

/// Verifies hosted profiles cannot disable digest-pinned image enforcement.
#[test]
fn production_and_required_probe_profiles_require_digest_pinned_images() {
    let mut probe_config = Config {
        host_probe_mode: super::HostProbeMode::Require,
        ..test_config()
    };
    let error = probe_config
        .validate_api_security()
        .expect_err("required hosted probes imply immutable images");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES")
    );
    assert!(probe_config.requires_digest_pinned_images());

    probe_config.require_digest_pinned_images = true;
    assert!(probe_config.validate_api_security().is_ok());

    let production_config = Config {
        runner_security_profile: super::RunnerSecurityProfile::Production,
        ..test_config()
    };
    let error = production_config
        .validate_api_security()
        .expect_err("production profile implies immutable images");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES")
    );

    let local_quota_config = Config {
        runner_writable_storage_mode: super::RunnerWritableStorageMode::XfsProjectQuotaSlots,
        ..test_config()
    };
    assert!(
        !local_quota_config.requires_digest_pinned_images(),
        "local quota-backed tests can still use local images when hosted probes are off"
    );
}

/// Verifies worker accelerator config is fail-closed for GPU workers.
#[test]
fn gpu_worker_requires_probe_image_and_linux_host() {
    let mut config = Config {
        worker_accelerators: super::WorkerAccelerators::Gpu,
        ..test_config()
    };

    let error = config
        .validate_runner_storage()
        .expect_err("GPU workers need an explicit probe image");
    if cfg!(target_os = "linux") {
        assert!(
            error
                .to_string()
                .contains("AGENTICS_WORKER_GPU_PROBE_IMAGE")
        );
    } else {
        assert!(error.to_string().contains("Linux-only"));
    }

    config.worker_gpu_probe_image = Some(
            "ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        );
    assert_eq!(
        config.validate_runner_storage().is_ok(),
        cfg!(target_os = "linux")
    );
}

/// Verifies worker accelerator capability matching stays explicit.
#[test]
fn worker_accelerator_capabilities_are_explicit() {
    assert!(super::WorkerAccelerators::None.supports(super::TargetAccelerator::None));
    assert!(!super::WorkerAccelerators::None.supports(super::TargetAccelerator::Gpu));
    assert!(super::WorkerAccelerators::Gpu.supports(super::TargetAccelerator::None));
    assert!(super::WorkerAccelerators::Gpu.supports(super::TargetAccelerator::Gpu));
    assert_eq!(
        super::WorkerAccelerators::Gpu.heartbeat_values(),
        vec!["none".to_string(), "gpu".to_string()]
    );
}

/// Handles test config for this module.
fn test_config() -> Config {
    Config {
        database_url: SecretString::from(""),
        api_host: super::default_api_host(),
        api_port: super::default_api_port(),
        storage_root: String::new(),
        challenges_root: String::new(),
        admin_username: super::default_admin_username(),
        admin_password: super::default_admin_password(),
        allow_insecure_default_admin_credentials: false,
        cors_allowed_origins: super::default_cors_allowed_origins(),
        worker_poll_interval_ms: 3000,
        worker_stale_job_minutes: 1,
        worker_accelerators: super::default_worker_accelerators(),
        worker_gpu_probe_image: None,
        validation_runs_per_agent_challenge_day: 20,
        official_runs_per_agent_challenge_day: 5,
        max_active_official_jobs: 20,
        max_active_agents: 1_000,
        max_active_challenge_drafts_per_agent: 10,
        challenge_private_asset_bytes_per_draft: 250 * 1024 * 1024,
        challenge_draft_validations_per_day: 10,
        challenge_draft_validation_timeout_minutes: 30,
        challenge_private_asset_pending_timeout_minutes: 30,
        challenge_draft_publish_timeout_minutes: 30,
        challenge_draft_ttl_days: 14,
        unpublished_challenge_asset_grace_days: 7,
        github_oauth_client_id: None,
        github_oauth_client_secret: None,
        github_oauth_redirect_url: None,
        github_oauth_authorize_url: super::default_github_oauth_authorize_url(),
        github_oauth_token_url: super::default_github_oauth_token_url(),
        github_api_user_url: super::default_github_api_user_url(),
        web_session_cookie_name: super::default_web_session_cookie_name(),
        web_csrf_cookie_name: super::default_web_csrf_cookie_name(),
        web_session_ttl_hours: super::default_web_session_ttl_hours(),
        web_session_cookie_secure: false,
        agent_registration_mode: super::default_agent_registration_mode(),
        docker_host: None,
        host_probe_mode: super::default_host_probe_mode(),
        runner_security_profile: super::default_runner_security_profile(),
        require_digest_pinned_images: false,
        runner_writable_storage_mode: super::default_runner_writable_storage_mode(),
        runner_runtime_root: None,
        runner_phase_mount_root: None,
        runner_writable_slot_classes_mb: super::default_runner_writable_slot_classes_mb(),
        runner_docker_layer_quota: false,
        runner_max_output_files: super::default_runner_max_output_files(),
        runner_max_output_dirs: super::default_runner_max_output_dirs(),
        runner_max_output_depth: super::default_runner_max_output_depth(),
        runner_max_runs: super::default_runner_max_runs(),
        runner_max_result_json_bytes: super::default_runner_max_result_json_bytes(),
        runner_max_public_results: super::default_runner_max_public_results(),
        runner_max_result_log_bytes: super::default_runner_max_result_log_bytes(),
        runner_max_interaction_bytes_per_direction:
            super::default_runner_max_interaction_bytes_per_direction(),
        runner_interaction_shutdown_grace_secs:
            super::default_runner_interaction_shutdown_grace_secs(),
        log_level: "info".to_string(),
    }
}
