#![allow(dead_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use api_server::router;
use api_server::state::AppState;
use shared::config::Config;
use shared::runner::connect_docker;
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
/// Tests use this to point storage and seeded challenge roots at temporary
/// directories while exercising the real router and startup seeding path.
pub async fn spawn_app_with_config(pool: PgPool, config: Config) -> TestApp {
    if std::fs::exists(&config.challenges_root).expect("failed to inspect challenge root") {
        shared::db::ensure_challenges_seeded_from_root(&pool, &config.challenges_root)
            .await
            .expect("failed to seed challenges");
    }

    let storage = Arc::new(LocalStorage::new(&config.storage_root));

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
pub fn test_config(storage_root: &Path, challenges_root: &Path) -> Config {
    Config {
        database_url: "postgres://agentics:agentics@127.0.0.1:5432/agentics_test".to_string(),
        api_host: "127.0.0.1".to_string(),
        api_port: 0,
        storage_root: storage_root.to_string_lossy().to_string(),
        challenges_root: challenges_root.to_string_lossy().to_string(),
        admin_username: "admin".to_string(),
        admin_password: "secret".to_string(),
        allow_insecure_default_admin_credentials: false,
        cors_allowed_origins: "http://127.0.0.1:3001,http://localhost:3001".to_string(),
        worker_poll_interval_ms: 3000,
        worker_stale_job_minutes: 1,
        validation_runs_per_agent_challenge_day: 20,
        official_runs_per_agent_challenge_day: 5,
        max_active_official_jobs: 20,
        max_active_agents: 1_000,
        allow_public_agent_registration_on_non_loopback: false,
        docker_host: None,
        log_level: "error".to_string(),
    }
}

/// Resolve the bundled example challenge fixtures.
pub fn examples_challenges_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/challenges")
        .canonicalize()
        .expect("failed to resolve example challenges root")
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
                "runtime": {
                    "language": "python",
                    "language_version": "3.12",
                    "runtime_profile": "python-cpu"
                },
                "commands": {
                    "setup": "scripts/setup.sh",
                    "build": "scripts/build.sh",
                    "run": "run.sh"
                },
                "phases": {
                    "setup": { "timeout_sec": 20, "network_access": "disabled" },
                    "build": { "timeout_sec": 20, "network_access": "disabled" },
                    "run": { "timeout_sec": 20, "network_access": "disabled" }
                },
                "interface": {
                    "kind": "stdio",
                    "input_contract": "JSON on stdin",
                    "output_contract": "answer on stdout"
                },
                "dependencies": { "policy": "image_provided" }
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
                "runtime": {
                    "language": "python",
                    "language_version": "3.12",
                    "runtime_profile": "python-cpu"
                },
                "commands": {
                    "run": "run.sh"
                },
                "phases": {
                    "run": { "timeout_sec": 20, "network_access": "disabled" }
                },
                "interface": {
                    "kind": "file_system",
                    "input_contract": "case.json in AGENTICS_INPUT_DIR",
                    "output_contract": "path.txt in AGENTICS_OUTPUT_DIR"
                },
                "dependencies": { "policy": "image_provided" }
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
