#![allow(dead_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use api_server::router;
use api_server::state::AppState;
use shared::config::Config;
use shared::runner::{connect_docker, pre_pull_image};
use shared::storage::{LocalStorage, Storage};
use sqlx::PgPool;

/// Running test server bound to an ephemeral local port.
pub struct TestApp {
    pub addr: SocketAddr,
    pub _client: reqwest::Client,
}

/// Spawn the API server with environment-derived config.
pub async fn spawn_app(pool: PgPool) -> TestApp {
    let config = Config::from_env().expect("failed to load config");
    spawn_app_with_config(pool, config).await
}

/// Spawn the API server with a caller-provided config.
///
/// Tests use this to point storage and seeded problem roots at temporary
/// directories while exercising the real router and startup seeding path.
pub async fn spawn_app_with_config(pool: PgPool, config: Config) -> TestApp {
    if Path::new(&config.problems_root).exists() {
        shared::db::queries::ensure_problems_seeded_from_root(&pool, &config.problems_root)
            .await
            .expect("failed to seed problems");
    }

    let storage = Arc::new(LocalStorage::new(&config.storage_root));

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        storage,
    };

    let app = router::router().with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test listener");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the spawned server task a chance to bind before the first request.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();

    TestApp {
        addr,
        _client: client,
    }
}

/// Build an isolated config for integration tests.
pub fn test_config(storage_root: &Path, problems_root: &Path) -> Config {
    Config {
        database_url: "postgres://llm_oj:llm_oj@127.0.0.1:5432/llm_oj_test".to_string(),
        api_host: "127.0.0.1".to_string(),
        api_port: 0,
        storage_root: storage_root.to_string_lossy().to_string(),
        problems_root: problems_root.to_string_lossy().to_string(),
        admin_username: "admin".to_string(),
        admin_password: "secret".to_string(),
        worker_poll_interval_ms: 3000,
        runner_timeout_sec: 30,
        runner_python_image: "python:3.12-slim-bookworm".to_string(),
        runner_memory_limit_mb: 512,
        runner_cpu_limit: 1.0,
        docker_host: None,
        log_level: "error".to_string(),
    }
}

/// Resolve the legacy TS example problem bundles used as rewrite fixtures.
pub fn examples_problems_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../llm-oj/examples/problems")
        .canonicalize()
        .expect("failed to resolve example problems root")
}

/// Build an absolute URL for the spawned test app.
pub fn api_url(app: &TestApp, path: &str) -> String {
    format!("http://{}{}", app.addr, path)
}

/// Encode credentials into an HTTP basic-auth header.
pub fn basic_auth_header(username: &str, password: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let creds = format!("{}:{}", username, password);
    format!("Basic {}", STANDARD.encode(creds))
}

/// Create a base64 ZIP containing a single `main.py` submission.
pub fn submission_zip_base64(main_py: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let cursor = std::io::Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    archive
        .start_file("main.py", options)
        .expect("failed to start main.py in zip");
    archive
        .write_all(main_py.as_bytes())
        .expect("failed to write main.py to zip");

    let cursor = archive.finish().expect("failed to finish zip");
    STANDARD.encode(cursor.into_inner())
}

/// Generate a tiny Python submission for the `sample-sum` fixture.
pub fn sample_sum_submission(expression: &str) -> String {
    [
        "from __future__ import annotations",
        "",
        "import json",
        "import sys",
        "",
        "",
        "def main() -> None:",
        "    payload = json.loads(sys.argv[1])",
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
    pre_pull_image(&docker, &config.runner_python_image)
        .await
        .expect("failed to pull runner image");
    let storage = LocalStorage::new(&config.storage_root);

    worker::cycle::run_worker_cycle(
        pool,
        &docker,
        config,
        &storage as &dyn Storage,
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
