#![allow(
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
)]

use super::Config;
use secrecy::{ExposeSecret, SecretString};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
        super::local_api_base_url(super::DEFAULT_API_HOST, super::DEFAULT_API_PORT),
        "http://127.0.0.1:3100"
    );
    assert_eq!(
        super::local_web_base_url(super::DEFAULT_API_HOST, super::DEFAULT_WEB_PORT),
        "http://127.0.0.1:3001"
    );
}

/// Verifies prefixed env values deserialize into grouped raw env structs.
#[test]
fn raw_app_env_deserializes_prefixed_values() {
    let raw = super::RawAppEnv::from_env_iter([
        ("AGENTICS_API_PORT".to_string(), "3222".to_string()),
        (
            "AGENTICS_ADMIN_PASSWORD".to_string(),
            "changed-password".to_string(),
        ),
        ("AGENTICS_POSTGRES_PORT".to_string(), "6543".to_string()),
    ])
    .expect("raw env should deserialize");

    let config = Config::try_from(raw).expect("raw env should convert");

    assert_eq!(config.api_port, 3222);
    assert!(
        config
            .database_url
            .expose_secret()
            .contains(":6543/agentics")
    );
    assert!(config.admin_password_matches("changed-password"));
}

/// Verifies malformed derived-default ports fail instead of falling back silently.
#[test]
fn invalid_derived_default_ports_are_rejected() {
    for (name, value) in [
        ("AGENTICS_POSTGRES_PORT", "not-a-port"),
        ("AGENTICS_WEB_PORT", "bad-web-port"),
        ("AGENTICS_API_PORT", "bad-api-port"),
    ] {
        let error = super::RawAppEnv::from_env_iter([(name.to_string(), value.to_string())])
            .expect_err("invalid port should fail during raw env parsing");
        assert!(
            error
                .to_string()
                .contains(name.trim_start_matches("AGENTICS_"))
        );
    }
}

/// Verifies generic bool env parsing does not keep legacy bool-ish aliases.
#[test]
fn bool_env_values_use_generic_deserialization() {
    let raw = super::RawAppEnv::from_env_iter([(
        "AGENTICS_S3_FORCE_PATH_STYLE".to_string(),
        "false".to_string(),
    )])
    .expect("standard bool literal should deserialize");
    let config = Config::try_from(raw).expect("raw env should convert");
    assert!(!config.s3_force_path_style);

    let error = super::RawAppEnv::from_env_iter([(
        "AGENTICS_S3_FORCE_PATH_STYLE".to_string(),
        "1".to_string(),
    )])
    .expect_err("legacy bool-ish alias should fail during raw env parsing");
    assert!(error.to_string().contains("S3_FORCE_PATH_STYLE"));
}

