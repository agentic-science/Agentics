//! Integration tests for worker-backed official submissions and validation runs.

mod helpers;

use helpers::{
    api_url, examples_problems_root, run_worker_once, sample_sum_submission, spawn_app_with_config,
    submission_zip_base64, test_config,
};

#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_official_submission(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_problems_root());
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

    let artifact_base64 =
        submission_zip_base64(&sample_sum_submission("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "problem_id": "sample-sum",
            "artifact_base64": artifact_base64,
            "explanation": "official eval smoke test"
        }))
        .send()
        .await
        .expect("failed to create submission")
        .json()
        .await
        .expect("failed to decode create submission response");
    let submission_id = create_response["id"]
        .as_str()
        .expect("missing submission id");

    let unauthenticated_submission_response = client
        .get(api_url(&app, &format!("/api/submissions/{submission_id}")))
        .send()
        .await
        .expect("failed to get submission without auth");
    assert_eq!(unauthenticated_submission_response.status(), 401);

    let other_agent_submission_response = client
        .get(api_url(&app, &format!("/api/submissions/{submission_id}")))
        .header("Authorization", format!("Bearer {other_token}"))
        .send()
        .await
        .expect("failed to get submission as another agent");
    assert_eq!(other_agent_submission_response.status(), 404);

    run_worker_once(&pool, &config).await;

    let submission_response = client
        .get(api_url(&app, &format!("/api/submissions/{submission_id}")))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get submission");
    assert_eq!(submission_response.status(), 200);

    let submission: serde_json::Value = submission_response
        .json()
        .await
        .expect("failed to decode submission response");
    assert_eq!(submission["status"], "completed");
    assert_eq!(submission["visible_after_eval"], true);
    assert_eq!(submission["evaluation"]["status"], "completed");
    assert_eq!(submission["evaluation"]["eval_type"], "official");
    assert_eq!(submission["evaluation"]["primary_score"], 1.0);
    assert_eq!(submission["evaluation"]["official_summary"]["score"], 1.0);
    assert_eq!(submission["evaluation"]["official_summary"]["passed"], 2);
    assert_eq!(submission["evaluation"]["official_summary"]["total"], 2);

    let job_status: (String, String) =
        sqlx::query_as("SELECT status, eval_type FROM evaluation_jobs WHERE submission_id = $1")
            .bind(submission_id)
            .fetch_one(&pool)
            .await
            .expect("failed to query evaluation job");
    let evaluation_status: (String, String, f64) = sqlx::query_as(
        "SELECT status, eval_type, primary_score FROM evaluations WHERE submission_id = $1",
    )
    .bind(submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query evaluation");

    assert_eq!(
        job_status,
        ("completed".to_string(), "official".to_string())
    );
    assert_eq!(
        evaluation_status,
        ("completed".to_string(), "official".to_string(), 1.0)
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_completes_private_validation_run_without_leaderboard(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_problems_root());
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

    let artifact_base64 =
        submission_zip_base64(&sample_sum_submission("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "problem_id": "sample-sum",
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
    assert_eq!(validation["evaluation"]["hidden_summary"]["score"], 1.0);

    let leaderboard_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leaderboard_entries")
        .fetch_one(&pool)
        .await
        .expect("failed to query leaderboard count");
    assert_eq!(leaderboard_count.0, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn worker_marks_submission_failed_when_artifact_is_missing(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_problems_root());
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

    let artifact_base64 =
        submission_zip_base64(&sample_sum_submission("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "problem_id": "sample-sum",
            "artifact_base64": artifact_base64,
            "explanation": "official eval failure test"
        }))
        .send()
        .await
        .expect("failed to create submission")
        .json()
        .await
        .expect("failed to decode create submission response");
    let submission_id = create_response["id"]
        .as_str()
        .expect("missing submission id");

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET payload_json = jsonb_set(payload_json, '{artifact_path}', to_jsonb($2::text))
        WHERE submission_id = $1
        "#,
    )
    .bind(submission_id)
    .bind("/tmp/agentics-missing-submission.zip")
    .execute(&pool)
    .await
    .expect("failed to corrupt artifact path");

    run_worker_once(&pool, &config).await;

    let submission_response = client
        .get(api_url(&app, &format!("/api/submissions/{submission_id}")))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get submission");
    assert_eq!(submission_response.status(), 200);

    let submission: serde_json::Value = submission_response
        .json()
        .await
        .expect("failed to decode submission response");
    assert_eq!(submission["status"], "failed");
    assert_eq!(submission["visible_after_eval"], false);
    assert_eq!(submission["evaluation"]["status"], "failed");
    assert!(submission["evaluation"].get("primary_score").is_none());
    assert_eq!(submission["evaluation_job"]["status"], "failed");
}
