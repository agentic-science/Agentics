use super::*;

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
