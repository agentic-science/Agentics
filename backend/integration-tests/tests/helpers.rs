#![allow(dead_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentics_config::{
    AgentRegistrationMode, Config, DEFAULT_S3_BUCKET, DEFAULT_S3_ENDPOINT_URL,
    DEFAULT_S3_FORCE_PATH_STYLE, DEFAULT_S3_REGION, ENV_AGENTICS_S3_BUCKET,
    ENV_AGENTICS_S3_ENDPOINT_URL, ENV_AGENTICS_S3_FORCE_PATH_STYLE, ENV_AGENTICS_S3_PREFIX,
    ENV_AGENTICS_S3_REGION, RunnerWritableStorageMode, StorageBackend,
};
use agentics_runner::connect_docker;
use agentics_storage::{
    S3Storage, StorageFactoryOptions, StorageKey, StorageWriteIntent, build_storage,
};
use api_server::router;
use api_server::state::AppState;
use chrono::{Duration, Utc};
use secrecy::SecretString;
use sqlx::PgPool;
use tokio::task::JoinHandle;

/// Resolve the canonical name for a published challenge fixture.
pub async fn published_challenge_name(pool: &PgPool, challenge_name: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT challenge_name FROM challenges WHERE challenge_name = $1 LIMIT 1",
    )
    .bind(challenge_name)
    .fetch_one(pool)
    .await
    .expect("published challenge name should exist")
}

