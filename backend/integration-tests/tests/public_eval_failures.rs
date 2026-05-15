//! Integration tests for worker failure propagation during evaluation.

mod helpers;

use helpers::{
    api_url, examples_challenges_root, run_worker_once, sample_sum_solution, solution_zip_base64,
    spawn_app_with_config, test_config,
};

#[sqlx::test(migrations = "../migrations")]
async fn worker_marks_solution_submission_failed_when_artifact_is_missing(pool: sqlx::PgPool) {
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
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
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
        SET payload_json = jsonb_set(payload_json, '{artifact_key}', to_jsonb($2::text))
        WHERE solution_submission_id = $1::uuid
        "#,
    )
    .bind(solution_submission_id)
    .bind("missing/agentics-missing-solution-submission.zip")
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
