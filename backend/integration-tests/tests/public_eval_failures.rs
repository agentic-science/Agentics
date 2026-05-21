//! Integration tests for worker failure propagation during evaluation.

mod helpers;

use helpers::{
    api_url, examples_challenges_root, run_worker_once, sample_sum_solution, solution_zip_base64,
    spawn_app_with_config, test_config,
};

/// Verifies that worker keeps sanitized logs for failed validation runs.
#[sqlx::test(migrations = "../migrations")]
async fn worker_keeps_failed_validation_run_logs_when_artifact_is_missing(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "worker-failure-agent" }))
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
            "explanation": "validation eval failure test"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let validation_run_id = create_response["id"]
        .as_str()
        .expect("missing validation run id");

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET payload_json = jsonb_set(payload_json, '{artifact_key}', to_jsonb($2::text))
        WHERE solution_submission_id = $1::uuid
        "#,
    )
    .bind(validation_run_id)
    .bind("missing/agentics-missing-solution-submission.zip")
    .execute(&pool)
    .await
    .expect("failed to corrupt artifact path");

    run_worker_once(&pool, &config).await;

    let solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_run_id}"),
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
    assert_eq!(solution_submission["status"], "failed");
    assert_eq!(solution_submission["visible_after_eval"], false);
    assert_eq!(solution_submission["evaluation"]["status"], "failed");
    assert!(
        solution_submission["evaluation"]["log_key"].is_string(),
        "failed validation evaluations should keep a runner log key"
    );
    assert!(
        solution_submission["evaluation"]
            .get("rank_score")
            .is_none()
    );
    assert_eq!(solution_submission["evaluation_job"]["status"], "failed");

    let logs_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{validation_run_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get validation failure logs");
    assert_eq!(logs_response.status(), 200);
}
