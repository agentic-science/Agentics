use super::*;

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
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "sample-sum").await,
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
        .send()
        .await
        .expect("failed to get solution submission");
    assert_eq!(solution_submission_response.status(), 200);

    let solution_submission: serde_json::Value = solution_submission_response
        .json()
        .await
        .expect("failed to decode solution submission response");
    assert_eq!(
        solution_submission["status"], "completed",
        "unexpected official submission response: {solution_submission:#}"
    );
    assert_eq!(solution_submission["note"], "sample-sum smoke solution");
    assert_eq!(solution_submission["visible_after_eval"], true);
    assert_eq!(
        solution_submission["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );
    assert_eq!(solution_submission["evaluation"]["status"], "completed");
    assert_eq!(solution_submission["evaluation"]["eval_type"], "official");
    assert_eq!(
        solution_submission["evaluation"]["aggregate_metrics"],
        serde_json::json!([
            { "metric_name": "score", "value": 1.0 },
            { "metric_name": "passed_cases", "value": 2.0 }
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
    assert_eq!(
        solution_submission["evaluation"]["official_summary"],
        serde_json::json!({ "score": 1.0, "passed": 2, "total": 2 })
    );
    assert!(solution_submission["evaluation"]["runner_log_storage_key"].is_null());

    let owner_logs: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get owner-visible logs")
        .json()
        .await
        .expect("failed to decode logs response");
    assert_eq!(owner_logs["availability"], "redacted_private_official");
    assert!(
        owner_logs["runner_log_storage_key"].is_null(),
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
    let evaluation_status: (String, String, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT status, eval_type, aggregate_metrics_json, run_metrics_json FROM evaluations WHERE solution_submission_id = $1::uuid",
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

/// Verifies submitter logs follow official-first result projection after admin official runs.
#[sqlx::test(migrations = "../migrations")]
async fn submitter_logs_prefer_official_redaction_after_validation_run(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.quotas.max_active_official_jobs = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;
    let admin_auth = admin_service_token_header(&app);

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "log-projection-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let validation_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "target": "linux-arm64-cpu",
            "artifact_base64": solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']")),
            "explanation": "validation first, official later"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .error_for_status()
        .expect("validation run should be accepted")
        .json()
        .await
        .expect("failed to decode validation response");
    let solution_submission_id = validation_response["id"]
        .as_str()
        .expect("missing validation submission id");
    run_worker_once(&pool, &config).await;

    let validation_logs: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation logs")
        .json()
        .await
        .expect("failed to decode validation logs");
    assert_eq!(validation_logs["availability"], "available");
    assert!(
        validation_logs["content"].is_string(),
        "validation logs should be visible to the submitter"
    );

    let official_run = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{solution_submission_id}/official-run"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue official run");
    assert_eq!(official_run.status(), 202);
    run_worker_once(&pool, &config).await;

    let official_logs: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get official-first logs")
        .json()
        .await
        .expect("failed to decode official-first logs");
    assert_eq!(official_logs["availability"], "redacted_private_official");
    assert!(official_logs["runner_log_storage_key"].is_null());
    assert!(official_logs["content"].is_null());
}

/// Verifies that the worker completes a piped-stdio official submission.
#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_piped_stdio_solution_submission(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    create_piped_stdio_challenge(challenges.path());
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "piped-stdio-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let validation_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "interactive-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": piped_stdio_sum_solution_zip_base64(),
            "explanation": "piped stdio validation smoke test"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = validation_response["id"]
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
    let validation_job_error: (Option<String>,) = sqlx::query_as(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query piped-stdio validation job error");
    let validation_runner_log = runner_log_text(
        &config,
        validation["evaluation"]["runner_log_storage_key"].as_str(),
    )
    .await;
    assert_eq!(
        validation["status"], "completed",
        "unexpected piped-stdio validation response: {validation:#}; job_error={:?}; runner_log={:?}",
        validation_job_error.0, validation_runner_log
    );
    assert_eq!(validation["evaluation"]["eval_type"], "validation");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);

    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "interactive-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": piped_stdio_sum_solution_zip_base64(),
            "explanation": "piped stdio official smoke test"
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

    run_worker_once(&pool, &config).await;

    let solution_submission: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get solution submission")
        .json()
        .await
        .expect("failed to decode solution submission response");

    assert_eq!(
        solution_submission["status"], "completed",
        "unexpected piped_stdio submission response"
    );
    assert_eq!(solution_submission["evaluation"]["eval_type"], "official");
    assert_eq!(
        solution_submission["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );
}

/// Verifies that the worker completes a coexecuted-evaluator without exposing private data to validation.
#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_coexecuted_benchmark_submission(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    let bundles = tempfile::tempdir().expect("failed to create bundle tempdir");
    let (public_bundle, private_bundle) = create_coexecuted_benchmark_bundles(bundles.path());
    let config = test_config(storage.path(), challenges.path());
    let (private_key, public_key, statement_key) =
        store_challenge_bundle_objects(&config, "coexecuted-sum", &private_bundle, &public_bundle)
            .await;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let coexecuted_challenge_name =
        agentics_domain::models::names::ChallengeName::try_new("coexecuted-sum".to_string())
            .expect("coexecuted-sum name is valid");
    sqlx::query(
        r#"
        INSERT INTO challenges (
            challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, starts_at, status
        )
        VALUES (
            $5,
            'Coexecuted Sum',
            '{"en":"Import participant code in a trusted coexecuted-evaluator.","zh":"在可信共执行评估器中导入参赛代码。"}'::jsonb,
            $1,
            $2,
            $3,
            $4,
            '2026-01-01T00:00:00Z'::timestamptz,
            'active'
        )
        "#,
    )
    .bind(private_key.as_str())
    .bind(public_key.as_str())
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
    .expect("failed to insert coexecuted challenge");
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "coexecuted-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let validation_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "coexecuted-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": coexecuted_sum_solution_zip_base64(),
            "explanation": "coexecuted validation smoke test"
        }))
        .send()
        .await
        .expect("failed to create validation run")
        .json()
        .await
        .expect("failed to decode create validation response");
    let validation_id = validation_response["id"]
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
    let validation_job_error: (Option<String>,) = sqlx::query_as(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(validation_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query coexecuted validation job error");
    let validation_runner_log = runner_log_text(
        &config,
        validation["evaluation"]["runner_log_storage_key"].as_str(),
    )
    .await;
    assert_eq!(
        validation["status"], "completed",
        "unexpected coexecuted validation response: {validation:#}; job_error={:?}; runner_log={:?}",
        validation_job_error.0, validation_runner_log
    );
    assert_eq!(validation["evaluation"]["eval_type"], "validation");
    assert_eq!(validation["evaluation"]["validation_summary"]["score"], 1.0);

    let create_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "coexecuted-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": coexecuted_sum_solution_zip_base64(),
            "explanation": "coexecuted official smoke test"
        }))
        .send()
        .await
        .expect("failed to create solution submission");
    let create_status = create_response.status();
    let create_response: serde_json::Value = create_response
        .json()
        .await
        .expect("failed to decode create solution submission response");
    assert_eq!(
        create_status,
        reqwest::StatusCode::CREATED,
        "unexpected coexecuted solution submission response: {create_response:#}"
    );
    let solution_submission_id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    run_worker_once(&pool, &config).await;

    let solution_submission: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get solution submission")
        .json()
        .await
        .expect("failed to decode solution submission response");

    assert_eq!(solution_submission["status"], "completed");
    assert_eq!(solution_submission["evaluation"]["eval_type"], "official");
    assert_eq!(
        solution_submission["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );
}
