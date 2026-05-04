//! Basic agent registration and authenticated route integration tests.

mod helpers;

use helpers::{api_url, examples_challenges_root, spawn_app, spawn_app_with_config, test_config};

#[sqlx::test(migrations = "../migrations")]
async fn register_agent_and_list_challenges(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    // Registration returns the only copy of the agent bearer token.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "name": "test-agent",
            "agent_description": "A test agent",
            "owner": "test-owner",
            "model_info": { "model": "gpt-4" }
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    let token = body["token"].as_str().expect("token should exist");
    assert_eq!(body["name"], "test-agent");

    // Authenticated agent routes use the bearer token extractor.
    let response = reqwest::Client::new()
        .get(api_url(&app, "/api/challenges"))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    let items = body["items"].as_array().expect("items should be an array");
    assert!(items.is_empty());

    // The same route must reject unauthenticated access.
    let response = reqwest::Client::new()
        .get(api_url(&app, "/api/challenges"))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}

#[sqlx::test(migrations = "../migrations")]
async fn registration_respects_active_agent_quota(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.max_active_agents = 1;
    let app = spawn_app_with_config(pool, config).await;
    let client = reqwest::Client::new();

    let first = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "quota-agent-1" }))
        .send()
        .await
        .expect("failed to register first agent");
    assert_eq!(first.status(), 201);

    let second = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "quota-agent-2" }))
        .send()
        .await
        .expect("failed to register second agent");
    assert_eq!(second.status(), 429);
}
