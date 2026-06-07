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
use agentics_domain::models::urls::GithubAppRedirectUrl;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
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
    config.database.url = SecretString::from("postgres://agentics:secret@localhost/agentics");
    config.github_app.client_secret = Some(SecretString::from("secret-github-app-client"));

    let debug = format!("{config:?}");

    assert!(!debug.contains("secret@localhost"));
    assert!(!debug.contains("secret-github-app-client"));
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
            "AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS".to_string(),
            "123,456".to_string(),
        ),
        (
            "AGENTICS_CHALLENGES_ROOT".to_string(),
            "/tmp/agentics-challenges".to_string(),
        ),
        (
            "AGENTICS_MAX_ACTIVE_CHALLENGE_REVIEW_RECORDS_PER_HUMAN".to_string(),
            "7".to_string(),
        ),
        ("AGENTICS_POSTGRES_PORT".to_string(), "6543".to_string()),
    ])
    .expect("raw env should deserialize");

    let config = Config::try_from(raw).expect("raw env should convert");

    assert_eq!(config.api_web.api_port, 3222);
    assert!(
        config
            .database
            .url
            .expose_secret()
            .contains(":6543/agentics")
    );
    assert_eq!(config.storage.challenges_root, "/tmp/agentics-challenges");
    assert_eq!(
        config.auth.bootstrap_admin_github_user_ids,
        vec![
            agentics_domain::models::auth::GithubUserId::try_new(123)
                .expect("valid test GitHub user id"),
            agentics_domain::models::auth::GithubUserId::try_new(456)
                .expect("valid test GitHub user id"),
        ]
    );
    assert_eq!(
        config.quotas.max_active_challenge_review_records_per_human,
        7
    );
}

/// Verifies stage-aware env policy parses every supported launch stage.
#[test]
fn env_policy_parses_supported_deployment_stages() {
    for (raw, expected) in [
        ("dev", super::DeploymentStage::Dev),
        ("test", super::DeploymentStage::Test),
        ("rehearsal", super::DeploymentStage::Rehearsal),
        ("production", super::DeploymentStage::Production),
    ] {
        let parsed = raw
            .parse::<super::DeploymentStage>()
            .expect("stage should parse");
        assert_eq!(parsed, expected);
    }

    assert!("staging".parse::<super::DeploymentStage>().is_err());
}

/// Verifies missing required production env values fail before service startup.
#[test]
fn env_policy_rejects_missing_required_production_values() {
    let mut env = HashMap::new();
    env.insert(
        "AGENTICS_DEPLOYMENT_STAGE".to_string(),
        "production".to_string(),
    );

    let error = super::validate_env_policy(&env, super::EnvServiceRole::Compose)
        .expect_err("missing production env should fail");
    assert!(error.to_string().contains("AGENTICS_POSTGRES_USER"));
}

/// Verifies optional values report defaults without failing startup.
#[test]
fn env_policy_reports_optional_defaults() {
    let env = minimal_dev_env();

    let report = super::validate_env_policy(&env, super::EnvServiceRole::LocalDev)
        .expect("minimal dev env should pass");

    assert!(report.warnings.iter().any(|warning| warning.name
        == "NEXT_PUBLIC_AGENTICS_GA_MEASUREMENT_ID"
        && warning.message.contains("analytics disabled")));
}

/// Verifies old env names are rejected instead of silently ignored.
#[test]
fn env_policy_rejects_removed_env_names() {
    let mut env = minimal_dev_env();
    env.insert(
        super::ENV_STALE_REVIEW_RECORD_LIMIT.to_string(),
        "3".to_string(),
    );
    env.insert(
        super::ENV_AGENTICS_REHEARSAL_ENVIRONMENT.to_string(),
        "true".to_string(),
    );

    let error = super::validate_env_policy(&env, super::EnvServiceRole::LocalDev)
        .expect_err("removed env names should fail");
    let message = error.to_string();
    assert!(message.contains(super::ENV_STALE_REVIEW_RECORD_LIMIT));
    assert!(message.contains(super::ENV_AGENTICS_REHEARSAL_ENVIRONMENT));
}

