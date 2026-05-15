mod helpers;

use helpers::{api_url, spawn_app};

/// Verifies that list challenges returns empty array.
#[sqlx::test(migrations = "../migrations")]
async fn list_challenges_returns_empty_array(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    let response = reqwest::get(api_url(&app, "/api/public/challenges"))
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    let items = body["items"].as_array().expect("items should be an array");
    assert!(items.is_empty());
}