/// Running test server bound to an ephemeral local port.
pub struct TestApp {
    pub addr: SocketAddr,
    pub _client: reqwest::Client,
    pub admin_service_token: String,
    server_task: JoinHandle<()>,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

/// Creator session material used by tests instead of a live GitHub sign-in round trip.
pub struct TestCreatorSession {
    pub human_id: String,
    pub cookie_header: String,
    pub csrf_token: String,
}

/// Spawn the API server with environment-derived config.
pub async fn spawn_app(pool: PgPool) -> TestApp {
    let mut config = Config::from_env().expect("failed to load config");
    config.auth.agent_registration_mode = AgentRegistrationMode::Public;
    spawn_app_with_config(pool, config).await
}

/// Spawn the API server with a caller-provided config.
///
/// Tests use this to point storage and seeded challenge roots at temporary
/// directories while exercising the real router and startup seeding path.
pub async fn spawn_app_with_config(pool: PgPool, config: Config) -> TestApp {
    ensure_test_storage_bucket(&config).await;
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("failed to initialize test storage");
    if std::fs::exists(&config.storage.challenges_root).expect("failed to inspect challenge root") {
        agentics_services::maintenance::ensure_challenges_seeded_from_root(
            &pool,
            &config,
            storage.as_ref(),
            &config.storage.challenges_root,
        )
        .await
        .expect("failed to seed challenges");
    }
    let admin_service_token = create_test_admin_service_token(&pool).await;

    let state = AppState {
        db: pool,
        config: Arc::new(config.clone()),
        storage,
    };

    let app = router::router(&config).with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test listener");
    let addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // Give the spawned server task a chance to bind before the first request.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();

    TestApp {
        addr,
        _client: client,
        admin_service_token,
        server_task,
    }
}

/// Build an isolated config for integration tests.
pub fn test_config(storage_root: &Path, challenges_root: &Path) -> Config {
    let mut config = Config::default();
    config.database.url =
        SecretString::from("postgres://agentics:agentics@127.0.0.1:5432/agentics_test");
    config.api_web.api_host = "127.0.0.1".to_string();
    config.api_web.api_port = 0;
    config.api_web.cors_allowed_origins = "http://127.0.0.1:3001,http://localhost:3001".to_string();
    config.api_web.web_session_cookie_name = "agentics_session".to_string();
    config.api_web.web_csrf_cookie_name = "agentics_csrf".to_string();
    config.api_web.web_session_ttl_hours = 24;
    config.api_web.web_session_cookie_secure = false;

    config.storage.root = storage_root.to_string_lossy().to_string();
    config.storage.backend = StorageBackend::S3;
    config.storage.work_root = Some(storage_root.join("_work").to_string_lossy().to_string());
    config.storage.s3_bucket = Some(env_or_default(ENV_AGENTICS_S3_BUCKET, DEFAULT_S3_BUCKET));
    config.storage.s3_prefix = Some(test_s3_prefix());
    config.storage.s3_region = env_or_default(ENV_AGENTICS_S3_REGION, DEFAULT_S3_REGION);
    config.storage.s3_endpoint_url = Some(test_s3_endpoint_url());
    config.storage.s3_force_path_style = test_s3_force_path_style();
    config.storage.max_bundle_archive_bytes = 1024 * 1024 * 1024;
    config.storage.max_statement_bytes = 1024 * 1024;
    config.storage.max_json_artifact_bytes = 1024 * 1024;
    config.storage.tmp_object_grace_hours = 24;
    config.storage.challenges_root = challenges_root.to_string_lossy().to_string();

    config.auth.agent_registration_mode = AgentRegistrationMode::Public;

    config.moltbook.submolt_name = "agentics-platform"
        .parse()
        .expect("valid test Moltbook Submolt name");
    config.moltbook.submolt_url = "https://www.moltbook.com/m/agentics-platform"
        .parse()
        .expect("valid test Moltbook Submolt URL");

    config.worker.poll_interval_ms = 3000;
    config.worker.stale_job_minutes = 1;
    config.worker.accelerators = agentics_config::WorkerAccelerators::None;
    config.worker.gpu_probe_image = None;

    config.quotas.validation_runs_per_agent_challenge_day = 20;
    config.quotas.official_runs_per_agent_challenge_day = 5;
    config.quotas.max_active_official_jobs = 20;
    config.quotas.max_active_agents = 1_000;
    config.quotas.max_active_challenge_review_records_per_human = 10;
    config
        .quotas
        .challenge_private_asset_bytes_per_review_record = 250 * 1024 * 1024;
    config.quotas.challenge_review_record_validations_per_day = 10;
    config
        .quotas
        .challenge_review_record_validation_timeout_minutes = 30;
    config
        .quotas
        .challenge_private_asset_pending_timeout_minutes = 30;
    config
        .quotas
        .challenge_review_record_publish_timeout_minutes = 30;
    config.quotas.challenge_review_record_ttl_days = 14;
    config.quotas.unpublished_challenge_asset_grace_days = 7;

    config.github_app.client_id = Some("test-client-id".to_string());
    config.github_app.client_secret = Some(SecretString::from("test-client-secret"));
    config.github_app.redirect_url = Some(
        "http://127.0.0.1/auth/github/callback"
            .parse()
            .expect("valid test GitHub sign-in redirect URL"),
    );
    config.github_app.authorize_url = "https://github.com/login/oauth/authorize"
        .parse()
        .expect("valid test GitHub sign-in authorize URL");
    config.github_app.token_url = "https://github.com/login/oauth/access_token"
        .parse()
        .expect("valid test GitHub sign-in token URL");
    config.github_app.api_user_url = "https://api.github.com/user"
        .parse()
        .expect("valid test GitHub API user URL");

    config.runner.docker_host = std::env::var("AGENTICS_TEST_DOCKER_HOST").ok();
    config.runner.host_probe_mode = agentics_config::HostProbeMode::Off;
    config.runner.host_probe_command = agentics_config::DEFAULT_HOST_PROBE_COMMAND.to_string();
    config.runner.security_profile = agentics_config::RunnerSecurityProfile::Development;
    config.runner.require_digest_pinned_images = false;
    config.runner.writable_storage_mode = RunnerWritableStorageMode::Unbounded;
    config.runner.namespace = test_runner_namespace();
    config.runner.runtime_root = std::env::var("AGENTICS_TEST_RUNNER_RUNTIME_ROOT").ok();
    config.runner.phase_mount_root = std::env::var("AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT").ok();
    config.runner.writable_slot_classes_mb =
        std::env::var("AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB")
            .unwrap_or_else(|_| "64,256,1024,4096".to_string());
    config.runner.docker_layer_quota = false;
    config.runner.max_output_files = 8192;
    config.runner.max_output_dirs = 1024;
    config.runner.max_output_depth = 32;
    config.runner.max_runs = 100;
    config.runner.max_result_json_bytes = 4 * 1024 * 1024;
    config.runner.max_public_results = 1024;
    config.runner.max_result_log_bytes = 256 * 1024;
    config.runner.max_interaction_bytes_per_direction = 256 * 1024 * 1024;
    config.runner.interaction_shutdown_grace_secs = 2;

    config.logging.log_level = "error".to_string();
    config
}

async fn ensure_test_storage_bucket(config: &Config) {
    if config.storage.backend != StorageBackend::S3 {
        return;
    }
    let storage = match config
        .storage_factory_options()
        .expect("valid S3 test storage options")
    {
        StorageFactoryOptions::S3(options) => S3Storage::from_options(options)
            .await
            .expect("failed to initialize S3 test storage for bucket setup"),
        StorageFactoryOptions::Local(_) => return,
    };
    storage
        .create_bucket_if_missing_for_tests()
        .await
        .expect("failed to create or inspect S3 test bucket");
}

/// Return whether a configured test-storage key exists.
pub async fn storage_key_exists(config: &Config, key: &str) -> bool {
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("failed to initialize test storage");
    let key = StorageKey::try_new(key).expect("test storage key should be valid");
    storage.exists(&key).await.expect("storage exists check")
}

/// Read bytes through the configured test-storage backend.
pub async fn read_storage_key(config: &Config, key: &str, intent: StorageWriteIntent) -> Vec<u8> {
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("failed to initialize test storage");
    let key = StorageKey::try_new(key).expect("test storage key should be valid");
    storage.get(&key, intent).await.expect("storage read")
}

/// Store test bytes through the configured test-storage backend.
pub async fn put_storage_key(config: &Config, key: &StorageKey, bytes: &[u8]) {
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("failed to initialize test storage");
    let max_bytes = u64::try_from(bytes.len()).expect("test bytes length fits u64");
    storage
        .put(
            key,
            bytes,
            StorageWriteIntent::new("test object", max_bytes),
        )
        .await
        .expect("storage put");
}

/// Return whether a configured durable-storage prefix has no objects.
pub async fn storage_prefix_is_empty(config: &Config, prefix: &str) -> bool {
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("failed to initialize test storage");
    let prefix = StorageKey::try_new(prefix).expect("test storage prefix should be valid");
    storage
        .list_prefix(&prefix)
        .await
        .expect("storage prefix list")
        .is_empty()
}

fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn test_s3_prefix() -> String {
    let base = std::env::var(ENV_AGENTICS_S3_PREFIX)
        .ok()
        .map(|value| value.trim_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "test/local".to_string());
    format!("{base}/{}", uuid::Uuid::new_v4())
}

fn test_s3_endpoint_url() -> url::Url {
    env_or_default(ENV_AGENTICS_S3_ENDPOINT_URL, DEFAULT_S3_ENDPOINT_URL)
        .parse()
        .expect("S3 test endpoint URL must be valid")
}

fn test_s3_force_path_style() -> bool {
    std::env::var(ENV_AGENTICS_S3_FORCE_PATH_STYLE)
        .ok()
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => true,
            "0" | "false" | "no" => false,
            other => {
                panic!("{ENV_AGENTICS_S3_FORCE_PATH_STYLE} must be true or false; got `{other}`")
            }
        })
        .unwrap_or(DEFAULT_S3_FORCE_PATH_STYLE)
}

