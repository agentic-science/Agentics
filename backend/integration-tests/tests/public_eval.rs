//! Integration tests for worker-backed official solution submissions and validation runs.

mod helpers;

use std::path::Path;

use helpers::{
    api_url, copy_dir_all, examples_challenges_root, grid_routing_solution_zip_base64,
    run_worker_once, sample_sum_solution, solution_zip_base64, solution_zip_base64_with_scripts,
    spawn_app_with_config, test_config, zip_project_zip_base64,
};

fn create_validation_disabled_challenge(root: &Path) {
    let source = examples_challenges_root().join("sample-sum/v1");
    let bundle_dir = root.join("validation-disabled/v1");
    copy_dir_all(&source, &bundle_dir);

    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read copied spec"),
    )
    .expect("failed to parse copied spec");
    spec["challenge_id"] = serde_json::json!("validation-disabled");
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

#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_official_solution_submission(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "worker-e2e-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let other_register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "worker-e2e-other-agent" }))
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
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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

    let unauthenticated_solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/solution-submissions/{solution_submission_id}"),
        ))
        .send()
        .await
        .expect("failed to get solution submission without auth");
    assert_eq!(unauthenticated_solution_submission_response.status(), 401);

    let other_agent_solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {other_token}"))
        .send()
        .await
        .expect("failed to get solution submission as another agent");
    assert_eq!(other_agent_solution_submission_response.status(), 404);

    run_worker_once(&pool, &config).await;

    let solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get solution submission");
    assert_eq!(solution_submission_response.status(), 200);

    let solution_submission: serde_json::Value = solution_submission_response
        .json()
        .await
        .expect("failed to decode solution submission response");
    assert_eq!(solution_submission["status"], "completed");
    assert_eq!(solution_submission["visible_after_eval"], true);
    assert_eq!(solution_submission["evaluation"]["status"], "completed");
    assert_eq!(solution_submission["evaluation"]["eval_type"], "official");
    assert_eq!(solution_submission["evaluation"]["primary_score"], 1.0);
    assert_eq!(solution_submission["evaluation"]["rank_score"], 1.0);
    assert_eq!(
        solution_submission["evaluation"]["aggregate_metrics"],
        serde_json::json!([
            { "metric_id": "score", "value": 1.0 },
            { "metric_id": "passed_cases", "value": 2.0 }
        ])
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
    assert!(solution_submission["evaluation"]["log_path"].is_null());

    let job_status: (String, String) = sqlx::query_as(
        "SELECT status, eval_type FROM evaluation_jobs WHERE solution_submission_id = $1",
    )
    .bind(solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query evaluation job");
    let evaluation_status: (String, String, f64, f64, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT status, eval_type, primary_score, rank_score, aggregate_metrics_json, run_metrics_json FROM evaluations WHERE solution_submission_id = $1",
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
                { "metric_id": "score", "value": 1.0 },
                { "metric_id": "passed_cases", "value": 2.0 }
            ]),
            serde_json::json!([
                {
                    "run_id": "private-benchmark-1",
                    "metrics": [{ "metric_id": "score", "value": 1.0 }]
                },
                {
                    "run_id": "private-benchmark-2",
                    "metrics": [{ "metric_id": "score", "value": 1.0 }]
                }
            ])
        )
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_private_validation_run_without_leaderboard(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "validation-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
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
            { "metric_id": "score", "value": 1.0 },
            { "metric_id": "passed_cases", "value": 3.0 }
        ])
    );

    let leaderboard_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leaderboard_entries")
        .fetch_one(&pool)
        .await
        .expect("failed to query leaderboard count");
    assert_eq!(leaderboard_count.0, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_file_mode_validation_run(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "file-mode-agent" }))
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
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "grid-routing",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
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
            "run_id": "public-1",
            "metrics": [{ "metric_id": "score", "value": 1.0 }]
        })
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_rejects_symlink_declared_output(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "symlink-output-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "grid-routing",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(last_error.contains("declared output file `path.txt` is a symlink"));
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_reports_build_phase_failure(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "build-failure-agent" }))
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
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");
    assert_eq!(validation["evaluation_job"]["status"], "failed");

    let last_error: (String,) =
        sqlx::query_as("SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1")
            .bind(validation_id)
            .fetch_one(&pool)
            .await
            .expect("failed to query failed job");
    assert!(last_error.0.contains("zip_project phase failed"));
    assert!(last_error.0.contains("\"phase\":\"build\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_blocks_run_stage_internet_access(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "no-egress-agent" }))
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
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "completed");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_mounts_run_workspace_read_only(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "readonly-run-agent" }))
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
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: (String,) =
        sqlx::query_as("SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1")
            .bind(validation_id)
            .fetch_one(&pool)
            .await
            .expect("failed to query failed job");
    assert!(last_error.0.contains("\"phase\":\"run\""));
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_enforces_run_writable_disk_limit(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "run-disk-limit-agent" }))
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
                "runtime": {
                    "language": "python",
                    "language_version": "3.12",
                    "runtime_profile": "python-cpu"
                },
                "commands": {
                    "run": "run.sh"
                },
                "phases": {
                    "run": {
                        "timeout_sec": 20,
                        "disk_limit_mb": 64,
                        "network_access": "disabled"
                    }
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
        ("run.sh", run_sh.to_string()),
        (
            "main.py",
            sample_sum_solution("payload['a'] + payload['b']"),
        ),
    ]);

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
            &format!("/api/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    assert_eq!(validation["status"], "failed");
    assert_eq!(validation["evaluation"]["status"], "failed");

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query failed job");
    assert!(
        last_error.contains("\"phase\":\"run\""),
        "expected run phase failure, got: {last_error}"
    );

    let bounded_slots = std::env::var("AGENTICS_TEST_RUNNER_WRITABLE_STORAGE_MODE")
        .is_ok_and(|value| value == "xfs-project-quota-slots");
    if bounded_slots {
        assert!(last_error.contains("\"reason\":\"non_zero_exit\""));
    } else {
        assert!(last_error.contains("phase exceeded disk limit"));
    }
}

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
        .json(&serde_json::json!({ "name": "validation-disabled-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let response = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "validation-disabled",
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

#[sqlx::test(migrations = "../migrations")]
async fn validation_run_quota_rejects_and_resets(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.validation_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "validation-quota-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let first_response = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "first validation run"
        }))
        .send()
        .await
        .expect("failed to create first validation run");
    assert_eq!(first_response.status(), 201);

    let quota_response = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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

#[sqlx::test(migrations = "../migrations")]
async fn official_submission_quota_rejects_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.official_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "official-quota-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let first_response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "first official run"
        }))
        .send()
        .await
        .expect("failed to create first official run");
    assert_eq!(first_response.status(), 201);

    let quota_response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
        .json(&serde_json::json!({ "name": "official-active-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));

    let first_response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "fills active queue"
        }))
        .send()
        .await
        .expect("failed to create first official run");
    assert_eq!(first_response.status(), 201);

    let quota_response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_a,
            "explanation": "concurrent official run A"
        }))
        .send();
    let request_b = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
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