/// Verifies removed-but-ignored env values are surfaced as warnings.
#[test]
fn env_policy_warns_for_ignored_env_names() {
    let mut env = minimal_dev_env();
    env.insert(
        super::ENV_AGENTICS_WEB_HOST.to_string(),
        "0.0.0.0".to_string(),
    );
    env.insert(super::ENV_RUST_LOG.to_string(), "debug".to_string());

    let report = super::validate_env_policy(&env, super::EnvServiceRole::LocalDev)
        .expect("ignored env names should not fail");

    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.name == super::ENV_AGENTICS_WEB_HOST)
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.name == super::ENV_RUST_LOG)
    );
}

/// Verifies hosted stage placeholders fail before Compose starts.
#[test]
fn env_policy_rejects_hosted_placeholders() {
    let mut env = full_production_env();
    env.insert(
        "AGENTICS_POSTGRES_PASSWORD".to_string(),
        "replace-with-postgres-password".to_string(),
    );

    let error = super::validate_env_policy(&env, super::EnvServiceRole::Compose)
        .expect_err("placeholder values should fail");

    assert!(error.to_string().contains("AGENTICS_POSTGRES_PASSWORD"));
}

/// Verifies every stage env example name is covered by env policy.
#[test]
fn stage_env_examples_are_covered_by_policy() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("config crate lives under crates/config");
    let known = super::known_stage_env_names();
    let mut missing = Vec::new();

    for relative in [
        "deploy/compose/env/dev.env.example",
        "deploy/compose/env/test.env.example",
        "deploy/compose/env/rehearsal.env.example",
        "deploy/compose/env/prod.env.example",
    ] {
        let content = std::fs::read_to_string(repo_root.join(relative))
            .expect("stage env example should be readable");
        for name in env_names_from_example(&content) {
            if !known.contains(name.as_str()) {
                missing.push(format!("{relative}:{name}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "stage env vars missing policy entries: {}",
        missing.join(", ")
    );
}

/// Verifies GitHub App sign-in env values map to the grouped config.
#[test]
fn raw_app_env_deserializes_github_app_sign_in_values() {
    let raw = super::RawAppEnv::from_env_iter([
        (
            "AGENTICS_GITHUB_APP_CLIENT_ID".to_string(),
            "app-client-id".to_string(),
        ),
        (
            "AGENTICS_GITHUB_APP_CLIENT_SECRET".to_string(),
            "app-client-secret".to_string(),
        ),
        (
            "AGENTICS_GITHUB_APP_REDIRECT_URL".to_string(),
            "http://127.0.0.1:3001/auth/github/callback".to_string(),
        ),
    ])
    .expect("GitHub App env should deserialize");
    let config = Config::try_from(raw).expect("GitHub App env should convert");

    assert_eq!(
        config.github_app.client_id.as_deref(),
        Some("app-client-id")
    );
    assert_eq!(
        config
            .github_app
            .client_secret
            .as_ref()
            .map(ExposeSecret::expose_secret),
        Some("app-client-secret")
    );
    assert_eq!(
        config
            .github_app
            .redirect_url
            .as_ref()
            .map(|url| url.as_str()),
        Some("http://127.0.0.1:3001/auth/github/callback")
    );
}

/// Verifies partial GitHub App sign-in env fails hosted API validation.
#[test]
fn partial_github_app_sign_in_config_fails_validation() {
    let mut config = test_config();
    config.github_app.client_id = Some("only-client-id".to_string());

    let error = config
        .validate_api_security()
        .expect_err("partial GitHub App config should fail");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_GITHUB_APP_CLIENT_SECRET must be set")
    );
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
    assert!(!config.storage.s3_force_path_style);

    let error = super::RawAppEnv::from_env_iter([(
        "AGENTICS_S3_FORCE_PATH_STYLE".to_string(),
        "1".to_string(),
    )])
    .expect_err("legacy bool-ish alias should fail during raw env parsing");
    assert!(error.to_string().contains("S3_FORCE_PATH_STYLE"));
}

/// Verifies hosted-probe env values fail closed when blank.
#[test]
fn blank_probe_env_values_are_rejected() {
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
        serde_json::from_value::<super::OfficialLogRedactionMode>(serde_json::json!(
            "contract_based"
        ))
        .unwrap(),
        super::OfficialLogRedactionMode::ContractBased
    );
    assert_eq!(
        serde_json::from_value::<super::OfficialLogRedactionMode>(serde_json::json!("always"))
            .unwrap()
            .as_str(),
        "always"
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
    assert!(
        serde_json::from_value::<super::OfficialLogRedactionMode>(serde_json::json!("private"))
            .is_err()
    );
    assert!(super::RunnerNamespace::try_new("../prod").is_err());
}

/// Verifies official log redaction defaults to contract-based diagnostics and accepts env override.
#[test]
fn official_log_redaction_env_defaults_and_overrides() {
    let default_config = test_config();
    assert_eq!(
        default_config.runner.official_log_redaction,
        super::OfficialLogRedactionMode::ContractBased
    );

    let raw = super::RawAppEnv::from_env_iter([(
        "AGENTICS_OFFICIAL_LOG_REDACTION".to_string(),
        "always".to_string(),
    )])
    .expect("official log redaction env should deserialize");
    let config = Config::try_from(raw).expect("raw env should convert");

    assert_eq!(
        config.runner.official_log_redaction,
        super::OfficialLogRedactionMode::Always
    );
}

/// Verifies durable storage defaults point at local RustFS-compatible S3.
#[test]
fn storage_defaults_use_rustfs_s3() {
    let config = test_config();

    assert_eq!(config.storage.backend, super::StorageBackend::S3);
    assert_eq!(
        config.storage.s3_bucket.as_deref(),
        Some(super::DEFAULT_S3_BUCKET)
    );
    assert_eq!(config.storage.s3_region, super::DEFAULT_S3_REGION);
    assert_eq!(
        config
            .storage
            .s3_endpoint_url
            .as_ref()
            .map(url::Url::as_str)
            .map(|value| value.trim_end_matches('/')),
        Some(super::DEFAULT_S3_ENDPOINT_URL)
    );
    assert!(config.storage.s3_force_path_style);
    assert!(config.storage.s3_prefix.is_none());
    assert!(config.validate_object_storage_config().is_ok());
}

/// Verifies that hosted browser sign-in requires secure cookies and invited registration.
#[test]
fn hosted_browser_sign_in_requires_secure_cookies_and_invited_registration() {
    let mut config = test_config();
    config.api_web.api_host = "0.0.0.0".to_string();

    assert!(config.validate_api_security().is_err());

    config.auth.agent_registration_mode = super::AgentRegistrationMode::PioneerCode;
    configure_test_github_sign_in(&mut config);
    let error = config
        .validate_api_security()
        .expect_err("hosted browser callback requires secure cookies");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_WEB_SESSION_COOKIE_SECURE=false")
    );

    config.api_web.web_session_cookie_secure = true;
    assert!(config.validate_api_security().is_ok());

    config.auth.agent_registration_mode = super::AgentRegistrationMode::Public;
    assert!(config.validate_api_security().is_err());
}

/// Verifies containerized local development may bind broadly while the browser callback is loopback.
#[test]
fn loopback_github_callback_allows_insecure_dev_cookies() {
    let mut config = test_config();
    config.api_web.api_host = "0.0.0.0".to_string();
    config.auth.agent_registration_mode = super::AgentRegistrationMode::PioneerCode;
    configure_test_github_sign_in(&mut config);
    config.github_app.redirect_url = Some(
        GithubAppRedirectUrl::try_new("http://127.0.0.1:3001/auth/github/callback")
            .expect("loopback HTTP redirect URL should parse"),
    );

    assert!(config.validate_api_security().is_ok());
}

/// Verifies bootstrap admin IDs cannot be configured without GitHub sign-in.
#[test]
fn bootstrap_admin_requires_github_sign_in_config() {
    let mut config = test_config();
    config.auth.bootstrap_admin_github_user_ids = vec![
        agentics_domain::models::auth::GithubUserId::try_new(9001)
            .expect("valid test GitHub user id"),
    ];

    let error = config
        .validate_api_security()
        .expect_err("bootstrap admin requires GitHub sign-in");
    assert!(
        error
            .to_string()
            .contains("GitHub sign-in must be fully configured")
    );

    configure_test_github_sign_in(&mut config);
    assert!(config.validate_api_security().is_ok());
}

/// Verifies that hosted API binds reject public registration mode.
#[test]
fn hosted_bind_rejects_public_agent_registration_mode() {
    let mut config = test_config();
    config.api_web.api_host = "0.0.0.0".to_string();
    config.api_web.web_session_cookie_secure = true;
    config.auth.agent_registration_mode = super::AgentRegistrationMode::Public;

    let error = config
        .validate_api_security()
        .expect_err("public mode must stay local-only");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_AGENT_REGISTRATION_MODE=public")
    );
}