fn test_runner_namespace() -> agentics_config::RunnerNamespace {
    let base = match std::env::var("AGENTICS_RUNNER_NAMESPACE") {
        Ok(value) => {
            agentics_config::RunnerNamespace::try_new(value.as_str())
                .expect("AGENTICS_RUNNER_NAMESPACE should be valid");
            value
        }
        Err(std::env::VarError::NotPresent) => "integration-tests".to_string(),
        Err(error) => panic!("AGENTICS_RUNNER_NAMESPACE should be valid UTF-8: {error}"),
    };
    // Parallel integration tests share one Docker daemon, so isolate runner
    // cleanup labels per test config while keeping the Compose project prefix.
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let suffix = &suffix[..8];
    let max_base_len = 63_usize
        .checked_sub(suffix.len() + 1)
        .expect("test namespace suffix should fit");
    let truncated_base = if base.len() > max_base_len {
        &base[..max_base_len]
    } else {
        &base
    };
    agentics_config::RunnerNamespace::try_new(format!("{truncated_base}-{suffix}"))
        .expect("AGENTICS_RUNNER_NAMESPACE should be valid")
}

/// Resolve the bundled integration challenge fixtures.
pub fn examples_challenges_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/challenges")
        .canonicalize()
        .expect("failed to resolve integration challenge fixtures root")
}

