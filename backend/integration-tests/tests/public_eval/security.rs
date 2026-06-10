use super::*;

/// Verifies that validation refuses private-benchmark challenges without a distinct public bundle.
#[sqlx::test(migrations = "../migrations")]
async fn validation_rejects_private_benchmark_bundle_alias(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    let bundles = tempfile::tempdir().expect("failed to create bundle tempdir");
    let (_public_bundle, private_bundle) = create_coexecuted_benchmark_bundles(bundles.path());
    let config = test_config(storage.path(), challenges.path());
    let (private_key, _public_key, statement_key) =
        store_challenge_bundle_objects(&config, "coexecuted-sum", &private_bundle, &private_bundle)
            .await;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let coexecuted_challenge_name =
        agentics_domain::models::names::ChallengeName::try_new("coexecuted-sum".to_string())
            .expect("coexecuted-sum name is valid");
    sqlx::query(
        r#"
        INSERT INTO challenges (
            challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, starts_at, status
        )
        VALUES (
            $4,
            'Coexecuted Sum',
            '{"en":"Import participant code in a trusted coexecuted-evaluator.","zh":"在可信共执行评估器中导入参赛代码。"}'::jsonb,
            $1,
            $1,
            $2,
            $3,
            '2026-01-01T00:00:00Z'::timestamptz,
            'active'
        )
        "#,
    )
    .bind(private_key.as_str())
    .bind(statement_key.as_str())
    .bind(
        serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(private_bundle.join("spec.json"))
                .expect("failed to read coexecuted spec"),
        )
        .expect("failed to parse coexecuted spec"),
    )
    .bind(coexecuted_challenge_name.as_str())
    .execute(&pool)
    .await
    .expect("failed to insert aliased coexecuted challenge");
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "coexecuted-alias-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let validation_response = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "coexecuted-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": coexecuted_sum_solution_zip_base64(),
            "explanation": "should fail before private bundle validation"
        }))
        .send()
        .await
        .expect("failed to submit validation request");

    assert_eq!(validation_response.status(), 400);
    let body: serde_json::Value = validation_response
        .json()
        .await
        .expect("failed to decode validation error");
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("missing error message: {body:#}"));
    assert!(
        message.contains("distinct public bundle key"),
        "unexpected validation error: {body:#}"
    );
    let submitted_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
        .fetch_one(&pool)
        .await
        .expect("failed to count submissions");
    assert_eq!(submitted_count.0, 0);
}

/// Verifies that piped-stdio transcript limits fail the run before result persistence.
#[sqlx::test(migrations = "../migrations")]
async fn worker_rejects_piped_stdio_interaction_limit(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    create_piped_stdio_challenge(challenges.path());
    let mut config = test_config(storage.path(), challenges.path());
    config.runner.max_interaction_bytes_per_direction = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "piped-limit-agent" }))
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "interactive-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": piped_stdio_sum_solution_zip_base64(),
            "explanation": "piped stdio limit smoke test"
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
        .send()
        .await
        .expect("failed to get validation run")
        .json()
        .await
        .expect("failed to decode validation response");
    let job_error: (Option<String>,) = sqlx::query_as(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query validation job error");

    assert_eq!(validation["status"], "failed");
    assert!(
        job_error
            .0
            .as_deref()
            .is_some_and(|message| message.contains("interaction output exceeded")),
        "unexpected piped_stdio interaction-limit job error: {:?}",
        job_error.0
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "sample-sum").await,
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "grid-routing").await,
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "grid-routing").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": grid_routing_symlink_solution_zip_base64(),
            "explanation": "symlink output should fail before evaluator"
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "sample-sum").await,
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
    assert!(
        last_error.0.contains("zip_project phase failed"),
        "last_error={}",
        last_error.0
    );
    assert!(
        last_error.0.contains("\"phase\":\"build\""),
        "last_error={}",
        last_error.0
    );
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "sample-sum").await,
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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "sample-sum").await,
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
    assert!(
        last_error.0.contains("\"phase\":\"run\""),
        "last_error={}",
        last_error.0
    );
}
