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
