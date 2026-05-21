//! Integration tests for pioneer-code gated agent registration.

mod helpers;

use helpers::{
    api_url, basic_auth_header, examples_challenges_root, spawn_app_with_config, test_config,
};
use reqwest::StatusCode;
use shared::config::AgentRegistrationMode;
use shared::models::ids::AgentPioneerCodeId;

/// Verifies default MVP registration mode rejects code-free registration and consumes finite codes.
#[sqlx::test(migrations = "../migrations")]
async fn pioneer_code_mode_gates_agent_registration(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let missing_code = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "missing-code-agent" }))
        .send()
        .await
        .expect("failed to register without code");
    assert_eq!(missing_code.status(), 403);

    let malformed_code = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "display_name": "malformed-code-agent",
            "pioneer_code": "BAD-CODE"
        }))
        .send()
        .await
        .expect("failed to register with malformed code");
    assert_eq!(malformed_code.status(), 403);

    let created: serde_json::Value = client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth.clone())
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "code": "jack-deadbeef",
            "note": "early private beta",
            "max_uses": 1
        }))
        .send()
        .await
        .expect("failed to create pioneer code")
        .json()
        .await
        .expect("failed to decode pioneer code");
    assert_eq!(created["code"]["code_display"], "jack-deadbeef");
    assert_eq!(created["code"]["label"], "jack");
    assert_eq!(created["code"]["note"], "early private beta");
    assert_eq!(created["code"]["use_count"], 0);

    let registered: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "display_name": "pioneer-agent",
            "pioneer_code": "jack-deadbeef"
        }))
        .send()
        .await
        .expect("failed to register with pioneer code")
        .json()
        .await
        .expect("failed to decode registration");
    let token = registered["token"].as_str().expect("token should exist");
    let code_id = created["code"]["id"]
        .as_str()
        .expect("code id should exist");

    let exhausted = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "display_name": "second-agent",
            "pioneer_code": "jack-deadbeef"
        }))
        .send()
        .await
        .expect("failed to send exhausted registration");
    assert_eq!(exhausted.status(), 403);

    let detail: serde_json::Value = client
        .get(api_url(&app, &format!("/admin/pioneer-codes/{code_id}")))
        .header("Authorization", auth.clone())
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to fetch pioneer code detail")
        .json()
        .await
        .expect("failed to decode pioneer code detail");
    assert_eq!(detail["code"]["use_count"], 1);
    assert_eq!(detail["uses"][0]["agent_display_name"], "pioneer-agent");
    assert_eq!(detail["uses"][0]["registration_kind"], "agent_api");

    let revoked: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/pioneer-codes/{code_id}/revoke"),
        ))
        .header("Authorization", auth)
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to revoke pioneer code")
        .json()
        .await
        .expect("failed to decode revoke response");
    assert_eq!(revoked["status"], "revoked");
    assert_eq!(revoked["revoked_agent_count"], 1);
    assert_eq!(revoked["revoked_token_count"], 1);

    let disabled_agent = client
        .get(api_url(&app, "/api/agent/challenges"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to call agent route with revoked token");
    assert_eq!(disabled_agent.status(), 401);
}

/// Verifies finite pioneer-code use counts are enforced inside one database transaction.
#[sqlx::test(migrations = "../migrations")]
async fn finite_pioneer_code_consumption_is_atomic(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({ "code": "deadbeef", "max_uses": 1 }))
        .send()
        .await
        .expect("failed to create pioneer code");

    let first_client = client.clone();
    let first_url = api_url(&app, "/api/agents/register");
    let second_url = first_url.clone();
    let first = async move {
        first_client
            .post(first_url)
            .json(&serde_json::json!({
                "display_name": "racer-one",
                "pioneer_code": "deadbeef"
            }))
            .send()
            .await
            .expect("first registration should receive response")
            .status()
    };
    let second = async move {
        client
            .post(second_url)
            .json(&serde_json::json!({
                "display_name": "racer-two",
                "pioneer_code": "deadbeef"
            }))
            .send()
            .await
            .expect("second registration should receive response")
            .status()
    };

    let statuses = [first.await, second.await];
    assert!(statuses.contains(&StatusCode::CREATED));
    assert!(statuses.contains(&StatusCode::FORBIDDEN));
}

