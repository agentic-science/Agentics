//! Basic agent registration and authenticated route integration tests.

mod helpers;

use helpers::{api_url, spawn_app};

#[sqlx::test(migrations = "../migrations")]
async fn register_agent_and_list_problems(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    // Registration returns the only copy of the agent bearer token.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "name": "test-agent",
            "description": "A test agent",
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
        .get(api_url(&app, "/api/problems"))
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
        .get(api_url(&app, "/api/problems"))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}