async fn register_agent_token(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    name: &str,
) -> String {
    let register_response: serde_json::Value = client
        .post(api_url(app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": name }))
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

fn grid_routing_symlink_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "runtime": {
                    "language": "shell",
                    "language_version": "posix",
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
            "#!/usr/bin/env sh\nset -eu\nln -sf /etc/passwd \"$AGENTICS_OUTPUT_DIR/path.txt\"\n"
                .to_string(),
        ),
    ])
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_marks_solution_submission_failed_when_artifact_is_missing(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "worker-failure-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "official eval failure test"
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

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET payload_json = jsonb_set(payload_json, '{artifact_path}', to_jsonb($2::text))
        WHERE solution_submission_id = $1
        "#,
    )
    .bind(solution_submission_id)
    .bind("/tmp/agentics-missing-solution_submission.zip")
    .execute(&pool)
    .await
    .expect("failed to corrupt artifact path");

    run_worker_once(&pool, &config).await;

    let solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get solution submission");
    assert_eq!(solution_submission_response.status(), 200);

    let solution_submission: serde_json::Value = solution_submission_response
        .json()
        .await
        .expect("failed to decode solution submission response");
    assert_eq!(solution_submission["status"], "failed");
    assert_eq!(solution_submission["visible_after_eval"], false);
    assert_eq!(solution_submission["evaluation"]["status"], "failed");
    assert!(
        solution_submission["evaluation"]
            .get("primary_score")
            .is_none()
    );
    assert_eq!(solution_submission["evaluation_job"]["status"], "failed");
}
