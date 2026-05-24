#![allow(dead_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentics_config::{AgentRegistrationMode, Config, RunnerWritableStorageMode};
use agentics_runner::connect_docker;
use agentics_storage::{LocalStorage, Storage};
use api_server::admin_auth_throttle::AdminAuthThrottle;
use api_server::router;
use api_server::state::AppState;
use chrono::{Duration, Utc};
use secrecy::SecretString;
use sqlx::PgPool;
use tokio::task::JoinHandle;

/// Resolve the generated id for a published challenge fixture.
pub async fn published_challenge_id(pool: &PgPool, challenge_name: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT challenge_id::text FROM challenges WHERE name = $1 LIMIT 1",
    )
    .bind(challenge_name)
    .fetch_one(pool)
    .await
    .expect("published challenge id should exist")
}

/// Running test server bound to an ephemeral local port.
pub struct TestApp {
    pub addr: SocketAddr,
    pub _client: reqwest::Client,
    server_task: JoinHandle<()>,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

/// Creator session material used by tests instead of a live GitHub OAuth round trip.
pub struct TestCreatorSession {
    pub agent_id: String,
    pub cookie_header: String,
    pub csrf_token: String,
}

/// Spawn the API server with environment-derived config.
pub async fn spawn_app(pool: PgPool) -> TestApp {
    let mut config = Config::from_env().expect("failed to load config");
    config.agent_registration_mode = AgentRegistrationMode::Public;
    spawn_app_with_config(pool, config).await
}

/// Spawn the API server with a caller-provided config.
///
/// Tests use this to point storage and seeded challenge roots at temporary
/// directories while exercising the real router and startup seeding path.
pub async fn spawn_app_with_config(pool: PgPool, config: Config) -> TestApp {
    if std::fs::exists(&config.challenges_root).expect("failed to inspect challenge root") {
        agentics_persistence::Repositories::new(&pool)
            .maintenance()
            .ensure_challenges_seeded_from_root(&config.challenges_root, &config.storage_root)
            .await
            .expect("failed to seed challenges");
    }

    let storage = Arc::new(LocalStorage::new(&config.storage_root));

    let state = AppState {
        db: pool,
        config: Arc::new(config.clone()),
        storage,
        admin_auth_throttle: Arc::new(
            AdminAuthThrottle::new().expect("admin auth throttle should initialize"),
        ),
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
        server_task,
    }
}

/// Build an isolated config for integration tests.
pub fn test_config(storage_root: &Path, challenges_root: &Path) -> Config {
    Config {
        database_url: SecretString::from(
            "postgres://agentics:agentics@127.0.0.1:5432/agentics_test",
        ),
        api_host: "127.0.0.1".to_string(),
        api_port: 0,
        storage_root: storage_root.to_string_lossy().to_string(),
        challenges_root: challenges_root.to_string_lossy().to_string(),
        admin_username: "admin".to_string(),
        admin_password: SecretString::from("secret"),
        allow_insecure_default_admin_credentials: false,
        cors_allowed_origins: "http://127.0.0.1:3001,http://localhost:3001".to_string(),
        moltbook_submolt_name: "agentics-platform"
            .parse()
            .expect("valid test Moltbook Submolt name"),
        moltbook_submolt_url: "https://www.moltbook.com/m/agentics-platform"
            .parse()
            .expect("valid test Moltbook Submolt URL"),
        worker_poll_interval_ms: 3000,
        worker_stale_job_minutes: 1,
        worker_accelerators: agentics_config::WorkerAccelerators::None,
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
        github_oauth_client_id: Some("test-client-id".to_string()),
        github_oauth_client_secret: Some(SecretString::from("test-client-secret")),
        github_oauth_redirect_url: Some(
            "http://127.0.0.1/auth/github/callback"
                .parse()
                .expect("valid test GitHub OAuth redirect URL"),
        ),
        github_oauth_authorize_url: "https://github.com/login/oauth/authorize"
            .parse()
            .expect("valid test GitHub OAuth authorize URL"),
        github_oauth_token_url: "https://github.com/login/oauth/access_token"
            .parse()
            .expect("valid test GitHub OAuth token URL"),
        github_api_user_url: "https://api.github.com/user"
            .parse()
            .expect("valid test GitHub API user URL"),
        web_session_cookie_name: "agentics_session".to_string(),
        web_csrf_cookie_name: "agentics_csrf".to_string(),
        web_session_ttl_hours: 24,
        web_session_cookie_secure: false,
        agent_registration_mode: AgentRegistrationMode::Public,
        docker_host: std::env::var("AGENTICS_TEST_DOCKER_HOST").ok(),
        host_probe_mode: agentics_config::HostProbeMode::Off,
        runner_security_profile: agentics_config::RunnerSecurityProfile::Development,
        require_digest_pinned_images: false,
        runner_writable_storage_mode: RunnerWritableStorageMode::Unbounded,
        runner_runtime_root: None,
        runner_phase_mount_root: None,
        runner_writable_slot_classes_mb: "64,256,1024,4096".to_string(),
        runner_docker_layer_quota: false,
        runner_max_output_files: 8192,
        runner_max_output_dirs: 1024,
        runner_max_output_depth: 32,
        runner_max_runs: 12,
        runner_max_result_json_bytes: 4 * 1024 * 1024,
        runner_max_public_results: 1024,
        runner_max_result_log_bytes: 256 * 1024,
        runner_max_interaction_bytes_per_direction: 16 * 1024 * 1024,
        runner_interaction_shutdown_grace_secs: 2,
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

/// Encode credentials into an HTTP basic-auth header.
pub fn basic_auth_header(username: &str, password: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let creds = format!("{}:{}", username, password);
    format!("Basic {}", STANDARD.encode(creds))
}

/// Insert a GitHub OAuth-equivalent creator session for integration tests.
pub async fn create_creator_session(
    pool: &PgPool,
    github_user_id: i64,
    github_login: &str,
) -> TestCreatorSession {
    let fallback_agent_id = agentics_domain::models::ids::AgentId::generate();
    let repos = agentics_persistence::Repositories::new(pool);
    let agent_id = repos
        .sessions()
        .upsert_github_creator_agent(&fallback_agent_id, github_user_id, github_login, 1_000)
        .await
        .expect("creator account should upsert");
    let session_token = agentics_services::auth::create_web_session_token();
    let csrf_token = agentics_services::auth::create_csrf_token();
    repos
        .sessions()
        .create_creator_session(&agentics_persistence::CreateCreatorSessionInput {
            session_id: uuid::Uuid::new_v4().to_string(),
            session_token_hash: agentics_services::auth::hash_opaque_token(&session_token),
            csrf_token_hash: agentics_services::auth::hash_opaque_token(&csrf_token),
            agent_id: agent_id.as_str().to_string(),
            github_user_id,
            github_login: github_login.to_string(),
            expires_at: Utc::now()
                .checked_add_signed(Duration::hours(24))
                .expect("test creator session TTL should not overflow"),
        })
        .await
        .expect("creator session should insert");

    TestCreatorSession {
        agent_id: agent_id.as_str().to_string(),
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

/// Create a base64 ZIP containing a C solution for the matrix multiplication demo.
pub fn matrix_multiplication_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "matrix multiplication C baseline",
                "commands": {
                    "build": "scripts/build.sh",
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "scripts/build.sh",
            "#!/usr/bin/env sh\nset -eu\ncc -O2 -std=c11 -Wall -Wextra -o matrix_solution main.c\n"
                .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\n./matrix_solution \"$AGENTICS_INPUT_DIR/input.bin\" \"$AGENTICS_OUTPUT_DIR/output.bin\"\n"
                .to_string(),
        ),
        ("main.c", matrix_multiplication_c_source()),
    ])
}

/// Handles matrix multiplication c source for this module.
fn matrix_multiplication_c_source() -> String {
    r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int read_exact(FILE *file, void *buffer, size_t bytes) {
    return fread(buffer, 1, bytes, file) == bytes ? 0 : 1;
}

static int write_exact(FILE *file, const void *buffer, size_t bytes) {
    return fwrite(buffer, 1, bytes, file) == bytes ? 0 : 1;
}

static void matmul(const float *a, const float *b, float *c, uint32_t m, uint32_t k, uint32_t n) {
    for (uint32_t row = 0; row < m; row++) {
        for (uint32_t col = 0; col < n; col++) {
            float acc = 0.0f;
            for (uint32_t inner = 0; inner < k; inner++) {
                acc += a[(size_t)row * k + inner] * b[(size_t)inner * n + col];
            }
            c[(size_t)row * n + col] = acc;
        }
    }
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s INPUT OUTPUT\n", argv[0]);
        return 2;
    }

    FILE *input = fopen(argv[1], "rb");
    if (input == NULL) {
        perror("open input");
        return 1;
    }
    FILE *output = fopen(argv[2], "wb");
    if (output == NULL) {
        perror("open output");
        fclose(input);
        return 1;
    }

    char magic[8];
    uint32_t cases = 0, m = 0, k = 0, n = 0;
    if (read_exact(input, magic, sizeof(magic)) || memcmp(magic, "AGMMIN1", 7) != 0 ||
        read_exact(input, &cases, sizeof(cases)) || read_exact(input, &m, sizeof(m)) ||
        read_exact(input, &k, sizeof(k)) || read_exact(input, &n, sizeof(n))) {
        fprintf(stderr, "invalid input header\n");
        fclose(input);
        fclose(output);
        return 1;
    }

    const char output_magic[8] = {'A', 'G', 'M', 'M', 'O', 'U', 'T', '1'};
    if (write_exact(output, output_magic, sizeof(output_magic)) ||
        write_exact(output, &cases, sizeof(cases)) || write_exact(output, &m, sizeof(m)) ||
        write_exact(output, &n, sizeof(n))) {
        fprintf(stderr, "failed to write output header\n");
        fclose(input);
        fclose(output);
        return 1;
    }

    size_t a_len = (size_t)m * k;
    size_t b_len = (size_t)k * n;
    size_t c_len = (size_t)m * n;
    float *a = calloc(a_len, sizeof(float));
    float *b = calloc(b_len, sizeof(float));
    float *c = calloc(c_len, sizeof(float));
    if (a == NULL || b == NULL || c == NULL) {
        fprintf(stderr, "allocation failed\n");
        free(a);
        free(b);
        free(c);
        fclose(input);
        fclose(output);
        return 1;
    }

    for (uint32_t case_index = 0; case_index < cases; case_index++) {
        if (read_exact(input, a, a_len * sizeof(float)) || read_exact(input, b, b_len * sizeof(float))) {
            fprintf(stderr, "truncated input case\n");
            free(a);
            free(b);
            free(c);
            fclose(input);
            fclose(output);
            return 1;
        }
        matmul(a, b, c, m, k, n);
        if (write_exact(output, c, c_len * sizeof(float))) {
            fprintf(stderr, "failed to write output case\n");
            free(a);
            free(b);
            free(c);
            fclose(input);
            fclose(output);
            return 1;
        }
    }

    free(a);
    free(b);
    free(c);
    fclose(input);
    fclose(output);
    return 0;
}
"#
    .to_string()
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
