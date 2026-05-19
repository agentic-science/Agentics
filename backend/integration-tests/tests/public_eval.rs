//! Integration tests for worker-backed official solution submissions and validation runs.

mod helpers;

use std::path::Path;

use helpers::{
    api_url, copy_dir_all, examples_challenges_root, grid_routing_solution_zip_base64,
    run_worker_once, sample_sum_solution, solution_zip_base64, solution_zip_base64_with_scripts,
    spawn_app_with_config, test_config, zip_project_zip_base64,
};

const QUOTA_TEST_STORAGE_MODE_ENV: &str = "AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE";
const QUOTA_TEST_PHASE_MOUNT_ROOT_ENV: &str = "AGENTICS_TEST_RUNNER_PHASE_MOUNT_ROOT";
const QUOTA_TEST_SLOT_CLASSES_ENV: &str = "AGENTICS_TEST_RUNNER_WRITABLE_SLOT_CLASSES_MB";
const QUOTA_TEST_SETUP_SCRIPT: &str = "scripts/ops/prepare-dgx-spark-test-storage.sh";
const QUOTA_TEST_REQUIRED_SLOT_CLASS_MB: u64 = 64;

/// Creates validation disabled challenge after validating caller inputs.
fn create_validation_disabled_challenge(root: &Path) {
    let source = examples_challenges_root().join("sample-sum/v1");
    let bundle_dir = root.join("validation-disabled/v1");
    copy_dir_all(&source, &bundle_dir);

    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read copied spec"),
    )
    .expect("failed to parse copied spec");
    spec["challenge_name"] = serde_json::json!("validation-disabled");
    spec["challenge_title"] = serde_json::json!("Validation Disabled");
    for target in spec["targets"]
        .as_array_mut()
        .expect("targets should be an array")
    {
        target["validation_enabled"] = serde_json::json!(false);
    }
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write copied spec");
}

/// Returns true when the local environment can run Linux quota-sensitive tests.
fn quota_sensitive_runner_env_configured() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }

    validate_quota_sensitive_runner_env().unwrap_or_else(|message| panic!("{message}"));
    true
}

/// Build a test config that uses the validated Linux quota test root.
fn quota_sensitive_runner_config(
    storage_root: &Path,
    challenges_root: &Path,
) -> shared::config::Config {
    validate_quota_sensitive_runner_env().expect("quota env should be validated before use");
    let mut config = test_config(storage_root, challenges_root);
    config.runner_writable_storage_mode =
        std::env::var(QUOTA_TEST_STORAGE_MODE_ENV).expect("validated quota storage mode");
    config.runner_phase_mount_root =
        Some(std::env::var(QUOTA_TEST_PHASE_MOUNT_ROOT_ENV).expect("validated quota mount root"));
    config.runner_writable_slot_classes_mb =
        std::env::var(QUOTA_TEST_SLOT_CLASSES_ENV).expect("validated quota slot classes");
    config.runner_docker_layer_quota =
        std::env::var("AGENTICS_TEST_RUNNER_DOCKER_LAYER_QUOTA").is_ok_and(|value| value == "true");
    config
}