/// Verifies GitHub sign-in redirects may use HTTP only on loopback hosts.
#[test]
fn github_app_redirect_http_is_loopback_only() {
    let mut loopback = test_config();
    configure_test_github_sign_in(&mut loopback);
    loopback.github_app.redirect_url = Some(
        GithubAppRedirectUrl::try_new("http://127.0.0.1:3001/auth/github/callback")
            .expect("loopback HTTP redirect URL should parse"),
    );
    assert!(loopback.validate_api_security().is_ok());

    let mut non_loopback = test_config();
    configure_test_github_sign_in(&mut non_loopback);
    non_loopback.github_app.redirect_url = Some(
        GithubAppRedirectUrl::try_new("http://agentics.example/auth/github/callback")
            .expect("non-loopback HTTP redirect URL should parse before config policy"),
    );
    let error = non_loopback
        .validate_api_security()
        .expect_err("non-loopback HTTP redirect should fail config validation");
    assert!(error.to_string().contains("must use HTTPS"));
}

/// Verifies invalid configured CORS origins fail startup validation.
#[test]
fn invalid_cors_origin_is_rejected() {
    let mut config = test_config();
    config.api_web.cors_allowed_origins = "http://localhost:3001,http://bad\nsite".to_string();

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
    assert_eq!(config.moltbook.submolt_name.as_str(), "agentics-platform");
    assert_eq!(
        config.moltbook.submolt_url.as_str(),
        "https://www.moltbook.com/m/agentics-platform"
    );
    assert!(config.validate_api_security().is_ok());

    config.moltbook.submolt_url = "https://www.moltbook.com/m/other-platform"
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
    let config = config_with_runner(|runner| {
        runner.writable_slot_classes_mb = "1024,64 256,1024".to_string();
    });

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
            config_with_runner(|runner| runner.max_output_files = 0),
            "AGENTICS_RUNNER_MAX_OUTPUT_FILES",
        ),
        (
            config_with_runner(|runner| runner.max_output_dirs = 0),
            "AGENTICS_RUNNER_MAX_OUTPUT_DIRS",
        ),
        (
            config_with_runner(|runner| runner.max_output_depth = 0),
            "AGENTICS_RUNNER_MAX_OUTPUT_DEPTH",
        ),
        (
            config_with_runner(|runner| runner.max_runs = 0),
            "AGENTICS_RUNNER_MAX_RUNS",
        ),
        (
            config_with_runner(|runner| runner.max_runs = 101),
            "AGENTICS_RUNNER_MAX_RUNS",
        ),
        (
            config_with_runner(|runner| runner.max_result_json_bytes = 0),
            "AGENTICS_RUNNER_MAX_RESULT_JSON_BYTES",
        ),
        (
            config_with_runner(|runner| runner.max_public_results = 0),
            "AGENTICS_RUNNER_MAX_PUBLIC_RESULTS",
        ),
        (
            config_with_runner(|runner| runner.max_result_log_bytes = 0),
            "AGENTICS_RUNNER_MAX_RESULT_LOG_BYTES",
        ),
        (
            config_with_runner(|runner| runner.max_interaction_bytes_per_direction = 0),
            "AGENTICS_RUNNER_MAX_INTERACTION_BYTES_PER_DIRECTION",
        ),
        (
            config_with_runner(|runner| runner.interaction_shutdown_grace_secs = 0),
            "AGENTICS_RUNNER_INTERACTION_SHUTDOWN_GRACE_SECS",
        ),
    ] {
        config.api_web.api_host = "127.0.0.1".to_string();
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
            config_with_storage(|storage| {
                storage.backend = super::StorageBackend::S3;
                storage.s3_bucket = None;
            }),
            "AGENTICS_S3_BUCKET",
        ),
        (
            config_with_storage(|storage| storage.max_bundle_archive_bytes = 0),
            "AGENTICS_STORAGE_MAX_BUNDLE_ARCHIVE_BYTES",
        ),
        (
            config_with_storage(|storage| storage.work_root = Some("relative-work".to_string())),
            "AGENTICS_STORAGE_WORK_ROOT",
        ),
        (
            config_with_storage(|storage| storage.tmp_object_grace_hours = 0),
            "AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS",
        ),
        (
            config_with_storage(|storage| {
                storage.backend = super::StorageBackend::S3;
                storage.s3_bucket = Some("agentics-test".to_string());
                storage.s3_prefix = Some("../bad".to_string());
            }),
            "AGENTICS_S3_PREFIX",
        ),
        (
            config_with_storage(|storage| {
                storage.backend = super::StorageBackend::S3;
                storage.s3_endpoint_url = Some("ftp://127.0.0.1".parse().expect("valid URL"));
            }),
            "AGENTICS_S3_ENDPOINT_URL",
        ),
    ] {
        let error = config
            .validate_object_storage_config()
            .expect_err("invalid storage config should be rejected");
        assert!(error.to_string().contains(expected));
    }

    let config = config_with_storage(|storage| {
        storage.backend = super::StorageBackend::S3;
        storage.s3_bucket = Some("agentics-test".to_string());
        storage.s3_prefix = Some("agentics/dev".to_string());
        storage.s3_endpoint_url = Some("http://127.0.0.1:9000".parse().expect("valid S3 URL"));
        storage.s3_force_path_style = true;
    });
    assert!(config.validate_object_storage_config().is_ok());

    let local_config = config_with_storage(|storage| {
        storage.backend = super::StorageBackend::Local;
        storage.s3_bucket = None;
        storage.s3_region.clear();
        storage.s3_prefix = Some("../ignored-for-local".to_string());
        storage.s3_endpoint_url = None;
        storage.s3_force_path_style = false;
    });
    assert!(local_config.validate_object_storage_config().is_ok());
}

