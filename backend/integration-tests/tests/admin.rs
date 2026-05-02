//! Basic admin route integration tests.

mod helpers;

use helpers::{api_url, examples_challenges_root, spawn_app, spawn_app_with_config, test_config};
use shared::config::Config;

#[sqlx::test(migrations = "../migrations")]
async fn admin_read_models_power_operator_console(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let auth = helpers::basic_auth_header(&config.admin_username, &config.admin_password);
    let client = reqwest::Client::new();

    shared::db::upsert_service_heartbeat(
        &pool,
        "test-worker",
        &shared::db::HeartbeatPayload {
            status: "idle".to_string(),
            job_id: None,
            solution_submission_id: None,
            last_completed_job_id: None,
            last_failed_job_id: None,
        },
    )
    .await
    .expect("failed to insert heartbeat");

    let challenges: serde_json::Value = client
        .get(api_url(&app, "/admin/challenges"))
        .header("Authorization", auth.clone())
        .send()
        .await
        .expect("failed to list admin challenges")
        .json()
        .await
        .expect("failed to decode admin challenges");
    assert!(challenges["items"].as_array().expect("items").len() >= 2);
    assert!(challenges["items"][0].get("status").is_some());

    let submissions: serde_json::Value = client
        .get(api_url(&app, "/admin/solution-submissions"))
        .header("Authorization", auth.clone())
        .send()
        .await
        .expect("failed to list admin solution submissions")
        .json()
        .await
        .expect("failed to decode admin solution submissions");
    assert!(submissions["items"].as_array().is_some());

    let heartbeats: serde_json::Value = client
        .get(api_url(&app, "/admin/service-heartbeats"))
        .header("Authorization", auth)
        .send()
        .await
        .expect("failed to list admin service heartbeats")
        .json()
        .await
        .expect("failed to decode admin service heartbeats");
    assert_eq!(heartbeats["items"][0]["service_name"], "test-worker");
    assert_eq!(heartbeats["items"][0]["payload"]["status"], "idle");
}

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
