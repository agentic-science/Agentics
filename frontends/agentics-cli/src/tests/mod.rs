use std::path::Path;

use clap::Parser;
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::cli::Cli;
use crate::config::Environment;
use crate::{CommandInput, execute, execute_with_input};

/// Writes solution manifest to the target path.
fn write_solution_manifest(workspace_dir: &Path) {
    std::fs::write(
        workspace_dir.join("agentics.solution.json"),
        json!({
            "protocol": "zip_project",
            "protocol_version": 1,
            "note": "",
            "commands": { "run": "run.sh" }
        })
        .to_string(),
    )
    .expect("manifest");
}

/// Verifies that init solution fetches challenge and creates workspace.
#[tokio::test]
async fn init_solution_fetches_challenge_and_creates_workspace() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let workspace_dir = temp.path().join("solution");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "init-solution",
        "sample-sum",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("init-solution should succeed");

    assert!(output.contains("Initialized solution workspace"));
    assert!(workspace_dir.join("README.md").is_file());
    assert!(workspace_dir.join("agentics.solution.json").is_file());
    assert!(workspace_dir.join("scripts/setup.sh").is_file());
    assert!(workspace_dir.join("scripts/build.sh").is_file());
    assert!(workspace_dir.join(".git/hooks/pre-commit").is_file());
    assert!(!workspace_dir.join("run.sh").exists());
}

/// Verifies that submit packages workspace and posts authenticated request.
#[tokio::test]
async fn submit_packages_workspace_and_posts_authenticated_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/agent/solution-submissions"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "status": "queued",
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_key": "solution-submissions/11111111-1111-4111-8111-111111111111.zip",
            "note": "",
            "evaluation_job_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir(&workspace_dir).expect("workspace dir");
    write_solution_manifest(&workspace_dir);
    std::fs::write(
        workspace_dir.join("run.sh"),
        "#!/usr/bin/env bash\npython main.py\n",
    )
    .expect("run.sh");
    std::fs::write(workspace_dir.join("main.py"), "print('ok')\n").expect("main.py");
    std::fs::write(workspace_dir.join("ignored.txt"), "ignored").expect("ignored");
    std::fs::write(workspace_dir.join(".gitignore"), "ignored.txt\n").expect("gitignore");

    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "--token",
        "test-token",
        "submit",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
        "--explanation",
        "first attempt",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submit should succeed");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");
    let post = requests
        .iter()
        .find(|request| request.url.path() == "/api/agent/solution-submissions")
        .expect("solution submission create request should be recorded");
    let body: serde_json::Value =
        serde_json::from_slice(&post.body).expect("request body should be JSON");

    assert!(output.contains("Submitted 11111111-1111-4111-8111-111111111111"));
    assert_eq!(body["challenge_name"], "sample-sum");
    assert_eq!(body["target"], "linux-arm64-cpu");
    assert_eq!(body["explanation"], "first attempt");
    assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
}

/// Verifies that submit rejects an over-limit manifest note before upload.
#[tokio::test]
async fn submit_rejects_over_limit_manifest_note_before_upload() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir(&workspace_dir).expect("workspace dir");
    std::fs::write(
        workspace_dir.join("agentics.solution.json"),
        json!({
            "protocol": "zip_project",
            "protocol_version": 1,
            "note": "a".repeat(1025),
            "commands": { "run": "run.sh" }
        })
        .to_string(),
    )
    .expect("manifest");
    std::fs::write(workspace_dir.join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");

    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "--token",
        "test-token",
        "submit",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("over-limit note should fail before upload");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(error.to_string().contains("note must be at most 1024"));
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/api/agent/solution-submissions")
    );
}

/// Verifies that submit rejects unknown target before packaging.
#[tokio::test]
async fn submit_rejects_unknown_target_before_packaging() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir(&workspace_dir).expect("workspace dir");

    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "--token",
        "test-token",
        "submit",
        "sample-sum",
        "--target",
        "cpu-linux-ppc64le",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("unknown target should be rejected before packaging");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(error.to_string().contains("target"));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/api/public/challenges/sample-sum");
}