/// Verifies GitHub OAuth starts with a POST body so pioneer codes stay out of URLs.
#[sqlx::test(migrations = "../migrations")]
async fn github_oauth_login_start_uses_post_body(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({ "code": "deadbeef", "max_uses": 1 }))
        .send()
        .await
        .expect("failed to create pioneer code");

    let get_response = client
        .get(api_url(
            &app,
            "/api/auth/github/login?pioneer_code=deadbeef",
        ))
        .send()
        .await
        .expect("failed to call old OAuth start route");
    assert_eq!(get_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let post_response: serde_json::Value = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({ "pioneer_code": "deadbeef" }))
        .send()
        .await
        .expect("failed to start OAuth login")
        .json()
        .await
        .expect("failed to decode OAuth login response");
    let authorization_url = post_response["authorization_url"]
        .as_str()
        .expect("authorization_url should exist");
    assert!(authorization_url.starts_with("https://github.com/login/oauth/authorize"));
    assert!(!authorization_url.contains("pioneer_code"));
}

/// Verifies creator OAuth account creation uses the same code consumption primitive.
#[sqlx::test(migrations = "../migrations")]
async fn creator_oauth_creation_consumes_pioneer_code_once(pool: sqlx::PgPool) {
    let code = "team-deadbeef";
    let code_hash = shared::auth::hash_opaque_token(code);
    let code_id = AgentPioneerCodeId::generate();
    shared::db::create_pioneer_code(
        &pool,
        &shared::db::CreatePioneerCodeInput {
            id: code_id.clone(),
            code_display: code.to_string(),
            code_hash: code_hash.clone(),
            label: Some("team".to_string()),
            note: "creator oauth".to_string(),
            max_uses: 1,
            expires_at: None,
            created_by_admin_username: "admin".to_string(),
        },
    )
    .await
    .expect("pioneer code should insert");

    let first_agent_id = shared::models::ids::AgentId::generate();
    let stored_agent_id = shared::db::upsert_github_creator_agent_with_pioneer_code(
        &pool,
        &first_agent_id,
        42,
        "creator-login",
        Some(&code_hash),
        true,
        1_000,
    )
    .await
    .expect("first oauth login should create agent");
    assert_eq!(stored_agent_id, first_agent_id);

    let repeated_agent_id = shared::models::ids::AgentId::generate();
    let repeated = shared::db::upsert_github_creator_agent_with_pioneer_code(
        &pool,
        &repeated_agent_id,
        42,
        "creator-login-renamed",
        Some("not-a-valid-code-hash"),
        true,
        1_000,
    )
    .await
    .expect("repeat oauth login should not consume another code");
    assert_eq!(repeated, first_agent_id);

    let repeated_without_code = shared::db::upsert_github_creator_agent_with_pioneer_code(
        &pool,
        &shared::models::ids::AgentId::generate(),
        42,
        "creator-login-returned",
        None,
        true,
        1_000,
    )
    .await
    .expect("existing oauth creator should not need another pioneer code");
    assert_eq!(repeated_without_code, first_agent_id);

    let missing_code = shared::db::upsert_github_creator_agent_with_pioneer_code(
        &pool,
        &shared::models::ids::AgentId::generate(),
        43,
        "new-creator-without-code",
        None,
        true,
        1_000,
    )
    .await
    .expect_err("new creator must still provide a pioneer code");
    assert!(
        missing_code
            .to_string()
            .contains(shared::models::pioneer_codes::INVALID_OR_UNAVAILABLE_PIONEER_CODE)
    );

    let (detail, uses) = shared::db::get_pioneer_code_detail(&pool, &code_id)
        .await
        .expect("pioneer code detail should load");
    assert_eq!(detail.use_count, 1);
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].registration_kind, "creator_oauth");

    shared::db::disable_agent(&pool, first_agent_id.as_str())
        .await
        .expect("agent should disable");
    let disabled = shared::db::upsert_github_creator_agent_with_pioneer_code(
        &pool,
        &shared::models::ids::AgentId::generate(),
        42,
        "creator-login",
        Some(&code_hash),
        true,
        1_000,
    )
    .await
    .expect_err("disabled linked agent should block oauth login");
    assert!(
        disabled
            .to_string()
            .contains("linked GitHub creator agent is disabled")
    );
}