/// Verifies that hosted workers must bound bind mounts and writable rootfs.
#[test]
fn production_runner_requires_bounded_mounts_layers_and_host_probes() {
    let mut config = test_config();
    config.runner.security_profile = super::RunnerSecurityProfile::Production;
    config.runner.require_digest_pinned_images = true;
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

    config.runner.docker_layer_quota = true;
    assert!(
        config.validate_runner_storage().is_err(),
        "Docker layer quota does not bound phase bind mounts"
    );

    config.runner.writable_storage_mode = super::RunnerWritableStorageMode::XfsProjectQuotaSlots;
    config.runner.docker_layer_quota = false;
    config.api_web.api_host = "127.0.0.1".to_string();
    config.runner.runtime_root = Some(runtime_root.path().display().to_string());
    config.runner.phase_mount_root = Some(phase_root.path().display().to_string());
    let error = config
        .validate_runner_storage()
        .expect_err("quota-backed writable rootfs also needs Docker layer quota");
    assert!(error.to_string().contains("xfs-project-quota-slots"));

    config.runner.docker_layer_quota = true;
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

    config.runner.host_probe_mode = super::HostProbeMode::Require;
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

    let config = config_with_runner(|runner| {
        runner.security_profile = super::RunnerSecurityProfile::Production;
        runner.require_digest_pinned_images = true;
        runner.writable_storage_mode = super::RunnerWritableStorageMode::XfsProjectQuotaSlots;
        runner.docker_layer_quota = true;
        runner.host_probe_mode = super::HostProbeMode::Require;
        runner.runtime_root = Some(runtime_root.path().display().to_string());
        runner.phase_mount_root = Some(phase_root.path().display().to_string());
    });

    let error = config
        .validate_runner_storage()
        .expect_err("production runtime root must not be traversable");
    assert!(error.to_string().contains("mode 0700"));
}