/// Verifies that submissions show fetches public solution submission details.
#[tokio::test]
async fn submissions_show_fetches_public_solution_submission() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "target": "linux-arm64-cpu",
            "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "agent_display_name": "solver",
            "status": "queued",
            "note": "",
            "explanation": "",
            "credit_text": "",
            "visible_after_eval": false,
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-01T00:00:00Z"
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
        "submissions",
        "show",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions show should succeed");

    assert!(output.contains("solution submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("status: queued"));
}

/// Verifies that submissions status fetches authenticated validation run id.
#[tokio::test]
async fn submissions_status_fetches_validation_run_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/22222222-2222-4222-8222-222222222222",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "22222222-2222-4222-8222-222222222222",
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "target": "linux-arm64-cpu",
            "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "agent_display_name": "solver",
            "status": "completed",
            "note": "Remote smoke",
            "explanation": "quick check",
            "credit_text": "",
            "visible_after_eval": false,
            "artifact_key": "solution-submissions/22222222-2222-4222-8222-222222222222.zip",
            "evaluation_job": {
                "id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "target": "linux-arm64-cpu",
                "status": "completed"
            },
            "evaluation": {
                "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                "target": "linux-arm64-cpu",
                "status": "completed",
                "eval_type": "validation",
                "rank_score": 1.0,
                "aggregate_metrics": [],
                "run_metrics": [],
                "public_results": []
            },
            "validation_evaluation": {
                "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                "target": "linux-arm64-cpu",
                "status": "completed",
                "eval_type": "validation",
                "rank_score": 1.0,
                "aggregate_metrics": [],
                "run_metrics": [],
                "public_results": []
            },
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-01T00:00:01Z"
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
        "--token",
        "test-token",
        "submissions",
        "status",
        "22222222-2222-4222-8222-222222222222",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions status should read validation ids through solution submissions");

    assert!(output.contains("solution submission: 22222222-2222-4222-8222-222222222222"));
    assert!(output.contains("validation_evaluation: completed"));
}

/// Verifies that submissions list uses the public target-scoped API.
#[tokio::test]
async fn submissions_list_uses_public_target_api_with_default_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/challenges/sample-sum/solution-submissions",
        ))
        .and(query_param("target", "linux-arm64-cpu"))
        .and(query_param("limit", "20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(public_submission_list_json()))
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
        "submissions",
        "list",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions list should succeed");

    assert!(output.contains("total_visible: 3"));
    assert!(output.contains("11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("solver"));
}

/// Verifies logs output renders available runner log storage keys and content.
#[tokio::test]
async fn submissions_logs_renders_available_runner_logs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/logs",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "solution_submission_id": "11111111-1111-4111-8111-111111111111",
            "availability": "available",
            "runner_log_storage_key": "eval-artifacts/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/attempt-1/runner.log",
            "content": "runner failed with useful diagnostics",
            "truncated": false
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
        "--token",
        "test-token",
        "submissions",
        "logs",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions logs should succeed");

    assert!(output.contains("availability: available"));
    assert!(output.contains("runner_log_storage_key: eval-artifacts/"));
    assert!(output.contains("runner failed with useful diagnostics"));
}

/// Verifies logs output reports redaction reasons instead of looking empty.
#[tokio::test]
async fn submissions_logs_renders_redaction_availability() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/logs",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "solution_submission_id": "11111111-1111-4111-8111-111111111111",
            "availability": "redacted_private_official",
            "truncated": false
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
        "--token",
        "test-token",
        "submissions",
        "logs",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions logs should succeed");

    assert!(output.contains("availability: redacted_private_official"));
    assert!(output.contains("runner_log_storage_key: none"));
}

/// Verifies JSON logs output uses the explicit storage-key and availability fields.
#[tokio::test]
async fn submissions_logs_json_uses_runner_log_storage_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/logs",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "solution_submission_id": "11111111-1111-4111-8111-111111111111",
            "availability": "available",
            "runner_log_storage_key": "eval-artifacts/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/attempt-1/runner.log",
            "content": "json diagnostics",
            "truncated": false
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
        "--token",
        "test-token",
        "--json",
        "submissions",
        "logs",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions logs should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("logs JSON should parse");

    assert_eq!(parsed["availability"], "available");
    assert!(parsed["runner_log_storage_key"].is_string());
    assert!(parsed.get("log_key").is_none());
}