/// Resolve the public challenge repository submodule fixtures.
pub fn challenge_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../challenge-repos/agentics-challenges")
        .canonicalize()
        .expect("failed to resolve challenge repository root")
}

/// Build an absolute URL for the spawned test app.
pub fn api_url(app: &TestApp, path: &str) -> String {
    format!("http://{}{}", app.addr, path)
}

/// Build an admin service-token Authorization header for the spawned app.
pub fn admin_service_token_header(app: &TestApp) -> String {
    format!("Bearer {}", app.admin_service_token)
}

async fn create_test_admin_service_token(pool: &PgPool) -> String {
    let repos = agentics_persistence::Repositories::new(pool);
    let human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: agentics_domain::models::ids::HumanId::generate(),
            github_user_id: 9_001,
            github_login: "integration-admin".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: false,
            bootstrap_admin_candidate: true,
        })
        .await
        .expect("test admin human should resolve");
    let token = agentics_services::auth::create_admin_service_token();
    repos
        .sessions()
        .create_admin_service_token(&agentics_persistence::CreateAdminServiceTokenInput {
            id: agentics_domain::models::ids::AdminServiceTokenId::generate(),
            token_hash: agentics_services::auth::hash_opaque_token(&token),
            label: "integration-tests".to_string(),
            created_by_human_id: human.human_id,
            expires_at: None,
        })
        .await
        .expect("test admin service token should insert");
    token
}

/// Insert a GitHub sign-in-equivalent human creator session for integration tests.
pub async fn create_creator_session(
    pool: &PgPool,
    github_user_id: i64,
    github_login: &str,
) -> TestCreatorSession {
    let repos = agentics_persistence::Repositories::new(pool);
    let human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: agentics_domain::models::ids::HumanId::generate(),
            github_user_id,
            github_login: github_login.to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: false,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("creator human should resolve");
    let session_token = agentics_services::auth::create_web_session_token();
    let csrf_token = agentics_services::auth::create_csrf_token();
    repos
        .sessions()
        .create_human_session(&agentics_persistence::CreateHumanSessionInput {
            session_id: agentics_domain::models::ids::HumanSessionId::generate(),
            session_token_hash: agentics_services::auth::hash_opaque_token(&session_token),
            csrf_token_hash: agentics_services::auth::hash_opaque_token(&csrf_token),
            human_id: human.human_id.clone(),
            expires_at: Utc::now()
                .checked_add_signed(Duration::hours(24))
                .expect("test creator session TTL should not overflow"),
        })
        .await
        .expect("creator session should insert");

    TestCreatorSession {
        human_id: human.human_id.as_str().to_string(),
        cookie_header: format!("agentics_session={session_token}"),
        csrf_token,
    }
}

