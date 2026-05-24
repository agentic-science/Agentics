use clap::Parser;
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::cli::Cli;
use crate::config::{CliConfig, ConfigStore, Environment};
use crate::execute;

/// Verifies that register persists returned token.
#[tokio::test]
async fn register_persists_returned_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/agents/register"))
        .and(body_json(json!({
            "display_name": "solver",
            "pioneer_code": "deadbeef",
            "agent_description": "",
            "owner": "",
            "model_info": {}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "token": "agentics_token",
            "display_name": "solver",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "register",
        "--display-name",
        "solver",
        "--pioneer-code",
        "deadbeef",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("register should succeed");
    let saved = ConfigStore::new(config_path)
        .load()
        .expect("config should load");

    assert!(output.contains("Registered agent solver"));
    assert!(!output.contains("deadbeef"));
    assert!(!output.contains("agentics_token"));
    assert_eq!(
        saved,
        CliConfig {
            api_base_url: Some(server.uri()),
            token: Some("agentics_token".to_string()),
        }
    );
}

/// Verifies that malformed success bodies do not leak one-time registration tokens.
#[tokio::test]
async fn register_decode_error_redacts_success_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/agents/register"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_string(r#"{"token":"agentics_secret_token","agent_id":"#),
        )
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "register",
        "--display-name",
        "solver",
        "--pioneer-code",
        "deadbeef",
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("malformed success JSON should fail");
    let message = format!("{error:#}");

    assert!(!message.contains("agentics_secret_token"));
    assert!(message.contains("body_bytes="));
}

/// Verifies that register can omit the pioneer code when the API allows public registration.
#[tokio::test]
async fn register_can_omit_pioneer_code_for_public_registration() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/agents/register"))
        .and(body_json(json!({
            "display_name": "solver",
            "agent_description": "",
            "owner": "",
            "model_info": {}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "agent_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "token": "agentics_token",
            "display_name": "solver",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "register",
        "--display-name",
        "solver",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("register should succeed without a local pioneer-code preflight");
    assert!(output.contains("Registered agent solver"));
}

/// Verifies that AGENTICS_PIONEER_CODE supplies the registration code.
#[tokio::test]
async fn register_uses_pioneer_code_from_environment() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/agents/register"))
        .and(body_json(json!({
            "display_name": "solver",
            "pioneer_code": "cafebabe",
            "agent_description": "",
            "owner": "",
            "model_info": {}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "agent_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "token": "agentics_token",
            "display_name": "solver",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "register",
        "--display-name",
        "solver",
        "--print-token",
    ]);
    let env = Environment {
        pioneer_code: Some(SecretString::from("cafebabe")),
        ..Environment::default()
    };

    let output = execute(cli, env).await.expect("register should succeed");
    assert!(output.contains("agentics_token"));
    assert!(!output.contains("cafebabe"));
    let saved = ConfigStore::new(config_path)
        .load()
        .expect("config should load");
    assert_eq!(
        saved,
        CliConfig {
            api_base_url: None,
            token: None,
        }
    );
}

/// Verifies that JSON registration output redacts the token unless explicitly requested.
#[tokio::test]
async fn register_json_redacts_saved_token_by_default() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/agents/register"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "agent_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "token": "agentics_token",
            "display_name": "solver",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--json",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "register",
        "--display-name",
        "solver",
        "--pioneer-code",
        "deadbeef",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("register should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["saved_token"], true);
    assert!(value.get("token").is_none());
    assert!(!output.contains("agentics_token"));
}

/// Verifies that invalid GitHub PR numbers fail during CLI parsing.
#[test]
fn invalid_pr_number_fails_during_cli_parsing() {
    let result = Cli::try_parse_from([
        "agentics",
        "challenge-creator",
        "draft",
        "create",
        "--repo-url",
        "git@github.com:agentics-reifying/agentics-challenges.git",
        "--pr-number",
        "0",
        "--pr-url",
        "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "--commit-sha",
        "0123456789012345678901234567890123456789",
        "--challenge-path",
        "challenges/matrix",
        "--pr-author-github-user-id",
        "1",
    ]);

    assert!(result.is_err());
}
