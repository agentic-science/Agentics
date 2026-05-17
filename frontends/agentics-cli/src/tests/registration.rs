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

/// Verifies that register refuses to send a request without a pioneer code.
#[tokio::test]
async fn register_requires_pioneer_code_before_http() {
    let server = MockServer::start().await;
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

    let error = execute(cli, Environment::default())
        .await
        .expect_err("register should fail before HTTP without a pioneer code");
    assert!(
        error
            .to_string()
            .contains("agent registration requires --pioneer-code")
    );
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
        "--no-save-token",
    ]);
    let env = Environment {
        pioneer_code: Some(SecretString::from("cafebabe")),
        ..Environment::default()
    };

    let output = execute(cli, env).await.expect("register should succeed");
    assert!(output.contains("agentics_token"));
    assert!(!output.contains("cafebabe"));
}
