//! Integration tests for pioneer-code gated agent registration.

mod helpers;

use std::sync::Arc;

use agentics_config::AgentRegistrationMode;
use agentics_domain::models::auth::GithubUserId;
use agentics_domain::models::ids::{HumanId, PioneerCodeId};
use agentics_error::Result;
use agentics_services::auth::{GithubSignInClient, GithubSignInUser};
use async_trait::async_trait;
use helpers::{
    admin_service_token_header, api_url, examples_challenges_root, spawn_app_with_config,
    spawn_app_with_config_and_github_client, test_config,
};
use reqwest::{StatusCode, header};
use secrecy::SecretString;

#[derive(Debug, Clone)]
struct FakeGithubSignInClient {
    user_id: GithubUserId,
    login: String,
}

#[async_trait]
impl GithubSignInClient for FakeGithubSignInClient {
    async fn exchange_code(
        &self,
        _config: &agentics_config::Config,
        code: &str,
    ) -> Result<SecretString> {
        if code.trim() == "valid-github-code" {
            return Ok(SecretString::from("fake-github-access-token"));
        }
        Err(agentics_error::ServiceError::BadRequest(
            "fake GitHub code rejected".to_string(),
        ))
    }

    async fn fetch_user(
        &self,
        _config: &agentics_config::Config,
        _access_token: &SecretString,
    ) -> Result<GithubSignInUser> {
        Ok(GithubSignInUser {
            id: self.user_id,
            login: self.login.clone(),
        })
    }
}

fn github_user_id(value: i64) -> GithubUserId {
    GithubUserId::try_new(value).expect("test GitHub user id should be positive")
}

fn fake_github_client(user_id: i64, login: &str) -> Arc<dyn GithubSignInClient> {
    Arc::new(FakeGithubSignInClient {
        user_id: github_user_id(user_id),
        login: login.to_string(),
    })
}

fn github_sign_in_nonce_cookie(response: &reqwest::Response) -> String {
    let cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("agentics_github_sign_in_nonce="))
        .expect("GitHub sign-in nonce cookie should be set");
    cookie
        .split(';')
        .next()
        .expect("cookie name and value should exist")
        .to_string()
}

fn github_sign_in_state(authorization_url: &str) -> String {
    let url = url::Url::parse(authorization_url).expect("authorization URL should parse");
    url.query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        .expect("authorization URL should include state")
}

/// Verifies default MVP registration mode rejects code-free registration and consumes finite codes.
#[sqlx::test(migrations = "../migrations")]
async fn pioneer_code_mode_gates_agent_registration(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
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

    let manual_code_create = client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth.clone())
        .json(&serde_json::json!({
            "code": "jack-deadbeef",
            "label": "jack",
            "max_uses": 1
        }))
        .send()
        .await
        .expect("failed to send manual pioneer code create request");
    assert_eq!(manual_code_create.status(), 400);

    let created: serde_json::Value = client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth.clone())
        .json(&serde_json::json!({
            "label": "jack",
            "note": "early private beta",
            "max_uses": 1
        }))
        .send()
        .await
        .expect("failed to create pioneer code")
        .json()
        .await
        .expect("failed to decode pioneer code");
    let code_display = created["code"]["code_display"]
        .as_str()
        .expect("generated code should exist")
        .to_string();
    assert!(
        code_display.starts_with("jack-"),
        "generated code should preserve the selected label"
    );
    assert_eq!(created["code"]["label"], "jack");
    assert_eq!(created["code"]["note"], "early private beta");
    assert_eq!(created["code"]["use_count"], 0);

    let registered: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "display_name": "pioneer-agent",
            "pioneer_code": code_display
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
            "pioneer_code": code_display
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
    assert_eq!(revoked["revoked_admin_service_token_count"], 0);
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
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let auth = admin_service_token_header(&app);

    let created: serde_json::Value = client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth)
        .json(&serde_json::json!({ "max_uses": 1 }))
        .send()
        .await
        .expect("failed to create pioneer code")
        .json()
        .await
        .expect("failed to decode pioneer code");
    let code_display = created["code"]["code_display"]
        .as_str()
        .expect("generated code should exist")
        .to_string();

    let first_client = client.clone();
    let first_url = api_url(&app, "/api/agents/register");
    let second_url = first_url.clone();
    let first_code = code_display.clone();
    let second_code = code_display.clone();
    let first = async move {
        first_client
            .post(first_url)
            .json(&serde_json::json!({
                "display_name": "racer-one",
                "pioneer_code": first_code
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
                "pioneer_code": second_code
            }))
            .send()
            .await
            .expect("second registration should receive response")
            .status()
    };

    let (first_status, second_status) = tokio::join!(first, second);
    let statuses = [first_status, second_status];
    assert!(statuses.contains(&StatusCode::CREATED));
    assert!(statuses.contains(&StatusCode::FORBIDDEN));
    let use_count =
        sqlx::query_scalar::<_, i64>("SELECT use_count FROM pioneer_codes WHERE code_display = $1")
            .bind(code_display)
            .fetch_one(&pool)
            .await
            .expect("pioneer code row should exist");
    assert_eq!(use_count, 1);
}