/// Verifies secret and hosted-probe env values fail closed when blank.
#[test]
fn blank_admin_and_probe_env_values_are_rejected() {
    let admin_error = Config::try_from(super::RawAppEnv {
        auth: super::RawAuthEnv {
            admin_password: Some("   ".to_string()),
            ..Default::default()
        },
        ..Default::default()
    })
    .expect_err("blank admin password should fail");
    assert!(admin_error.to_string().contains("AGENTICS_ADMIN_PASSWORD"));

    let probe_error = Config::try_from(super::RawAppEnv {
        runner: super::RawRunnerEnv {
            host_probe_command: Some(" ".to_string()),
            ..Default::default()
        },
        ..Default::default()
    })
    .expect_err("blank host probe command should fail");
    assert!(
        probe_error
            .to_string()
            .contains("AGENTICS_HOST_PROBE_COMMAND")
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
    assert_eq!(
        serde_json::from_value::<super::RunnerNamespace>(serde_json::json!("compose-dev_1"))
            .unwrap()
            .as_str(),
        "compose-dev_1"
    );
    assert!(
        serde_json::from_value::<super::RunnerWritableStorageMode>(serde_json::json!(
            "xfs_project_quota_slots"
        ))
        .is_err()
    );
    assert!(super::RunnerNamespace::try_new("../prod").is_err());
}

/// Verifies durable storage defaults point at local RustFS-compatible S3.
#[test]
fn storage_defaults_use_rustfs_s3() {
    let config = test_config();

    assert_eq!(config.storage_backend, super::StorageBackend::S3);
    assert_eq!(config.s3_bucket.as_deref(), Some(super::DEFAULT_S3_BUCKET));
    assert_eq!(config.s3_region, super::DEFAULT_S3_REGION);
    assert_eq!(
        config
            .s3_endpoint_url
            .as_ref()
            .map(url::Url::as_str)
            .map(|value| value.trim_end_matches('/')),
        Some(super::DEFAULT_S3_ENDPOINT_URL)
    );
    assert!(config.s3_force_path_style);
    assert!(config.s3_prefix.is_none());
    assert!(config.validate_object_storage_config().is_ok());
}

/// Verifies that default admin credentials are rejected on wildcard bind.
#[test]
fn rejects_default_admin_credentials_on_wildcard_bind() {
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

/// Verifies Moltbook defaults and name/URL consistency.
#[test]
fn validates_moltbook_community_config() {
    let mut config = test_config();
    assert_eq!(config.moltbook_submolt_name.as_str(), "agentics-platform");
    assert_eq!(
        config.moltbook_submolt_url.as_str(),
        "https://www.moltbook.com/m/agentics-platform"
    );
    assert!(config.validate_api_security().is_ok());

    config.moltbook_submolt_url = "https://www.moltbook.com/m/other-platform"
        .parse()
        .expect("valid Moltbook Submolt URL");
    let error = config
        .validate_api_security()
        .expect_err("mismatched Moltbook Submolt config should fail startup validation");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_MOLTBOOK_SUBMOLT_NAME must match")
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
                runner_max_runs: 101,
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

/// Verifies durable storage configuration fails closed for S3 and object limits.
#[test]
fn object_storage_config_requires_backend_specific_settings() {
    for (config, expected) in [
        (
            Config {
                storage_backend: super::StorageBackend::S3,
                s3_bucket: None,
                ..test_config()
            },
            "AGENTICS_S3_BUCKET",
        ),
        (
            Config {
                storage_max_bundle_archive_bytes: 0,
                ..test_config()
            },
            "AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES",
        ),
        (
            Config {
                storage_work_root: Some("relative-work".to_string()),
                ..test_config()
            },
            "AGENTICS_STORAGE_WORK_ROOT",
        ),
        (
            Config {
                storage_tmp_object_grace_hours: 0,
                ..test_config()
            },
            "AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS",
        ),
        (
            Config {
                storage_backend: super::StorageBackend::S3,
                s3_bucket: Some("agentics-test".to_string()),
                s3_prefix: Some("../bad".to_string()),
                ..test_config()
            },
            "AGENTICS_S3_PREFIX",
        ),
        (
            Config {
                storage_backend: super::StorageBackend::S3,
                s3_endpoint_url: Some("ftp://127.0.0.1".parse().expect("valid URL")),
                ..test_config()
            },
            "AGENTICS_S3_ENDPOINT_URL",
        ),
    ] {
        let error = config
            .validate_object_storage_config()
            .expect_err("invalid storage config should be rejected");
        assert!(error.to_string().contains(expected));
    }

    let config = Config {
        storage_backend: super::StorageBackend::S3,
        s3_bucket: Some("agentics-test".to_string()),
        s3_prefix: Some("agentics/dev".to_string()),
        s3_endpoint_url: Some("http://127.0.0.1:9000".parse().expect("valid S3 URL")),
        s3_force_path_style: true,
        ..test_config()
    };
    assert!(config.validate_object_storage_config().is_ok());

    let local_config = Config {
        storage_backend: super::StorageBackend::Local,
        s3_bucket: None,
        s3_endpoint_url: None,
        s3_force_path_style: false,
        ..test_config()
    };
    assert!(local_config.validate_object_storage_config().is_ok());
}

/// Verifies that hosted workers must bound bind mounts and writable rootfs.
#[test]
fn production_runner_requires_bounded_mounts_layers_and_host_probes() {
    let mut config = test_config();
    config.runner_security_profile = super::RunnerSecurityProfile::Production;
    config.require_digest_pinned_images = true;
    let runtime_root = tempfile::tempdir().expect("runtime root tempdir");
    let phase_root = tempfile::tempdir().expect("phase root tempdir");
    #[cfg(unix)]
    {
        std::fs::set_permissions(runtime_root.path(), std::fs::Permissions::from_mode(0o700))
            .expect("runtime root permissions");
        std::fs::set_permissions(phase_root.path(), std::fs::Permissions::from_mode(0o700))
            .expect("phase root permissions");
    }

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
    config.runner_runtime_root = Some(runtime_root.path().display().to_string());
    config.runner_phase_mount_root = Some(phase_root.path().display().to_string());
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

/// Verifies production runners reject traversable runtime roots.
#[test]
#[cfg(unix)]
fn production_runner_rejects_world_traversable_runtime_root() {
    let runtime_root = tempfile::tempdir().expect("runtime root tempdir");
    let phase_root = tempfile::tempdir().expect("phase root tempdir");
    std::fs::set_permissions(runtime_root.path(), std::fs::Permissions::from_mode(0o755))
        .expect("runtime root permissions");
    std::fs::set_permissions(phase_root.path(), std::fs::Permissions::from_mode(0o700))
        .expect("phase root permissions");

    let config = Config {
        runner_security_profile: super::RunnerSecurityProfile::Production,
        require_digest_pinned_images: true,
        runner_writable_storage_mode: super::RunnerWritableStorageMode::XfsProjectQuotaSlots,
        runner_docker_layer_quota: true,
        host_probe_mode: super::HostProbeMode::Require,
        runner_runtime_root: Some(runtime_root.path().display().to_string()),
        runner_phase_mount_root: Some(phase_root.path().display().to_string()),
        ..test_config()
    };

    let error = config
        .validate_runner_storage()
        .expect_err("production runtime root must not be traversable");
    assert!(error.to_string().contains("mode 0700"));
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
    use agentics_domain::models::challenge::TargetAccelerator;

    assert!(super::WorkerAccelerators::None.supports(TargetAccelerator::None));
    assert!(!super::WorkerAccelerators::None.supports(TargetAccelerator::Gpu));
    assert!(super::WorkerAccelerators::Gpu.supports(TargetAccelerator::None));
    assert!(super::WorkerAccelerators::Gpu.supports(TargetAccelerator::Gpu));
    assert_eq!(
        super::WorkerAccelerators::Gpu.heartbeat_values(),
        vec!["none".to_string(), "gpu".to_string()]
    );
}

/// Handles test config for this module.
fn test_config() -> Config {
    Config {
        database_url: SecretString::from(""),
        challenges_root: String::new(),
        ..Default::default()
    }
}
