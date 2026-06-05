use super::*;

/// Verifies that the pre-MVP CLI rejects the old output-format flag.
#[test]
fn old_output_json_flag_is_removed() {
    let result = Cli::try_parse_from(["agentics", "--output", "json", "challenges", "list"]);

    assert!(result.is_err());
}

/// Verifies that old status command is removed.
#[test]
fn old_status_command_is_removed() {
    let result = Cli::try_parse_from(["agentics", "status", "submission-1"]);

    assert!(result.is_err());
}

/// Verifies that invalid submit target fails during cli parse.
#[test]
fn invalid_submit_target_fails_during_cli_parse() {
    let result = Cli::try_parse_from([
        "agentics",
        "submit",
        "sample-sum",
        "--target",
        "linux arm64",
    ]);

    assert!(result.is_err());
}

/// Verifies that invalid submission id fails during cli parse.
#[test]
fn invalid_submission_id_fails_during_cli_parse() {
    let result = Cli::try_parse_from(["agentics", "submissions", "show", "submission-1"]);

    assert!(result.is_err());
}

/// Verifies that admin challenge-review-record commands parse review record IDs before HTTP execution.
#[test]
fn invalid_challenge_review_record_id_fails_during_cli_parse() {
    let result = Cli::try_parse_from([
        "agentics",
        "admin",
        "review-record",
        "validate",
        "review-record-1",
        "--repository-path",
        ".",
        "--admin-service-token-stdin",
    ]);

    assert!(result.is_err());
}

/// Verifies admin service tokens cannot be supplied through process argv.
#[test]
fn admin_service_token_argv_flag_is_removed() {
    let result = Cli::try_parse_from([
        "agentics",
        "admin",
        "review-record",
        "cleanup",
        "--admin-service-token",
        "agentics_admin_secret",
    ]);

    assert!(result.is_err());
}

/// Verifies long-lived tokens are not accepted as config-set argv values.
#[tokio::test]
async fn config_set_rejects_secret_values_in_argv() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "config",
        "set",
        "creator-api-token",
        "agentics_creator_secret",
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("secret positional config value should fail");

    assert!(error.to_string().contains("pass it with --stdin"));
}

/// Verifies secret config stdin trims newlines and redacts command output.
#[tokio::test]
async fn config_set_creator_token_reads_stdin_without_echoing_secret() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "config",
        "set",
        "creator-api-token",
        "--stdin",
    ]);

    let output = execute_with_input(
        cli,
        Environment::default(),
        CommandInput::test_stdin("agentics_creator_secret\n"),
    )
    .await
    .expect("config set should read creator token from stdin");
    let raw_config = std::fs::read_to_string(&config_path).expect("config should be written");

    assert!(output.contains("updated: creator_api_token"));
    assert!(!output.contains("agentics_creator_secret"));
    assert!(raw_config.contains("creator_api_token = \"agentics_creator_secret\""));
}

/// Verifies config set token also requires stdin and trims trailing newlines.
#[tokio::test]
async fn config_set_agent_token_reads_stdin() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "config",
        "set",
        "token",
        "--stdin",
    ]);

    execute_with_input(
        cli,
        Environment::default(),
        CommandInput::test_stdin("agentics_agent_secret\r\n"),
    )
    .await
    .expect("config set should read token from stdin");
    let raw_config = std::fs::read_to_string(&config_path).expect("config should be written");

    assert!(raw_config.contains("token = \"agentics_agent_secret\""));
}