/// Verifies GitHub sign-in starts with a POST body so pioneer codes stay out of URLs.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_login_start_uses_post_body(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();
    let auth = admin_service_token_header(&app);

    let created: serde_json::Value = client
        .post(api_url(&app, "/admin/pioneer-codes"))
        .header("Authorization", auth)
        .json(&serde_json::json!({ "max_uses": 1 }))
        .send()
        .await
        .expect("failed to create pioneer code")
        .json()
        .await
        .expect("failed to decode pioneer code");
    let code_display = created["code"]["code_display"]
        .as_str()
        .expect("generated code should exist")
        .to_string();

    let get_response = client
        .get(api_url(
            &app,
            &format!("/api/auth/github/login?pioneer_code={code_display}"),
        ))
        .send()
        .await
        .expect("failed to call unsupported GET login route");
    assert_eq!(get_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let post_response = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({ "pioneer_code": code_display }))
        .send()
        .await
        .expect("failed to start GitHub sign-in");
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
            .any(|value| value.starts_with("agentics_github_sign_in_nonce=")
                && value.contains("HttpOnly")
                && value.contains("SameSite=Lax")),
        "GitHub sign-in start should bind state to an HttpOnly browser nonce cookie"
    );
    let post_response: serde_json::Value = post_response
        .json()
        .await
        .expect("failed to decode GitHub sign-in response");
    let authorization_url = post_response["authorization_url"]
        .as_str()
        .expect("authorization_url should exist");
    assert!(authorization_url.starts_with("https://github.com/login/oauth/authorize"));
    assert!(!authorization_url.contains("pioneer_code"));
    assert!(
        !authorization_url.contains("scope="),
        "GitHub App login-only sign-in should not request repository scopes"
    );
    assert!(
        post_response.get("state").is_none(),
        "raw GitHub sign-in state stays inside the authorization URL"
    );
}

/// Verifies GitHub sign-in callback state cannot be consumed without the initiating browser nonce.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_state_requires_browser_nonce(pool: sqlx::PgPool) {
    let state = "github-sign-in-state";
    let nonce = "github-sign-in-browser-nonce";
    let repos = agentics_persistence::Repositories::new(&pool);
    repos
        .sessions()
        .create_github_sign_in_state(&agentics_persistence::CreateGithubSignInStateInput {
            state_hash: agentics_services::auth::hash_opaque_token(state),
            browser_nonce_hash: agentics_services::auth::hash_opaque_token(nonce),
            pioneer_code_hash: None,
            return_to: None,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        })
        .await
        .expect("GitHub sign-in state should insert");

    let wrong_nonce = repos
        .sessions()
        .consume_github_sign_in_state(
            &agentics_services::auth::hash_opaque_token(state),
            &agentics_services::auth::hash_opaque_token("wrong-browser-nonce"),
        )
        .await
        .expect("wrong nonce lookup should not fail");
    assert!(wrong_nonce.is_none());

    let consumed = repos
        .sessions()
        .consume_github_sign_in_state(
            &agentics_services::auth::hash_opaque_token(state),
            &agentics_services::auth::hash_opaque_token(nonce),
        )
        .await
        .expect("matching nonce should consume state");
    assert!(consumed.is_some());
}

/// Verifies the real GitHub callback route issues a human session with test GitHub IO.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_callback_route_issues_session(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::Public;
    let app = spawn_app_with_config_and_github_client(
        pool.clone(),
        config,
        fake_github_client(71001, "callback-creator"),
    )
    .await;
    let client = reqwest::Client::new();

    let login = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({ "return_to": "/creator" }))
        .send()
        .await
        .expect("failed to start GitHub sign-in");
    assert_eq!(login.status(), StatusCode::OK);
    let nonce_cookie = github_sign_in_nonce_cookie(&login);
    let login_body: serde_json::Value = login.json().await.expect("login body should decode");
    let state = github_sign_in_state(
        login_body["authorization_url"]
            .as_str()
            .expect("authorization URL should be present"),
    );

    let callback = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .header(header::COOKIE, nonce_cookie)
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": state
        }))
        .send()
        .await
        .expect("failed to complete GitHub sign-in");
    assert_eq!(callback.status(), StatusCode::OK);
    let set_cookies = callback
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>();
    assert!(
        set_cookies
            .iter()
            .any(|value| value.starts_with("agentics_session="))
    );
    assert!(
        set_cookies
            .iter()
            .any(|value| value.starts_with("agentics_csrf="))
    );
    assert!(set_cookies.iter().any(|value| {
        value.starts_with("agentics_github_sign_in_nonce=") && value.contains("Max-Age=0")
    }));
    let body: serde_json::Value = callback.json().await.expect("callback body should decode");
    assert_eq!(body["return_to"], "/creator");
    assert_eq!(body["session"]["github_user_id"], 71001);
    assert_eq!(body["session"]["github_login"], "callback-creator");
}