/// Verifies that public result reports work without a configured token.
#[tokio::test]
async fn submissions_report_uses_public_result_report_without_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/result-report",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "solution_submission": solution_submission_json()
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(query_param("challenge_name", "sample-sum"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ranking_context_json()))
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
        "submissions",
        "report",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions report should succeed");

    assert!(output.contains("solution_submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("rank: 1"));
    assert!(output.contains("logs: configure the submitter token"));
}

/// Verifies reports use validation rank score when no official evaluation exists.
#[tokio::test]
async fn submissions_report_uses_validation_rank_score_without_official_eval() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/result-report",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "solution_submission": validation_only_solution_submission_json()
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {
                "code": "forbidden",
                "message": "leaderboard is hidden"
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {
                "code": "forbidden",
                "message": "leaderboard is hidden"
            }
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
        "--token",
        "test-token",
        "submissions",
        "report",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions report should use validation evaluation");

    assert!(output.contains("validation_primary_metric: score=0.75"));
    assert!(output.contains("official_primary_metric: none"));
    assert!(output.contains("rank_score: 0.75"));
}

/// Verifies that reports still render when ranking context is hidden.
#[tokio::test]
async fn submissions_report_tolerates_hidden_public_ranking_context() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/result-report",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "solution_submission": solution_submission_json()
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {
                "code": "forbidden",
                "message": "leaderboard is hidden"
            }
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
        "submissions",
        "report",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions report should tolerate hidden ranking context");

    assert!(output.contains("solution_submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("rank: unranked"));
    assert!(output.contains("total_ranked: unknown"));
}

/// Verifies that ranking context can use the public route without a token.
#[tokio::test]
async fn submissions_rank_uses_public_context_without_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(query_param("challenge_name", "sample-sum"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ranking_context_json()))
        .expect(1)
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
        "submissions",
        "rank",
        "11111111-1111-4111-8111-111111111111",
        "--challenge",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions rank should use public context");

    assert!(output.contains("solution_submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("rank: 1"));
}

/// Verifies that ranking context uses the authenticated route when a token is configured.
#[tokio::test]
async fn submissions_rank_uses_authenticated_context_with_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("challenge_name", "sample-sum"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ranking_context_json()))
        .expect(1)
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
        "submissions",
        "rank",
        "11111111-1111-4111-8111-111111111111",
        "--challenge",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(
        cli,
        Environment {
            token: Some(SecretString::from("test-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("submissions rank should use authenticated context");

    assert!(output.contains("solution_submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("rank: 1"));
}

/// Verifies that ranking context falls back to the public route when owner details are hidden.
#[tokio::test]
async fn submissions_rank_falls_back_to_public_context_after_auth_forbidden() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/agent/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("challenge_name", "sample-sum"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {
                "code": "forbidden",
                "message": "ranking context hidden"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(query_param("challenge_name", "sample-sum"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ranking_context_json()))
        .expect(1)
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
        "submissions",
        "rank",
        "11111111-1111-4111-8111-111111111111",
        "--challenge",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(
        cli,
        Environment {
            token: Some(SecretString::from("test-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("submissions rank should fall back to public context");

    assert!(output.contains("solution_submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("rank: 1"));
}

mod admin_workflows;
mod challenge_creator;
mod config_auth;
mod creator_workflows;
mod fixtures;
mod public_challenges;
mod registration;
mod validation;

use fixtures::*;