/// Validate Linux-only quota-sensitive test environment variables.
fn validate_quota_sensitive_runner_env() -> std::result::Result<(), String> {
    let storage_mode = std::env::var(QUOTA_TEST_STORAGE_MODE_ENV).ok();
    let phase_mount_root = std::env::var(QUOTA_TEST_PHASE_MOUNT_ROOT_ENV).ok();
    let slot_classes = std::env::var(QUOTA_TEST_SLOT_CLASSES_ENV).ok();

    let missing: Vec<&str> = [
        (storage_mode.is_none(), QUOTA_TEST_STORAGE_MODE_ENV),
        (phase_mount_root.is_none(), QUOTA_TEST_PHASE_MOUNT_ROOT_ENV),
        (slot_classes.is_none(), QUOTA_TEST_SLOT_CLASSES_ENV),
    ]
    .into_iter()
    .filter_map(|(is_missing, name)| is_missing.then_some(name))
    .collect();
    if !missing.is_empty() {
        return Err(quota_test_setup_error(format!(
            "missing environment variable(s): {}",
            missing.join(", ")
        )));
    }

    let storage_mode = storage_mode.expect("checked above");
    if storage_mode != "xfs-project-quota-slots" {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_STORAGE_MODE_ENV} must be xfs-project-quota-slots, got `{storage_mode}`"
        )));
    }

    let phase_mount_root = phase_mount_root.expect("checked above");
    let root = Path::new(&phase_mount_root);
    if !root.is_absolute() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_PHASE_MOUNT_ROOT_ENV} must be an absolute path, got `{phase_mount_root}`"
        )));
    }
    if !root.is_dir() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_PHASE_MOUNT_ROOT_ENV} must point to a prepared test quota root, got `{phase_mount_root}`"
        )));
    }

    let slot_classes = parse_quota_test_slot_classes(&slot_classes.expect("checked above"))?;
    if !slot_classes.contains(&QUOTA_TEST_REQUIRED_SLOT_CLASS_MB) {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_SLOT_CLASSES_ENV} must include {QUOTA_TEST_REQUIRED_SLOT_CLASS_MB} MiB because these tests lower the challenge disk limit to that class"
        )));
    }
    let slot_metadata = root
        .join("solution-run")
        .join("slots")
        .join(format!("{QUOTA_TEST_REQUIRED_SLOT_CLASS_MB}mb"))
        .join("slot-001")
        .join(".agentics-slot.json");
    if !slot_metadata.is_file() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_PHASE_MOUNT_ROOT_ENV} does not look like a prepared bounded quota root; missing {}",
            slot_metadata.display()
        )));
    }

    Ok(())
}

/// Parse comma or whitespace separated quota slot classes.
fn parse_quota_test_slot_classes(raw: &str) -> std::result::Result<Vec<u64>, String> {
    let mut parsed = Vec::new();
    for value in raw.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let class_mb = value.parse::<u64>().map_err(|error| {
            quota_test_setup_error(format!(
                "{QUOTA_TEST_SLOT_CLASSES_ENV} contains invalid slot class `{value}`: {error}"
            ))
        })?;
        if class_mb == 0 {
            return Err(quota_test_setup_error(format!(
                "{QUOTA_TEST_SLOT_CLASSES_ENV} entries must be positive"
            )));
        }
        parsed.push(class_mb);
    }
    parsed.sort_unstable();
    parsed.dedup();
    if parsed.is_empty() {
        return Err(quota_test_setup_error(format!(
            "{QUOTA_TEST_SLOT_CLASSES_ENV} must not be empty"
        )));
    }
    Ok(parsed)
}

/// Build the actionable Linux quota-test setup failure.
fn quota_test_setup_error(reason: String) -> String {
    format!(
        "Linux quota-sensitive runner tests require a prepared bounded test quota root: {reason}. Run `{QUOTA_TEST_SETUP_SCRIPT}` and export {QUOTA_TEST_STORAGE_MODE_ENV}=xfs-project-quota-slots, {QUOTA_TEST_PHASE_MOUNT_ROOT_ENV}=/srv/agentics-test/phase-mounts, and {QUOTA_TEST_SLOT_CLASSES_ENV}=64,256,1024,4096 before running these tests on Linux."
    )
}