/// Verifies the GitHub callback route rejects callbacks without the browser nonce cookie.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_callback_route_requires_nonce_cookie(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::Public;
    let app = spawn_app_with_config_and_github_client(
        pool.clone(),
        config,
        fake_github_client(71002, "missing-nonce"),
    )
    .await;
    let client = reqwest::Client::new();
    let login = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to start GitHub sign-in");
    let login_body: serde_json::Value = login.json().await.expect("login body should decode");
    let state = github_sign_in_state(
        login_body["authorization_url"]
            .as_str()
            .expect("authorization URL should be present"),
    );

    let callback = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": state
        }))
        .send()
        .await
        .expect("failed to call callback route");
    assert_eq!(callback.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies a callback state cannot be reused after one successful route callback.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_callback_route_consumes_state_once(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::Public;
    let app = spawn_app_with_config_and_github_client(
        pool.clone(),
        config,
        fake_github_client(71003, "state-reuse"),
    )
    .await;
    let client = reqwest::Client::new();
    let login = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to start GitHub sign-in");
    let nonce_cookie = github_sign_in_nonce_cookie(&login);
    let login_body: serde_json::Value = login.json().await.expect("login body should decode");
    let state = github_sign_in_state(
        login_body["authorization_url"]
            .as_str()
            .expect("authorization URL should be present"),
    );

    let first = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .header(header::COOKIE, nonce_cookie.clone())
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": state.clone()
        }))
        .send()
        .await
        .expect("first callback should complete");
    assert_eq!(first.status(), StatusCode::OK);

    let second = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .header(header::COOKIE, nonce_cookie)
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": state
        }))
        .send()
        .await
        .expect("second callback should return response");
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies the callback route rejects disabled linked humans.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_callback_route_rejects_disabled_human(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::Public;
    let repos = agentics_persistence::Repositories::new(&pool);
    let human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(71004),
            github_login: "disabled-callback".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: false,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("human should resolve");
    sqlx::query("UPDATE humans SET status = 'disabled', disabled_at = NOW() WHERE id = $1::uuid")
        .bind(human.human_id.as_str())
        .execute(&pool)
        .await
        .expect("human should disable");
    let app = spawn_app_with_config_and_github_client(
        pool.clone(),
        config,
        fake_github_client(71004, "disabled-callback"),
    )
    .await;
    let client = reqwest::Client::new();
    let login = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to start GitHub sign-in");
    let nonce_cookie = github_sign_in_nonce_cookie(&login);
    let login_body: serde_json::Value = login.json().await.expect("login body should decode");
    let state = github_sign_in_state(
        login_body["authorization_url"]
            .as_str()
            .expect("authorization URL should be present"),
    );

    let callback = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .header(header::COOKIE, nonce_cookie)
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": state
        }))
        .send()
        .await
        .expect("failed to complete GitHub sign-in");
    assert_eq!(callback.status(), StatusCode::FORBIDDEN);
}

/// Verifies new humans still need a pioneer code in gated mode.
#[sqlx::test(migrations = "../migrations")]
async fn github_sign_in_callback_route_requires_pioneer_code_for_new_human(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::PioneerCode;
    let app = spawn_app_with_config_and_github_client(
        pool.clone(),
        config,
        fake_github_client(71005, "new-human-without-code"),
    )
    .await;
    let client = reqwest::Client::new();
    let login = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to start GitHub sign-in");
    let nonce_cookie = github_sign_in_nonce_cookie(&login);
    let login_body: serde_json::Value = login.json().await.expect("login body should decode");
    let state = github_sign_in_state(
        login_body["authorization_url"]
            .as_str()
            .expect("authorization URL should be present"),
    );

    let callback = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .header(header::COOKIE, nonce_cookie)
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": state
        }))
        .send()
        .await
        .expect("failed to complete GitHub sign-in");
    assert_eq!(callback.status(), StatusCode::FORBIDDEN);
}