/// Verifies quota-backed runner storage requires a host-visible runtime root.
#[test]
fn quota_backed_runner_requires_runtime_root() {
    let config = config_with_runner(|runner| {
        runner.writable_storage_mode = super::RunnerWritableStorageMode::XfsProjectQuotaSlots;
        runner.docker_layer_quota = true;
        runner.phase_mount_root = Some("/agentics-runner-slots".to_string());
    });
    let error = config
        .validate_runner_storage()
        .expect_err("quota-backed storage must require a runtime root");
    if cfg!(target_os = "linux") {
        assert!(error.to_string().contains("AGENTICS_RUNNER_RUNTIME_ROOT"));
    } else {
        assert!(error.to_string().contains("Linux-only"));
    }

    let config = config_with_runner(|runner| {
        runner.writable_storage_mode = super::RunnerWritableStorageMode::XfsProjectQuotaSlots;
        runner.docker_layer_quota = true;
        runner.runtime_root = Some("relative-runtime".to_string());
        runner.phase_mount_root = Some("/agentics-runner-slots".to_string());
    });
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
    let mut probe_config = config_with_runner(|runner| {
        runner.host_probe_mode = super::HostProbeMode::Require;
    });
    let error = probe_config
        .validate_api_security()
        .expect_err("required hosted probes imply immutable images");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES")
    );
    assert!(probe_config.requires_digest_pinned_images());

    probe_config.runner.require_digest_pinned_images = true;
    assert!(probe_config.validate_api_security().is_ok());

    let production_config = config_with_runner(|runner| {
        runner.security_profile = super::RunnerSecurityProfile::Production;
    });
    let error = production_config
        .validate_api_security()
        .expect_err("production profile implies immutable images");
    assert!(
        error
            .to_string()
            .contains("AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES")
    );

    let local_quota_config = config_with_runner(|runner| {
        runner.writable_storage_mode = super::RunnerWritableStorageMode::XfsProjectQuotaSlots;
    });
    assert!(
        !local_quota_config.requires_digest_pinned_images(),
        "local quota-backed tests can still use local images when hosted probes are off"
    );
}