/// Verifies that worker completes official solution submission.
#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_official_solution_submission(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "worker-e2e-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let other_register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "worker-e2e-other-agent" }))
        .send()
        .await
        .expect("failed to register other agent")
        .json()
        .await
        .expect("failed to decode other register response");
    let other_token = other_register_response["token"]
        .as_str()
        .expect("missing other token");

    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "official eval smoke test"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let solution_submission_id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    assert_eq!(create_response["note"], "sample-sum smoke solution");

    let unauthenticated_solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}"),
        ))
        .send()
        .await
        .expect("failed to get solution submission without auth");
    assert_eq!(unauthenticated_solution_submission_response.status(), 401);

    let other_agent_solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {other_token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get solution submission as another agent");
    assert_eq!(other_agent_solution_submission_response.status(), 404);

    run_worker_once(&pool, &config).await;

    let solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get solution submission");
    assert_eq!(solution_submission_response.status(), 200);

    let solution_submission: serde_json::Value = solution_submission_response
        .json()
        .await
        .expect("failed to decode solution submission response");
    assert_eq!(solution_submission["status"], "completed");
    assert_eq!(solution_submission["note"], "sample-sum smoke solution");
    assert_eq!(solution_submission["visible_after_eval"], true);
    assert_eq!(solution_submission["evaluation"]["status"], "completed");
    assert_eq!(solution_submission["evaluation"]["eval_type"], "official");
    assert_eq!(solution_submission["evaluation"]["primary_score"], 1.0);
    assert_eq!(solution_submission["evaluation"]["rank_score"], 1.0);
    assert_eq!(
        solution_submission["evaluation"]["aggregate_metrics"],
        serde_json::json!([])
    );
    assert_eq!(
        solution_submission["evaluation"]["run_metrics"],
        serde_json::json!([])
    );
    assert_eq!(
        solution_submission["evaluation"]["public_results"],
        serde_json::json!([])
    );
    assert!(solution_submission["evaluation"]["official_summary"].is_null());
    assert!(solution_submission["evaluation"]["log_key"].is_null());

    let owner_logs: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get owner-visible logs")
        .json()
        .await
        .expect("failed to decode logs response");
    assert!(
        owner_logs["log_key"].is_null(),
        "official evaluation logs must not be exposed to submitters"
    );
    assert!(
        owner_logs["content"].is_null(),
        "official evaluation log content must not be exposed to submitters"
    );

    let job_status: (String, String) = sqlx::query_as(
        "SELECT status, eval_type FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query evaluation job");
    let evaluation_status: (String, String, f64, f64, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT status, eval_type, primary_score, rank_score, aggregate_metrics_json, run_metrics_json FROM evaluations WHERE solution_submission_id = $1::uuid",
    )
    .bind(solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query evaluation");

    assert_eq!(
        job_status,
        ("completed".to_string(), "official".to_string())
    );
    assert_eq!(
        evaluation_status,
        (
            "completed".to_string(),
            "official".to_string(),
            1.0,
            1.0,
            serde_json::json!([
                { "metric_name": "score", "value": 1.0 },
                { "metric_name": "passed_cases", "value": 2.0 }
            ]),
            serde_json::json!([
                {
                    "run_name": "private-benchmark-1",
                    "metrics": [{ "metric_name": "score", "value": 1.0 }]
                },
                {
                    "run_name": "private-benchmark-2",
                    "metrics": [{ "metric_name": "score", "value": 1.0 }]
                }
            ])
        )
    );
}

/// Verifies that worker completes private validation run without leaderboard.
#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_private_validation_run_without_leaderboard(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "validation-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "validation smoke test"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run");
    assert_eq!(validation_response.status(), 200);

    let validation: serde_json::Value = validation_response
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "completed");
    assert_eq!(validation["visible_after_eval"], false);
    assert_eq!(validation["evaluation"]["eval_type"], "validation");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);
    assert_eq!(validation["evaluation"]["rank_score"], 1.0);
    assert_eq!(
        validation["evaluation"]["aggregate_metrics"],
        serde_json::json!([
            { "metric_name": "score", "value": 1.0 },
            { "metric_name": "passed_cases", "value": 3.0 }
        ])
    );

    let leaderboard_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leaderboard_entries")
        .fetch_one(&pool)
        .await
        .expect("failed to query leaderboard count");
    assert_eq!(leaderboard_count.0, 0);
}

/// Verifies that worker completes file mode validation run.
#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_file_mode_validation_run(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "file-mode-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let artifact_base64 = grid_routing_solution_zip_base64(&[
        ("public-1", "RRRRDDDD"),
        ("public-2", "DDDDRRUUUURRDDDD"),
        ("public-3", "RRDDRDRDDR"),
    ]);
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "grid-routing",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "file mode validation smoke test"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "completed");
    assert_eq!(validation["evaluation"]["validation_summary"]["passed"], 3);
    assert_eq!(
        validation["evaluation"]["run_metrics"][0],
        serde_json::json!({
            "run_name": "public-1",
            "metrics": [{ "metric_name": "score", "value": 1.0 }]
        })
    );
}

/// Verifies that worker rejects symlink declared output.
#[sqlx::test(migrations = "../migrations")]
async fn worker_rejects_symlink_declared_output(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "symlink-output-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "grid-routing",
            "target": "linux-arm64-cpu",
            "artifact_base64": grid_routing_symlink_solution_zip_base64(),
            "explanation": "symlink output should fail before scorer"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(last_error.contains("declared output file `path.txt` is a symlink"));
}