/// Verifies human GitHub sign-in account creation uses the same code consumption primitive.
#[sqlx::test(migrations = "../migrations")]
async fn human_github_sign_in_creation_consumes_pioneer_code_once(pool: sqlx::PgPool) {
    let code = "team-deadbeef";
    let code_hash = agentics_services::auth::hash_opaque_token(code);
    let code_id = PioneerCodeId::generate();
    let repos = agentics_persistence::Repositories::new(&pool);
    let admin_human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(9001),
            github_login: "integration-admin".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: false,
            bootstrap_admin_candidate: true,
        })
        .await
        .expect("admin human should resolve");
    repos
        .pioneer_codes()
        .create(&agentics_persistence::CreatePioneerCodeInput {
            id: code_id.clone(),
            code_display: code.to_string(),
            code_hash: code_hash.clone(),
            label: Some("team".to_string()),
            note: "human github sign-in".to_string(),
            max_uses: 1,
            expires_at: None,
            created_by_human_id: Some(admin_human.human_id.clone()),
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
            github_user_id: github_user_id(42),
            github_login: "creator-login".to_string(),
            pioneer_code_hash: Some(code_hash.clone()),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("first GitHub sign-in should create human");
    assert_eq!(stored_human.human_id, first_human_id);

    let repeated = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(42),
            github_login: "creator-login-renamed".to_string(),
            pioneer_code_hash: Some("not-a-valid-code-hash".to_string()),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("repeat GitHub sign-in should not consume another code");
    assert_eq!(repeated.human_id, first_human_id);

    let repeated_without_code = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(42),
            github_login: "creator-login-returned".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("existing GitHub sign-in creator should not need another pioneer code");
    assert_eq!(repeated_without_code.human_id, first_human_id);

    let missing_code = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(43),
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
    assert_eq!(uses[0].registration_kind, "human_github_sign_in");
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
            github_user_id: github_user_id(42),
            github_login: "creator-login".to_string(),
            pioneer_code_hash: Some(code_hash),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect_err("disabled linked human should block GitHub sign-in");
    assert!(
        disabled
            .to_string()
            .contains("linked human account is disabled")
    );
}

/// Verifies revoking a pioneer code also revokes admin service tokens created by derived humans.
#[sqlx::test(migrations = "../migrations")]
async fn pioneer_code_revoke_revokes_derived_human_admin_service_tokens(pool: sqlx::PgPool) {
    let code = "team-deadbeef";
    let code_hash = agentics_services::auth::hash_opaque_token(code);
    let code_id = PioneerCodeId::generate();
    let repos = agentics_persistence::Repositories::new(&pool);
    let admin_human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(9001),
            github_login: "integration-admin".to_string(),
            pioneer_code_hash: None,
            pioneer_code_required_for_new_human: false,
            bootstrap_admin_candidate: true,
        })
        .await
        .expect("admin human should resolve");
    repos
        .pioneer_codes()
        .create(&agentics_persistence::CreatePioneerCodeInput {
            id: code_id.clone(),
            code_display: code.to_string(),
            code_hash: code_hash.clone(),
            label: Some("team".to_string()),
            note: "human github sign-in".to_string(),
            max_uses: 1,
            expires_at: None,
            created_by_human_id: Some(admin_human.human_id.clone()),
            created_by_admin_service_token_id: None,
            created_by_display: "integration-admin".to_string(),
        })
        .await
        .expect("pioneer code should insert");
    let invited_human = repos
        .sessions()
        .resolve_github_human(&agentics_persistence::ResolveGithubHumanInput {
            fallback_human_id: HumanId::generate(),
            github_user_id: github_user_id(42),
            github_login: "creator-login".to_string(),
            pioneer_code_hash: Some(code_hash),
            pioneer_code_required_for_new_human: true,
            bootstrap_admin_candidate: false,
        })
        .await
        .expect("GitHub sign-in should create human");
    let admin_token = agentics_services::auth::create_admin_service_token();
    let token_hash = agentics_services::auth::hash_opaque_token(&admin_token);
    repos
        .sessions()
        .create_admin_service_token(&agentics_persistence::CreateAdminServiceTokenInput {
            id: agentics_domain::models::ids::AdminServiceTokenId::generate(),
            token_hash: token_hash.clone(),
            label: "derived-human-token".to_string(),
            created_by_human_id: invited_human.human_id.clone(),
            expires_at: None,
        })
        .await
        .expect("admin service token should insert");

    let outcome = repos
        .pioneer_codes()
        .revoke(&code_id)
        .await
        .expect("pioneer code should revoke");

    assert_eq!(outcome.revoked_human_count, 1);
    assert_eq!(outcome.revoked_admin_service_token_count, 1);
    assert!(
        repos
            .sessions()
            .authenticate_admin_service_token(&token_hash)
            .await
            .expect("token auth lookup should not fail")
            .is_none()
    );
}