/// Verifies worker accelerator config is fail-closed for GPU workers.
#[test]
fn gpu_worker_requires_probe_image_and_linux_host() {
    let mut config = config_with_worker(|worker| {
        worker.accelerators = super::WorkerAccelerators::Gpu;
    });

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

    config.worker.gpu_probe_image = Some(
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

fn minimal_dev_env() -> HashMap<String, String> {
    env_map([
        ("AGENTICS_DEPLOYMENT_STAGE", "dev"),
        (
            "AGENTICS_DATABASE_URL",
            "postgres://agentics:agentics@postgres:5432/agentics_dev",
        ),
        ("AGENTICS_LOCAL_DEV_DATABASE_NAME", "agentics_dev"),
        (
            "AGENTICS_LOCAL_DEV_DATABASE_URL",
            "postgres://agentics:agentics@postgres:5432/agentics_dev",
        ),
        (
            "AGENTICS_LOCAL_DEV_DATABASE_URL_CONFIRM",
            "non-loopback-local-dev-db",
        ),
        (
            "AGENTICS_LOCAL_DEV_CHALLENGE_SOURCE_ROOT",
            "/workspace/Agentics/challenge-repos/agentics-challenges/dev/challenges",
        ),
        (
            "AGENTICS_LOCAL_DEV_TEST_SOLUTIONS_ROOT",
            "/workspace/Agentics/challenge-repos/agentics-challenges/dev/test-solutions",
        ),
        ("AGENTICS_STORAGE_BACKEND", "s3"),
        ("AGENTICS_S3_BUCKET", "agentics"),
        ("AGENTICS_S3_REGION", "us-east-1"),
        ("AGENTICS_S3_ENDPOINT_URL", "http://rustfs:9000"),
        ("AGENTICS_S3_FORCE_PATH_STYLE", "true"),
        ("AGENTICS_API_BASE_URL", "http://api:3100"),
    ])
}

fn full_production_env() -> HashMap<String, String> {
    env_map([
        ("AGENTICS_DEPLOYMENT_STAGE", "production"),
        ("AGENTICS_POSTGRES_USER", "agentics"),
        ("AGENTICS_POSTGRES_PASSWORD", "postgres-password"),
        ("AGENTICS_POSTGRES_DB", "agentics"),
        ("AGENTICS_RUSTFS_ACCESS_KEY", "rustfs-access"),
        ("AGENTICS_RUSTFS_SECRET_KEY", "rustfs-secret"),
        ("AGENTICS_STORAGE_BACKEND", "s3"),
        ("AGENTICS_S3_BUCKET", "agentics"),
        ("AGENTICS_S3_PREFIX", "prod"),
        ("AGENTICS_S3_REGION", "us-east-1"),
        ("AGENTICS_S3_ENDPOINT_URL", "http://rustfs:9000"),
        ("AGENTICS_S3_FORCE_PATH_STYLE", "true"),
        ("AGENTICS_STORAGE_WORK_ROOT", "/srv/agentics/storage-work"),
        ("AGENTICS_API_BASE_URL", "https://agentics.example"),
        ("AGENTICS_WEB_BASE_URL", "https://agentics.example"),
        ("AGENTICS_CORS_ALLOWED_ORIGINS", "https://agentics.example"),
        ("AGENTICS_BOOTSTRAP_ADMIN_GITHUB_USER_IDS", "39153080"),
        ("AGENTICS_WEB_SESSION_COOKIE_SECURE", "true"),
        ("AGENTICS_GITHUB_APP_CLIENT_ID", "client-id"),
        ("AGENTICS_GITHUB_APP_CLIENT_SECRET", "client-secret"),
        (
            "AGENTICS_GITHUB_APP_REDIRECT_URL",
            "https://agentics.example/auth/github/callback",
        ),
        (
            "AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT",
            "/srv/agentics/review-checkouts/agentics-challenges",
        ),
        (
            "AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT",
            "/srv/agentics/review-checkouts/agentics-challenges",
        ),
        ("AGENTICS_DOCKER_SOCKET_PATH", "/srv/agentics/docker.sock"),
        ("AGENTICS_DOCKER_HOST", "unix:///srv/agentics/docker.sock"),
        ("AGENTICS_RUNNER_NAMESPACE", "agentics-prod"),
        ("AGENTICS_RUNTIME_UID", "10001"),
        ("AGENTICS_RUNTIME_GID", "10001"),
        ("AGENTICS_DOCKER_SOCKET_GID", "10001"),
        ("AGENTICS_RUNNER_SECURITY_PROFILE", "production"),
        ("AGENTICS_HOST_PROBE_MODE", "require"),
        (
            "AGENTICS_HOST_PROBE_COMMAND",
            "/usr/local/bin/agentics-check-dgx-spark-profile",
        ),
        ("AGENTICS_REQUIRE_DIGEST_PINNED_IMAGES", "true"),
        (
            "AGENTICS_RUNNER_WRITABLE_STORAGE_MODE",
            "xfs-project-quota-slots",
        ),
        ("AGENTICS_RUNNER_RUNTIME_ROOT", "/srv/agentics/runtime"),
        (
            "AGENTICS_RUNNER_PHASE_MOUNT_ROOT",
            "/srv/agentics/phase-mounts",
        ),
        (
            "AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB",
            "64,256,1024,4096",
        ),
        ("AGENTICS_RUNNER_DOCKER_LAYER_QUOTA", "true"),
    ])
}

fn env_map<const N: usize>(entries: [(&str, &str); N]) -> HashMap<String, String> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn env_names_from_example(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(name, _value)| name.trim().to_string())
        .collect()
}

/// Handles test config for this module.
fn test_config() -> Config {
    let mut config = Config::default();
    config.database.url = SecretString::from("");
    config.storage.challenges_root = String::new();
    config
}

fn configure_test_github_sign_in(config: &mut Config) {
    config.github_app.client_id = Some("test-client-id".to_string());
    config.github_app.client_secret = Some(SecretString::from("test-client-secret"));
    config.github_app.redirect_url = Some(
        GithubAppRedirectUrl::try_new("https://agentics.example/auth/github/callback")
            .expect("test GitHub App redirect URL should parse"),
    );
}

fn config_with_runner(update: impl FnOnce(&mut super::RunnerConfig)) -> Config {
    let mut config = test_config();
    update(&mut config.runner);
    config
}

fn config_with_storage(update: impl FnOnce(&mut super::StorageConfig)) -> Config {
    let mut config = test_config();
    update(&mut config.storage);
    config
}

fn config_with_worker(update: impl FnOnce(&mut super::WorkerConfig)) -> Config {
    let mut config = test_config();
    update(&mut config.worker);
    config
}