/// Verifies that worker reports build phase failure.
#[sqlx::test(migrations = "../migrations")]
async fn worker_reports_build_phase_failure(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "build-failure-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\n",
        "#!/usr/bin/env sh\nset -eu\nexit 7\n",
        "#!/usr/bin/env sh\nset -eu\npython main.py\n",
    );
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "build phase failure smoke test"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");
    assert_eq!(validation["evaluation_job"]["status"], "failed");

    let last_error: (String,) = sqlx::query_as(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(last_error.0.contains("zip_project phase failed"));
    assert!(last_error.0.contains("\"phase\":\"build\""));
}

/// Verifies that worker blocks run stage internet access.
#[sqlx::test(migrations = "../migrations")]
async fn worker_blocks_run_stage_internet_access(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "no-egress-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let run_sh = r#"#!/usr/bin/env sh
set -eu
test -f build/generated.txt
python - <<'PY'
import socket

try:
    with socket.create_connection(("1.1.1.1", 53), timeout=1):
        pass
except OSError:
    raise SystemExit(0)

raise SystemExit("run stage unexpectedly has internet access")
PY
python main.py
"#;
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        "#!/usr/bin/env sh\nset -eu\nmkdir -p build\nprintf built > build/generated.txt\n",
        run_sh,
    );
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "run stage internet probe"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "completed");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);
}

/// Verifies that worker mounts run workspace read only.
#[sqlx::test(migrations = "../migrations")]
async fn worker_mounts_run_workspace_read_only(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "readonly-run-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let run_sh = r#"#!/usr/bin/env sh
set -eu
test -f build/generated.txt
printf mutated > build/generated.txt
python main.py
"#;
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\n",
        "#!/usr/bin/env sh\nset -eu\nmkdir -p build\nprintf built > build/generated.txt\n",
        run_sh,
    );
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "run workspace readonly probe"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: (String,) = sqlx::query_as(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(last_error.0.contains("\"phase\":\"run\""));
}

/// Verifies that worker enforces run writable disk limit.
#[sqlx::test(migrations = "../migrations")]
async fn worker_enforces_run_writable_disk_limit(pool: sqlx::PgPool) {
    if !quota_sensitive_runner_env_configured() {
        return;
    }

    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = quota_sensitive_runner_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    sqlx::query(
        r#"
        UPDATE challenges
        SET spec_json = jsonb_set(spec_json, '{targets,0,resource_profile,disk_limit_mb}', '64'::jsonb)
        WHERE name = 'sample-sum'
        "#,
    )
    .execute(&pool)
    .await
    .expect("failed to lower test resource profile disk limit");

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "run-disk-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let run_sh = r#"#!/usr/bin/env sh
set -eu
dd if=/dev/zero of="$AGENTICS_OUTPUT_DIR/quota.bin" bs=1M count=80
python main.py
"#;
    let artifact_base64 = zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "disk limit probe",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        ("run.sh", run_sh.to_string()),
        (
            "main.py",
            sample_sum_solution("payload['a'] + payload['b']"),
        ),
    ]);

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "run writable disk limit probe"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(
        last_error.contains("\"phase\":\"run\""),
        "expected run phase failure, got: {last_error}"
    );

    assert!(last_error.contains("\"reason\":\"non_zero_exit\""));
}

/// Verifies that worker enforces platform-owned output file count limits.
#[sqlx::test(migrations = "../migrations")]
async fn worker_rejects_excessive_run_output_files(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "run-output-file-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let run_sh = r#"#!/usr/bin/env sh
set -eu
i=0
while [ "$i" -lt 8193 ]; do
  : > "$AGENTICS_OUTPUT_DIR/file-$i.txt"
  i=$((i + 1))
done
python main.py
"#;
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        "#!/usr/bin/env sh\nset -eu\nmkdir -p build\nprintf built > build/generated.txt\n",
        run_sh,
    );

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "run output file limit probe"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(
        last_error.contains("output file limit"),
        "unexpected last_error: {last_error}"
    );
    assert!(
        last_error.contains("resource_limit"),
        "unexpected last_error: {last_error}"
    );
}

