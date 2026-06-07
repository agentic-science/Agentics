//! Integration tests for human account deletion.

mod helpers;

use std::sync::Arc;

use agentics_config::AgentRegistrationMode;
use agentics_domain::models::auth::GithubUserId;
use agentics_domain::models::ids::{AdminServiceTokenId, CreatorApiTokenId, HumanId};
use agentics_error::Result;
use agentics_services::auth::{GithubSignInClient, GithubSignInUser};
use async_trait::async_trait;
use helpers::{
    api_url, create_creator_session, examples_challenges_root, spawn_app_with_config,
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

fn response_cookie_pair(response: &reqwest::Response, name: &str) -> String {
    let prefix = format!("{name}=");
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&prefix))
        .and_then(|value| value.split(';').next())
        .expect("response cookie should be set")
        .to_string()
}

fn cookie_pair_value(pair: &str) -> String {
    pair.split_once('=')
        .expect("cookie pair should include value")
        .1
        .to_string()
}

fn github_sign_in_state(authorization_url: &str) -> String {
    let url = url::Url::parse(authorization_url).expect("authorization URL should parse");
    url.query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
        .expect("authorization URL should include state")
}

/// Verifies account deletion deidentifies GitHub login and revokes owned credentials.
#[sqlx::test(migrations = "../migrations")]
async fn account_delete_revokes_human_access_and_blocks_re_registration(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.auth.agent_registration_mode = AgentRegistrationMode::Public;
    let app = spawn_app_with_config_and_github_client(
        pool.clone(),
        config,
        fake_github_client(71006, "delete-me"),
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
    assert_eq!(callback.status(), StatusCode::OK);
    let session_cookie = response_cookie_pair(&callback, "agentics_session");
    let csrf_cookie = response_cookie_pair(&callback, "agentics_csrf");
    let session_token = cookie_pair_value(&session_cookie);
    let csrf_token = cookie_pair_value(&csrf_cookie);
    let body: serde_json::Value = callback.json().await.expect("callback body should decode");
    let human_id = HumanId::try_new(
        body["session"]["human_id"]
            .as_str()
            .expect("human id should be present"),
    )
    .expect("human id should parse");

    sqlx::query(
        r#"
        UPDATE humans
        SET status = 'active',
            disabled_at = NULL,
            deleted_at = NULL
        WHERE id = $1::uuid
        "#,
    )
    .bind(human_id.as_str())
    .execute(&pool)
    .await
    .expect("human should activate");
    sqlx::query(
        r#"
        INSERT INTO human_roles (id, human_id, role)
        VALUES ($1::uuid, $2::uuid, 'creator')
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(human_id.as_str())
    .execute(&pool)
    .await
    .expect("creator role should insert");

    let repos = agentics_persistence::Repositories::new(&pool);
    let admin_token = agentics_services::auth::create_admin_service_token();
    let admin_token_hash = agentics_services::auth::hash_opaque_token(&admin_token);
    repos
        .sessions()
        .create_admin_service_token(&agentics_persistence::CreateAdminServiceTokenInput {
            id: AdminServiceTokenId::generate(),
            token_hash: admin_token_hash.clone(),
            label: "deleted-human-admin-token".to_string(),
            created_by_human_id: human_id.clone(),
            expires_at: None,
        })
        .await
        .expect("admin service token should insert");
    let creator_token = agentics_services::auth::create_creator_api_token();
    let creator_token_hash = agentics_services::auth::hash_opaque_token(&creator_token);
    repos
        .sessions()
        .create_creator_api_token(&agentics_persistence::CreateCreatorApiTokenInput {
            id: CreatorApiTokenId::generate(),
            token_hash: creator_token_hash.clone(),
            label: "deleted-human-creator-token".to_string(),
            created_by_human_id: human_id.clone(),
            expires_at: None,
        })
        .await
        .expect("creator API token should insert");

    let deletion = client
        .post(api_url(&app, "/api/auth/account/delete"))
        .header(header::COOKIE, format!("{session_cookie}; {csrf_cookie}"))
        .header("x-agentics-csrf-token", csrf_token)
        .send()
        .await
        .expect("failed to delete account");
    assert_eq!(deletion.status(), StatusCode::NO_CONTENT);
    let set_cookies = deletion
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>();
    assert!(
        set_cookies
            .iter()
            .any(|value| value.starts_with("agentics_session=") && value.contains("Max-Age=0"))
    );
    assert!(
        set_cookies
            .iter()
            .any(|value| value.starts_with("agentics_csrf=") && value.contains("Max-Age=0"))
    );

    let deleted_human = repos
        .sessions()
        .get_human_by_id(&human_id)
        .await
        .expect("deleted human should still be retained");
    assert_eq!(deleted_human.status, "deleted");
    assert!(deleted_human.deleted_at.is_some());
    assert!(deleted_human.github_login.starts_with("deleted-user-"));
    assert_eq!(deleted_human.github_user_id.as_i64(), 71006);
    assert!(
        repos
            .sessions()
            .authenticate_human(&session_token)
            .await
            .expect("session auth lookup should not fail")
            .is_none()
    );
    assert!(
        repos
            .sessions()
            .authenticate_admin_service_token(&admin_token_hash)
            .await
            .expect("admin token lookup should not fail")
            .is_none()
    );
    assert!(
        repos
            .sessions()
            .authenticate_creator_api_token(&creator_token_hash)
            .await
            .expect("creator token lookup should not fail")
            .is_none()
    );

    let repeated_login = client
        .post(api_url(&app, "/api/auth/github/login"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to start repeated GitHub sign-in");
    let repeated_nonce_cookie = github_sign_in_nonce_cookie(&repeated_login);
    let repeated_login_body: serde_json::Value = repeated_login
        .json()
        .await
        .expect("login body should decode");
    let repeated_state = github_sign_in_state(
        repeated_login_body["authorization_url"]
            .as_str()
            .expect("authorization URL should be present"),
    );
    let repeated_callback = client
        .post(api_url(&app, "/api/auth/github/callback"))
        .header(header::COOKIE, repeated_nonce_cookie)
        .json(&serde_json::json!({
            "code": "valid-github-code",
            "state": repeated_state
        }))
        .send()
        .await
        .expect("failed to complete repeated GitHub sign-in");
    assert_eq!(repeated_callback.status(), StatusCode::FORBIDDEN);
}

/// Verifies deleting the final active human admin is rejected.
#[sqlx::test(migrations = "../migrations")]
async fn account_delete_rejects_final_active_admin(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let admin = create_creator_session(&pool, 71007, "last-delete-admin").await;
    let admin_human_id =
        HumanId::try_new(admin.human_id.clone()).expect("test human id should parse");
    agentics_persistence::Repositories::new(&pool)
        .sessions()
        .grant_admin_role(&admin_human_id, &admin_human_id)
        .await
        .expect("test human admin role should grant");
    sqlx::query(
        r#"
        UPDATE humans
        SET status = 'disabled',
            disabled_at = NOW()
        WHERE id IN (
            SELECT human_id
            FROM human_external_identities
            WHERE provider = 'github'
              AND provider_user_id = 9001
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("integration bootstrap admin should disable");

    let response = reqwest::Client::new()
        .post(api_url(&app, "/api/auth/account/delete"))
        .header("Cookie", &admin.cookie_header)
        .header("X-Agentics-CSRF-Token", &admin.csrf_token)
        .send()
        .await
        .expect("failed to delete final admin");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let human = agentics_persistence::Repositories::new(&pool)
        .sessions()
        .get_human_by_id(&admin_human_id)
        .await
        .expect("admin human should still exist");
    assert_eq!(human.status, "active");
    assert!(human.deleted_at.is_none());
}
