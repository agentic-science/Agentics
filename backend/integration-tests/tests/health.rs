mod helpers;

use helpers::{api_url, spawn_app};

/// Verifies that health check works.
#[sqlx::test(migrations = "../migrations")]
async fn health_check_works(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    let response = reqwest::get(api_url(&app, "/healthz"))
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "api-server");
}