/// Verifies quota-backed Linux runner slots enforce inode limits when configured.
#[test]
fn test_quota_root_enforces_inode_limit_when_configured() {
    if !quota_sensitive_runner_env_configured() {
        return;
    }

    let phase_mount_root =
        std::env::var(QUOTA_TEST_PHASE_MOUNT_ROOT_ENV).expect("validated quota env");
    let slot_root = Path::new(&phase_mount_root)
        .join("solution-run")
        .join("slots")
        .join(format!("{QUOTA_TEST_REQUIRED_SLOT_CLASS_MB}mb"))
        .join("slot-004");
    let probe_root = slot_root.join(format!("direct-inode-probe-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&probe_root).expect("failed to create inode probe root");

    let mut quota_hit = None;
    for index in 0..17_000 {
        let path = probe_root.join(format!("inode-{index}"));
        match std::fs::File::create(&path) {
            Ok(_) => {}
            Err(error) if is_quota_exhaustion(&error) => {
                quota_hit = Some((index, error));
                break;
            }
            Err(error) => panic!("unexpected inode probe write failure at {index}: {error}"),
        }
    }
    drop(std::fs::remove_dir_all(&probe_root));

    let Some((index, error)) = quota_hit else {
        panic!(
            "expected the prepared {QUOTA_TEST_REQUIRED_SLOT_CLASS_MB} MiB test quota slot to exhaust before 17000 files; verify {QUOTA_TEST_SETUP_SCRIPT} prepared inode quotas"
        );
    };
    assert!(
        index < 17_000,
        "quota exhaustion must happen before the full probe count: {error}"
    );
}

/// Return whether a filesystem error represents byte or inode quota exhaustion.
fn is_quota_exhaustion(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(28) | Some(122))
        || error.to_string().contains("No space left on device")
        || error.to_string().contains("Disk quota exceeded")
}

/// Verifies setup/build dependency trees are not capped by scorer-visible output limits.
#[sqlx::test(migrations = "../migrations")]
async fn setup_build_file_count_is_not_limited_by_output_cap(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.runner_max_output_files = 8;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "dependency-file-count-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let build_sh = r#"#!/usr/bin/env sh
set -eu
mkdir -p build/deps
i=0
while [ "$i" -lt 16 ]; do
  : > "build/deps/file-$i.txt"
  i=$((i + 1))
done
printf built > build/generated.txt
"#;
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        build_sh,
        "#!/usr/bin/env sh\nset -eu\ntest -f build/generated.txt\npython main.py\n",
    );

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "dependency tree should not hit output file cap"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = create_response["id"]
        .as_str()
        .expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "completed");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);
}

/// Verifies that validation run is rejected when challenge disables validation.
#[sqlx::test(migrations = "../migrations")]
async fn validation_run_is_rejected_when_challenge_disables_validation(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    create_validation_disabled_challenge(challenges.path());
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "validation-disabled-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let response = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "validation-disabled",
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64",
            "explanation": "should fail before artifact decode"
        }))
        .send()
        .await
        .expect("failed to request disabled validation");
    assert_eq!(response.status(), 400);

    let error: serde_json::Value = response.json().await.expect("failed to decode error");
    assert_eq!(error["error"], "bad_request");
    assert!(
        error["message"]
            .as_str()
            .expect("error message")
            .contains("validation pass is disabled")
    );

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    let job_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluation_jobs")
        .fetch_one(&pool)
        .await
        .expect("failed to query job count");
    assert_eq!(solution_submission_count.0, 0);
    assert_eq!(job_count.0, 0);
}

