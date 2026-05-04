//! Integration tests for strict request validation and error response shape.

mod helpers;

use helpers::{api_url, spawn_app};

#[sqlx::test(migrations = "../migrations")]
async fn request_validation_returns_contract_error_shape(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let empty_name = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "   " }))
        .send()
        .await
        .expect("failed to send empty-name request");
    assert_eq!(empty_name.status(), 400);
    let empty_name_body: serde_json::Value = empty_name
        .json()
        .await
        .expect("failed to decode empty-name response");
    assert_eq!(empty_name_body["error"], "bad_request");
    assert!(
        empty_name_body["message"]
            .as_str()
            .unwrap()
            .contains("name")
    );

    let unknown_field = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "name": "strict-agent",
            "unexpected": true
        }))
        .send()
        .await
        .expect("failed to send unknown-field request");
    assert_eq!(unknown_field.status(), 400);
    let unknown_field_body: serde_json::Value = unknown_field
        .json()
        .await
        .expect("failed to decode unknown-field response");
    assert_eq!(unknown_field_body["error"], "bad_request");
}

#[sqlx::test(migrations = "../migrations")]
async fn zip_submission_routes_accept_declared_large_json_bodies(pool: sqlx::PgPool) {
    let app = helpers::spawn_app(pool).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(helpers::api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "large-body-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let oversized_for_axum_default = vec![0_u8; 3 * 1024 * 1024];
    let artifact_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        oversized_for_axum_default,
    );

    let response = client
        .post(helpers::api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "missing-challenge",
            "benchmark_target_id": "cpu-linux-arm64",
            "artifact_base64": artifact_base64
        }))
        .send()
        .await
        .expect("failed to submit large request body");

    assert_ne!(
        response.status(),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "route-specific body limit should exceed Axum's small JSON default"
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_invalid_target_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = helpers::test_config(storage.path(), &helpers::examples_challenges_root());
    let app = helpers::spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(helpers::api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "invalid-target-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let response = client
        .post(helpers::api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "benchmark_target_id": "cpu-linux-ppc64le",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send invalid-target submission");
    assert_eq!(response.status(), 400);

    let error: serde_json::Value = response.json().await.expect("failed to decode error");
    assert_eq!(error["error"], "bad_request");
    assert!(
        error["message"]
            .as_str()
            .expect("error message")
            .contains("benchmark target")
    );

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 0);
}