/// Create a base64 ZIP containing a manifest-based `zip_project` solution.
pub fn solution_zip_base64(main_py: &str) -> String {
    solution_zip_base64_with_scripts(
        main_py,
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        "#!/usr/bin/env sh\nset -eu\nmkdir -p build\nprintf built > build/generated.txt\n",
        "#!/usr/bin/env sh\nset -eu\ntest -f build/generated.txt\npython main.py\n",
    )
}

/// Create a base64 sample-sum ZIP with caller-provided phase scripts.
pub fn solution_zip_base64_with_scripts(
    main_py: &str,
    setup_sh: &str,
    build_sh: &str,
    run_sh: &str,
) -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "sample-sum smoke solution",
                "commands": {
                    "setup": "scripts/setup.sh",
                    "build": "scripts/build.sh",
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        ("scripts/setup.sh", setup_sh.to_string()),
        ("scripts/build.sh", build_sh.to_string()),
        ("run.sh", run_sh.to_string()),
        ("main.py", main_py.to_string()),
    ])
}

/// Create a base64 ZIP containing a file-mode grid-routing solution.
pub fn grid_routing_solution_zip_base64(paths_by_instance: &[(&str, &str)]) -> String {
    let path_entries = paths_by_instance
        .iter()
        .map(|(instance_id, path)| format!("    {instance_id:?}: {path:?},"))
        .collect::<Vec<_>>()
        .join("\n");
    let main_py = [
        "from __future__ import annotations",
        "",
        "import json",
        "import os",
        "from pathlib import Path",
        "",
        "PATHS = {",
        &path_entries,
        "}",
        "",
        "def main() -> None:",
        "    input_dir = Path(os.environ['AGENTICS_INPUT_DIR'])",
        "    output_dir = Path(os.environ['AGENTICS_OUTPUT_DIR'])",
        "    payload = json.loads((input_dir / 'case.json').read_text())",
        "    output_dir.mkdir(parents=True, exist_ok=True)",
        "    (output_dir / 'path.txt').write_text(PATHS[payload['instance_id']] + '\\n')",
        "",
        "if __name__ == '__main__':",
        "    main()",
        "",
    ]
    .join("\n");

    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "grid-routing file-mode solution",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\npython main.py\n".to_string(),
        ),
        ("main.py", main_py),
    ])
}

/// Create a base64 ZIP from explicit archive entries.
pub fn zip_project_zip_base64(entries: Vec<(&str, String)>) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let cursor = std::io::Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for (name, content) in entries {
        archive
            .start_file(name, options)
            .unwrap_or_else(|_| panic!("failed to start {name} in zip"));
        archive
            .write_all(content.as_bytes())
            .unwrap_or_else(|_| panic!("failed to write {name} to zip"));
    }

    let cursor = archive.finish().expect("failed to finish zip");
    STANDARD.encode(cursor.into_inner())
}

/// Generate a tiny Python solution for the `sample-sum` fixture.
pub fn sample_sum_solution(expression: &str) -> String {
    [
        "from __future__ import annotations",
        "",
        "import json",
        "import sys",
        "",
        "",
        "def main() -> None:",
        "    payload = json.loads(sys.stdin.read())",
        &format!("    print({expression})"),
        "",
        "",
        "if __name__ == '__main__':",
        "    main()",
        "",
    ]
    .join("\n")
}

/// Execute one production worker cycle against the integration-test database.
pub async fn run_worker_once(pool: &PgPool, config: &Config) {
    let docker = connect_docker(config).expect("failed to connect to Docker");
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("failed to initialize worker storage");

    worker::cycle::run_worker_cycle(
        pool,
        &docker,
        config,
        storage.as_ref(),
        "integration-test-worker",
    )
    .await
    .expect("worker cycle failed");
}

/// Recursively copy a fixture directory into a temporary test location.
pub fn copy_dir_all(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("failed to create destination directory");

    for entry in std::fs::read_dir(source).expect("failed to read source directory") {
        let entry = entry.expect("failed to read directory entry");
        let file_type = entry.file_type().expect("failed to read entry file type");
        let target = destination.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("failed to copy file");
        }
    }
}
