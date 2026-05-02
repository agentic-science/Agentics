//! Basic admin route integration tests.

mod helpers;

use helpers::{api_url, spawn_app};
use shared::config::Config;

#[sqlx::test(migrations = "../migrations")]
async fn create_challenge_and_publish_version(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;
    let config = Config::from_env().expect("failed to load config");

    // Successful admin creation verifies basic-auth extraction and challenge upsert.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .header(
            "Authorization",
            helpers::basic_auth_header(&config.admin_username, &config.admin_password),
        )
        .json(&serde_json::json!({
            "id": "test-challenge",
            "slug": "test-challenge",
            "title": "Test Challenge",
            "description": "A test challenge"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    assert_eq!(body["id"], "test-challenge");
    assert_eq!(body["title"], "Test Challenge");

    // Publishing still validates bundle paths before writing a version row.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges/test-challenge/versions"))
        .header(
            "Authorization",
            helpers::basic_auth_header(&config.admin_username, &config.admin_password),
        )
        .json(&serde_json::json!({
            "bundle_path": "/nonexistent/bundle"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(!response.status().is_success());
}

#[sqlx::test(migrations = "../migrations")]
async fn admin_routes_require_auth(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .json(&serde_json::json!({
            "id": "test-challenge",
            "title": "Test Challenge"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}
