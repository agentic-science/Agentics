use super::*;

fn temp_config_path(temp: &tempfile::TempDir) -> String {
    temp.path()
        .join("config.toml")
        .to_str()
        .expect("utf8 config path")
        .to_string()
}

/// Verifies creator review-record status supports JSON output.
#[tokio::test]
async fn creator_review_record_status_renders_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/creator/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        ))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(challenge_review_record_json("pending_review")),
        )
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "--json",
        "challenge-creator",
        "review-record",
        "status",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator status should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["id"], "dddddddd-dddd-4ddd-8ddd-dddddddddddd");
    assert_eq!(value["status"], "pending_review");
}

/// Verifies owner stats call the creator endpoint and render table output.
#[tokio::test]
async fn creator_stats_uses_creator_token_and_renders_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/stats"))
        .and(query_param("target", "linux-arm64-cpu"))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_stats_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "stats",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator stats should succeed");

    assert!(output.contains("challenge: sample-sum"));
    assert!(output.contains("solution_submissions: 5"));
    assert!(output.contains("primary_metric_max: 2.5"));
}

/// Verifies owner stats support JSON output.
#[tokio::test]
async fn creator_stats_renders_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/stats"))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_stats_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "--json",
        "challenge-creator",
        "stats",
        "sample-sum",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator stats should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["agent_count"], 2);
    assert_eq!(value["target"], "linux-arm64-cpu");
}

/// Verifies owner participants call the creator endpoint and render table output.
#[tokio::test]
async fn creator_participants_uses_creator_token_and_renders_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/participants"))
        .and(query_param("target", "linux-arm64-cpu"))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_participants_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "participants",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator participants should succeed");

    assert!(output.contains("challenge: sample-sum"));
    assert!(output.contains("solver"));
    assert!(output.contains("completed"));
}

/// Verifies participant output supports JSON mode.
#[tokio::test]
async fn creator_participants_renders_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/participants"))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_participants_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "--json",
        "challenge-creator",
        "participants",
        "sample-sum",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator participants should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["items"][0]["agent_display_name"], "solver");
}

/// Verifies shortlist show calls the creator endpoint and renders table output.
#[tokio::test]
async fn creator_shortlist_show_renders_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/shortlist"))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_shortlist_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "shortlist",
        "show",
        "sample-sum",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("shortlist show should succeed");

    assert!(output.contains("challenge: sample-sum"));
    assert!(output.contains("solver"));
}

/// Verifies shortlist show supports JSON mode.
#[tokio::test]
async fn creator_shortlist_show_renders_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/shortlist"))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_shortlist_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "--json",
        "challenge-creator",
        "shortlist",
        "show",
        "sample-sum",
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("shortlist show should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["items"][0]["agent_display_name"], "solver");
}

/// Verifies shortlist upload sends the file payload to the creator endpoint.
#[tokio::test]
async fn creator_shortlist_upload_sends_delta_file() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/creator/challenges/sample-sum/shortlist-revisions",
        ))
        .and(header("authorization", "Bearer creator-token"))
        .and(body_json(json!({
            "agent_ids_to_add": [
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
            ]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(challenge_shortlist_revision_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let shortlist_path = temp.path().join("shortlist.json");
    std::fs::write(
        &shortlist_path,
        json!({
            "agent_ids_to_add": [
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
            ]
        })
        .to_string(),
    )
    .expect("shortlist file");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "shortlist",
        "upload",
        "sample-sum",
        "--file",
        shortlist_path.to_str().expect("utf8 path"),
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("shortlist upload should succeed");

    assert!(output.contains("shortlist_revision: cccccccc-cccc-4ccc-8ccc-cccccccccccc"));
    assert!(output.contains("added_count: 1"));
}

/// Verifies shortlist upload supports JSON output.
#[tokio::test]
async fn creator_shortlist_upload_renders_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/creator/challenges/sample-sum/shortlist-revisions",
        ))
        .and(header("authorization", "Bearer creator-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(challenge_shortlist_revision_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let shortlist_path = temp.path().join("shortlist.json");
    std::fs::write(
        &shortlist_path,
        json!({ "agent_ids_to_add": ["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"] }).to_string(),
    )
    .expect("shortlist file");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "--json",
        "challenge-creator",
        "shortlist",
        "upload",
        "sample-sum",
        "--file",
        shortlist_path.to_str().expect("utf8 path"),
    ]);

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("shortlist upload should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["added_count"], 1);
}

/// Verifies invalid shortlist JSON is rejected before any HTTP request.
#[tokio::test]
async fn creator_shortlist_upload_rejects_invalid_json_before_http() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let shortlist_path = temp.path().join("shortlist.json");
    std::fs::write(&shortlist_path, "{ not json").expect("shortlist file");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "shortlist",
        "upload",
        "sample-sum",
        "--file",
        shortlist_path.to_str().expect("utf8 path"),
    ]);

    let error = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("creator-token")),
            ..Environment::default()
        },
    )
    .await
    .expect_err("invalid shortlist JSON should fail");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(requests.is_empty());
    assert!(
        error
            .to_string()
            .contains("shortlist file must be valid JSON")
    );
}

/// Verifies configured creator API tokens are used when env/stdin are absent.
#[tokio::test]
async fn creator_token_config_source_is_used() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/stats"))
        .and(header("authorization", "Bearer config-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_stats_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    std::fs::write(&config_path, "creator_api_token = \"config-token\"\n").expect("config");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "stats",
        "sample-sum",
    ]);

    execute(cli, Environment::default())
        .await
        .expect("creator stats should use config token");
}

/// Verifies env creator API tokens override config tokens.
#[tokio::test]
async fn creator_token_env_overrides_config_source() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/stats"))
        .and(header("authorization", "Bearer env-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_stats_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    std::fs::write(&config_path, "creator_api_token = \"config-token\"\n").expect("config");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "stats",
        "sample-sum",
    ]);

    execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("env-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator stats should use env token");
}

/// Verifies stdin creator API tokens override both env and config tokens.
#[tokio::test]
async fn creator_token_stdin_overrides_env_and_config_sources() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/creator/challenges/sample-sum/stats"))
        .and(header("authorization", "Bearer stdin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(creator_stats_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    std::fs::write(&config_path, "creator_api_token = \"config-token\"\n").expect("config");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "--creator-token-stdin",
        "stats",
        "sample-sum",
    ]);

    execute_with_input(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("env-token")),
            ..Environment::default()
        },
        CommandInput::test_stdin("stdin-token\n"),
    )
    .await
    .expect("creator stats should use stdin token");
}

/// Verifies missing creator tokens fail before any HTTP request.
#[tokio::test]
async fn creator_command_rejects_missing_token_before_http() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "challenge-creator",
        "stats",
        "sample-sum",
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("missing creator token should fail");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(requests.is_empty());
    assert!(error.to_string().contains("AGENTICS_CREATOR_API_TOKEN"));
}
