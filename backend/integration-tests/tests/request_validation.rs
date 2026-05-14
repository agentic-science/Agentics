//! Integration tests for strict request validation and error response shape.

mod helpers;

use std::path::Path;

use helpers::{
    api_url, copy_dir_all, examples_challenges_root, spawn_app, spawn_app_with_config, test_config,
};

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
            "round_id": "main",
            "benchmark_target_id": "linux-arm64-cpu",
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
            "round_id": "main",
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

#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_invalid_round_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "invalid-round-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let missing_round = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "benchmark_target_id": "linux-arm64-cpu",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send missing-round submission");
    assert_eq!(missing_round.status(), 400);

    let unknown_round = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "round_id": "missing-round",
            "benchmark_target_id": "linux-arm64-cpu",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send unknown-round submission");
    assert_eq!(unknown_round.status(), 400);
    let unknown_error: serde_json::Value =
        unknown_round.json().await.expect("failed to decode error");
    assert!(
        unknown_error["message"]
            .as_str()
            .expect("message")
            .contains("round")
    );

    let malformed_round = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "round_id": "Main Round!",
            "benchmark_target_id": "linux-arm64-cpu",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send malformed-round submission");
    assert_eq!(malformed_round.status(), 400);
    let malformed_error: serde_json::Value = malformed_round
        .json()
        .await
        .expect("failed to decode error");
    assert!(
        malformed_error["message"]
            .as_str()
            .expect("message")
            .contains("round_id")
    );

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_unopened_and_closed_rounds_before_artifact_decode(
    pool: sqlx::PgPool,
) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    write_round_window_challenge(
        challenges.path(),
        "future-round",
        Some("2999-01-01T00:00:00Z"),
        None,
    );
    write_round_window_challenge(
        challenges.path(),
        "closed-round",
        Some("2000-01-01T00:00:00Z"),
        Some("2000-01-02T00:00:00Z"),
    );
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "round-window-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    for (challenge_id, expected_message) in
        [("future-round", "not open yet"), ("closed-round", "closed")]
    {
        let response = client
            .post(api_url(&app, "/api/solution-submissions"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "challenge_id": challenge_id,
                "round_id": "main",
                "benchmark_target_id": "linux-arm64-cpu",
                "artifact_base64": "not-base64"
            }))
            .send()
            .await
            .expect("failed to send round-window submission");
        assert_eq!(response.status(), 400);
        let error: serde_json::Value = response.json().await.expect("failed to decode error");
        assert!(
            error["message"]
                .as_str()
                .expect("message")
                .contains(expected_message)
        );
    }

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 0);
}

fn write_round_window_challenge(
    root: &Path,
    challenge_id: &str,
    opens_at: Option<&str>,
    closes_at: Option<&str>,
) {
    let bundle_dir = root.join(challenge_id).join("v1");
    copy_dir_all(
        &examples_challenges_root().join("sample-sum/v1"),
        &bundle_dir,
    );
    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("failed to read spec"))
            .expect("failed to parse spec");
    spec["challenge_id"] = serde_json::json!(challenge_id);
    spec["challenge_title"] = serde_json::json!(challenge_id);
    if let Some(opens_at) = opens_at {
        spec["rounds"][0]["opens_at"] = serde_json::json!(opens_at);
    }
    if let Some(closes_at) = closes_at {
        spec["rounds"][0]["closes_at"] = serde_json::json!(closes_at);
    }
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
}
