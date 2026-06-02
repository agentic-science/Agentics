//! Integration tests for pioneer-code gated agent registration.

mod helpers;

use agentics_config::AgentRegistrationMode;
use agentics_domain::models::ids::{HumanId, PioneerCodeId};
use helpers::{
    admin_service_token_header, api_url, examples_challenges_root, spawn_app_with_config,
    test_config,
};
use reqwest::StatusCode;

/// Verifies default MVP registration mode rejects code-free registration and consumes finite codes.
#[sqlx::test(migrations = "../migrations")]
async fn pioneer_code_mode_gates_agent_registration(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = admin_service_token_header(&app);

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
    config.auth.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = admin_service_token_header(&app);

    client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth)
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
    config.auth.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = admin_service_token_header(&app);

    client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth)
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

    let post_response = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({ "pioneer_code": "deadbeef" }))
        .send()
        .await
        .expect("failed to start OAuth login");
    assert_eq!(post_response.status(), StatusCode::OK);
    let set_cookie = post_response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().expect("set-cookie should be valid"))
        .collect::<Vec<_>>();
    assert!(
        set_cookie
            .iter()
            .any(|value| value.starts_with("agentics_oauth_nonce=")
                && value.contains("HttpOnly")
                && value.contains("SameSite=Lax")),
        "OAuth start should bind state to an HttpOnly browser nonce cookie"
    );
    let post_response: serde_json::Value = post_response
        .json()
        .await
        .expect("failed to decode OAuth login response");
    let authorization_url = post_response["authorization_url"]
        .as_str()
        .expect("authorization_url should exist");
    assert!(authorization_url.starts_with("https://github.com/login/oauth/authorize"));
    assert!(!authorization_url.contains("pioneer_code"));
    assert!(
        post_response.get("state").is_none(),
        "raw OAuth state stays inside the authorization URL"
    );
}

/// Verifies OAuth callback state cannot be consumed without the initiating browser nonce.
#[sqlx::test(migrations = "../migrations")]
async fn github_oauth_state_requires_browser_nonce(pool: sqlx::PgPool) {
    let state = "oauth-state";
    let nonce = "oauth-browser-nonce";
    let repos = agentics_persistence::Repositories::new(&pool);
    repos
        .sessions()
        .create_github_oauth_state(&agentics_persistence::CreateGithubOauthStateInput {
            state_hash: agentics_services::auth::hash_opaque_token(state),
            browser_nonce_hash: agentics_services::auth::hash_opaque_token(nonce),
            pioneer_code_hash: None,
            return_to: None,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        })
        .await
        .expect("OAuth state should insert");

    let wrong_nonce = repos
        .sessions()
        .consume_github_oauth_state(
            &agentics_services::auth::hash_opaque_token(state),
            &agentics_services::auth::hash_opaque_token("wrong-browser-nonce"),
        )
        .await
        .expect("wrong nonce lookup should not fail");
    assert!(wrong_nonce.is_none());

    let consumed = repos
        .sessions()
        .consume_github_oauth_state(
            &agentics_services::auth::hash_opaque_token(state),
            &agentics_services::auth::hash_opaque_token(nonce),
        )
        .await
        .expect("matching nonce should consume state");
    assert!(consumed.is_some());
}

/// Verifies human OAuth account creation uses the same code consumption primitive.
#[sqlx::test(migrations = "../migrations")]
async fn human_oauth_creation_consumes_pioneer_code_once(pool: sqlx::PgPool) {
    let code = "team-deadbeef";
    let code_hash = agentics_services::auth::hash_opaque_token(code);
    let code_id = PioneerCodeId::generate();
    let repos = agentics_persistence::Repositories::new(&pool);
    repos
        .pioneer_codes()
        .create(&agentics_persistence::CreatePioneerCodeInput {
            id: code_id.clone(),
            code_display: code.to_string(),
            code_hash: code_hash.clone(),
            label: Some("team".to_string()),
            note: "human oauth".to_string(),
            max_uses: 1,
            expires_at: None,
            created_by_human_id: None,
            created_by_admin_service_token_id: None,
            created_by_display: "integration-admin".to_string(),
        })
        .await
        .expect("pioneer code should insert");

    let first_human_id = HumanId::generate();
    let stored_human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: first_human_id.clone(),
            github_user_id: 42,
            github_login: "creator-login".to_string(),
            pioneer_code_hash: Some(code_hash.clone()),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("first oauth login should create human");
    assert_eq!(stored_human.human_id, first_human_id);

    let repeated = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: 42,
            github_login: "creator-login-renamed".to_string(),
            pioneer_code_hash: Some("not-a-valid-code-hash".to_string()),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("repeat oauth login should not consume another code");
    assert_eq!(repeated.human_id, first_human_id);

    let repeated_without_code = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: 42,
            github_login: "creator-login-returned".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("existing oauth creator should not need another pioneer code");
    assert_eq!(repeated_without_code.human_id, first_human_id);

    let missing_code = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: 43,
            github_login: "new-creator-without-code".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect_err("new creator must still provide a pioneer code");
    assert!(
        missing_code
            .to_string()
            .contains(agentics_domain::models::pioneer_codes::INVALID_OR_UNAVAILABLE_PIONEER_CODE)
    );

    let (detail, uses) = repos
        .pioneer_codes()
        .detail(&code_id)
        .await
        .expect("pioneer code detail should load");
    assert_eq!(detail.use_count, 1);
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].registration_kind, "human_github_oauth");
    assert_eq!(uses[0].human_id.as_ref(), Some(&first_human_id));

    sqlx::query("UPDATE humans SET status = 'disabled', disabled_at = NOW() WHERE id = $1::uuid")
        .bind(first_human_id.as_str())
        .execute(&pool)
        .await
        .expect("human should disable");
    let disabled = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: 42,
            github_login: "creator-login".to_string(),
            pioneer_code_hash: Some(code_hash),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect_err("disabled linked human should block oauth login");
    assert!(
        disabled
            .to_string()
            .contains("linked human account is disabled")
    );
}