/// Verifies that validation run quota rejects and resets.
#[sqlx::test(migrations = "../migrations")]
async fn validation_run_quota_rejects_and_resets(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.validation_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "validation-quota-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let first_response = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "first validation run"
        }))
        .send()
        .await
        .expect("failed to create first validation run");
    assert_eq!(first_response.status(), 201);

    let quota_response = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64",
            "explanation": "should fail before artifact decode"
        }))
        .send()
        .await
        .expect("failed to request over-quota validation run");
    assert_eq!(quota_response.status(), 429);

    let quota_error: serde_json::Value = quota_response
        .json()
        .await
        .expect("failed to decode quota error");
    assert_eq!(quota_error["error"], "too_many_requests");
    assert!(
        quota_error["message"]
            .as_str()
            .expect("quota error message")
            .contains("validation quota exceeded")
    );

    sqlx::query("UPDATE solution_submissions SET created_at = NOW() - INTERVAL '25 hours'")
        .execute(&pool)
        .await
        .expect("failed to age validation run");

    let reset_artifact_base64 =
        solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let reset_response = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": reset_artifact_base64,
            "explanation": "validation run after quota reset"
        }))
        .send()
        .await
        .expect("failed to create validation after quota reset");
    assert_eq!(reset_response.status(), 201);

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    let job_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluation_jobs")
        .fetch_one(&pool)
        .await
        .expect("failed to query job count");
    assert_eq!(solution_submission_count.0, 2);
    assert_eq!(job_count.0, 2);
}

/// Verifies that official submission quota rejects before artifact decode.
#[sqlx::test(migrations = "../migrations")]
async fn official_submission_quota_rejects_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.official_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "official-quota-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let first_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "first official run"
        }))
        .send()
        .await
        .expect("failed to create first official run");
    assert_eq!(first_response.status(), 201);

    let quota_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64",
            "explanation": "should fail before artifact decode"
        }))
        .send()
        .await
        .expect("failed to request over-quota official run");
    assert_eq!(quota_response.status(), 429);

    let quota_error: serde_json::Value = quota_response
        .json()
        .await
        .expect("failed to decode quota error");
    assert_eq!(quota_error["error"], "too_many_requests");
    assert!(
        quota_error["message"]
            .as_str()
            .expect("quota error message")
            .contains("official quota exceeded")
    );
}

/// Verifies that official active queue limit rejects before artifact decode.
#[sqlx::test(migrations = "../migrations")]
async fn official_active_queue_limit_rejects_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.official_runs_per_agent_challenge_day = 10;
    config.max_active_official_jobs = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "official-active-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let first_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "fills active queue"
        }))
        .send()
        .await
        .expect("failed to create first official run");
    assert_eq!(first_response.status(), 201);

    let quota_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64",
            "explanation": "should fail before artifact decode"
        }))
        .send()
        .await
        .expect("failed to request over active official queue limit");
    assert_eq!(quota_response.status(), 429);

    let quota_error: serde_json::Value = quota_response
        .json()
        .await
        .expect("failed to decode active queue error");
    assert_eq!(quota_error["error"], "too_many_requests");
    assert!(
        quota_error["message"]
            .as_str()
            .expect("active queue error message")
            .contains("official evaluation queue is full")
    );
}

/// Verifies that concurrent official admission locks admit only one.
#[sqlx::test(migrations = "../migrations")]
async fn concurrent_official_admission_locks_admit_only_one(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.official_runs_per_agent_challenge_day = 1;
    config.max_active_official_jobs = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let token = register_agent_token(&client, &app, "concurrent-official-quota-agent").await;

    let artifact_a = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let artifact_b = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let request_a = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_a,
            "explanation": "concurrent official run A"
        }))
        .send();
    let request_b = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_b,
            "explanation": "concurrent official run B"
        }))
        .send();

    let (response_a, response_b) = tokio::join!(request_a, request_b);
    let mut statuses = [
        response_a.expect("official request A").status().as_u16(),
        response_b.expect("official request B").status().as_u16(),
    ];
    statuses.sort();
    assert_eq!(statuses, [201, 429]);

    let solution_submission_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM evaluation_jobs")
        .fetch_one(&pool)
        .await
        .expect("failed to query job count");
    assert_eq!(solution_submission_count, 1);
    assert_eq!(job_count, 1);
}

/// Handles register agent token for this module.
async fn register_agent_token(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    name: &str,
) -> String {
    let register_response: serde_json::Value = client
        .post(api_url(app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": name }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    register_response["token"]
        .as_str()
        .expect("missing token")
        .to_string()
}

/// Handles grid routing symlink solution zip base64 for this module.
fn grid_routing_symlink_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "symlink output probe",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\nln -sf /etc/passwd \"$AGENTICS_OUTPUT_DIR/path.txt\"\n"
                .to_string(),
        ),
    ])
}
