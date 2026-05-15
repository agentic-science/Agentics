use std::path::Path;

use clap::Parser;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::cli::Cli;
use crate::config::{CliConfig, ConfigStore, Environment};
use crate::execute;

/// Writes solution manifest to the target path.
fn write_solution_manifest(workspace_dir: &Path) {
    std::fs::write(
        workspace_dir.join("agentics.solution.json"),
        json!({
            "protocol": "zip_project",
            "protocol_version": 1,
            "runtime": { "language": "python" },
            "commands": { "run": "run.sh" },
            "interface": { "kind": "stdio" },
            "dependencies": { "policy": "image_provided" }
        })
        .to_string(),
    )
    .expect("manifest");
}

/// Verifies that register persists returned token.
#[tokio::test]
async fn register_persists_returned_token() {
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
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("register should succeed");
    let saved = ConfigStore::new(config_path)
        .load()
        .expect("config should load");

    assert!(output.contains("Registered agent solver"));
    assert_eq!(
        saved,
        CliConfig {
            api_base_url: Some(server.uri()),
            token: Some("agentics_token".to_string()),
        }
    );
}

/// Verifies that challenges list uses public api and renders table.
#[tokio::test]
async fn challenges_list_uses_public_api_and_renders_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {
                    "name": "sample-sum",
                    "title": "Sample Sum",
                    "summary": "Add numbers",
                    "eligibility": { "type": "open" }
                }
            ]
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
        "challenges",
        "list",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("challenge list should succeed");

    assert_eq!(
        output,
        "NAME        ELIGIBILITY  TITLE\nsample-sum  open         Sample Sum"
    );
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
        .and(path("/api/solution-submissions"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "status": "queued",
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_key": "solution-submissions/11111111-1111-4111-8111-111111111111.zip",
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
        .find(|request| request.url.path() == "/api/solution-submissions")
        .expect("solution submission create request should be recorded");
    let body: serde_json::Value =
        serde_json::from_slice(&post.body).expect("request body should be JSON");

    assert!(output.contains("Submitted 11111111-1111-4111-8111-111111111111"));
    assert_eq!(body["challenge_name"], "sample-sum");
    assert_eq!(body["target"], "linux-arm64-cpu");
    assert_eq!(body["explanation"], "first attempt");
    assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
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

/// Verifies that submissions show fetches authenticated solution submission.
#[tokio::test]
async fn submissions_show_fetches_authenticated_solution_submission() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/solution-submissions/11111111-1111-4111-8111-111111111111",
        ))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "target": "linux-arm64-cpu",
            "agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "agent_display_name": "solver",
            "status": "queued",
            "explanation": "",
            "parent_solution_submission_id": null,
            "credit_text": "",
            "visible_after_eval": false,
            "artifact_key": "solution-submissions/11111111-1111-4111-8111-111111111111.zip",
            "evaluation_job": {
                "id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "target": "linux-arm64-cpu",
                "status": "queued"
            },
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
        "--token",
        "test-token",
        "submissions",
        "show",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions show should succeed");

    assert!(output.contains("solution submission: 11111111-1111-4111-8111-111111111111"));
    assert!(output.contains("evaluation_job: bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb (queued)"));
}

/// Verifies that submissions show fetches validation run id.
#[tokio::test]
async fn submissions_show_fetches_validation_run_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/api/solution-submissions/22222222-2222-4222-8222-222222222222",
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
            "explanation": "quick check",
            "parent_solution_submission_id": null,
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
                "primary_score": 1.0,
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
                "primary_score": 1.0,
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
        "show",
        "22222222-2222-4222-8222-222222222222",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("submissions show should read validation ids through solution submissions");

    assert!(output.contains("solution submission: 22222222-2222-4222-8222-222222222222"));
    assert!(output.contains("validation_evaluation: completed"));
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

/// Verifies that validate remote posts validation run and polls status.
#[tokio::test]
async fn validate_remote_posts_validation_run_and_polls_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/validation-runs"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "22222222-2222-4222-8222-222222222222",
            "status": "queued",
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_key": "solution-submissions/22222222-2222-4222-8222-222222222222.zip",
            "evaluation_job_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/validation-runs/22222222-2222-4222-8222-222222222222",
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
            "explanation": "quick check",
            "parent_solution_submission_id": null,
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
                "primary_score": 1.0,
                "rank_score": 1.0,
                "aggregate_metrics": [
                    { "metric_name": "score", "value": 1.0 },
                    { "metric_name": "passed_cases", "value": 2.0 }
                ],
                "run_metrics": [],
                "public_results": [],
                "validation_summary": {
                    "score": 1.0,
                    "passed": 2,
                    "total": 2
                }
            },
            "created_at": "2026-05-01T00:00:00Z",
            "updated_at": "2026-05-01T00:00:01Z"
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

    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "--token",
        "test-token",
        "validate",
        "--remote",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
        "--explanation",
        "quick check",
        "--poll-interval-ms",
        "1",
        "--timeout-sec",
        "1",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("remote validation should succeed");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");
    let post = requests
        .iter()
        .find(|request| request.url.path() == "/api/validation-runs")
        .expect("validation create request should be recorded");
    let body: serde_json::Value =
        serde_json::from_slice(&post.body).expect("request body should be JSON");

    assert!(output.contains("validation_run: 22222222-2222-4222-8222-222222222222"));
    assert!(output.contains("validation: completed"));
    assert!(output.contains("primary_score: 1"));
    assert!(output.contains("rank_score: 1"));
    assert!(output.contains("visible_after_eval: false"));
    assert_eq!(body["challenge_name"], "sample-sum");
    assert_eq!(body["target"], "linux-arm64-cpu");
    assert_eq!(body["explanation"], "quick check");
    assert!(body["artifact_base64"].as_str().expect("artifact").len() > 20);
}

/// Verifies that validate remote rejects disabled validation before packaging.
#[tokio::test]
async fn validate_remote_rejects_disabled_validation_before_packaging() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(false)))
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
        "validate",
        "--remote",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("validation should be rejected before packaging");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(error.to_string().contains("validation pass is disabled"));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/api/public/challenges/sample-sum");
}

/// Verifies that validate local requires bundle dir.
#[test]
fn validate_local_requires_bundle_dir() {
    let result = Cli::try_parse_from([
        "agentics",
        "validate",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    assert!(result.is_err());
}

/// Verifies that validate remote rejects local bundle flags.
#[test]
fn validate_remote_rejects_local_bundle_flags() {
    let result = Cli::try_parse_from([
        "agentics",
        "validate",
        "--remote",
        "sample-sum",
        "--bundle-dir",
        "/tmp/challenge",
        "--target",
        "linux-arm64-cpu",
    ]);

    assert!(result.is_err());
}

/// Verifies that validate local rejects disabled target before packaging.
#[tokio::test]
async fn validate_local_rejects_disabled_target_before_packaging() {
    let temp = tempfile::tempdir().expect("tempdir");
    let bundle_dir = temp.path().join("bundle");
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir(&bundle_dir).expect("bundle dir");
    std::fs::create_dir(&workspace_dir).expect("workspace dir");
    let spec = challenge_detail_json(false)
        .get("spec")
        .expect("spec")
        .clone();
    std::fs::write(bundle_dir.join("spec.json"), spec.to_string()).expect("spec");

    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "validate",
        "sample-sum",
        "--bundle-dir",
        bundle_dir.to_str().expect("utf8 path"),
        "--target",
        "linux-arm64-cpu",
        "--dir",
        workspace_dir.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("validation should reject disabled local target before packaging");

    assert!(error.to_string().contains("validation pass is disabled"));
}

/// Verifies that challenge creator creates draft from repo manifest.
#[tokio::test]
async fn challenge_creator_creates_draft_from_repo_manifest() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/creator/challenge-drafts"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(challenge_draft_json("draft")))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let challenge_root = temp.path().join("challenges/sample-sum");
    std::fs::create_dir_all(&challenge_root).expect("challenge root");
    std::fs::write(
        challenge_root.join("agentics.challenge.json"),
        challenge_manifest_json().to_string(),
    )
    .expect("manifest");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "--token",
        "test-token",
        "challenge-creator",
        "draft",
        "create",
        "--repo-url",
        "https://github.com/agentics-reifying/agentics-challenges",
        "--pr-number",
        "7",
        "--pr-url",
        "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "--commit-sha",
        "0123456789abcdef0123456789abcdef01234567",
        "--repo-dir",
        temp.path().to_str().expect("utf8 path"),
        "--challenge-path",
        "challenges/sample-sum",
        "--pr-author-github-user-id",
        "1001",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("draft create should succeed");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("request body");

    assert!(output.contains("challenge_draft: dddddddd-dddd-4ddd-8ddd-dddddddddddd"));
    assert_eq!(body["manifest"]["request"], "new_challenge");
    assert_eq!(body["challenge_path"], "challenges/sample-sum");
}

/// Verifies that challenge creator rejects invalid commit sha during cli parse.
#[test]
fn challenge_creator_rejects_invalid_commit_sha_during_cli_parse() {
    let result = Cli::try_parse_from([
        "agentics",
        "challenge-creator",
        "draft",
        "create",
        "--repo-url",
        "https://github.com/agentics-reifying/agentics-challenges",
        "--pr-number",
        "7",
        "--pr-url",
        "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "--commit-sha",
        "0123456789abcdef",
        "--challenge-path",
        "challenges/sample-sum",
        "--pr-author-github-user-id",
        "1001",
    ]);

    assert!(result.is_err());
}

/// Verifies that challenge creator uploads private asset file.
#[tokio::test]
async fn challenge_creator_uploads_private_asset_file() {
    let server = MockServer::start().await;
    let encoded_asset = {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.encode(b"private zip bytes")
    };
    Mock::given(method("POST"))
        .and(path("/api/creator/challenge-drafts/dddddddd-dddd-4ddd-8ddd-dddddddddddd/private-assets"))
        .and(header("authorization", "Bearer test-token"))
        .and(body_json(json!({
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "required": true,
            "asset_base64": encoded_asset
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            "draft_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "required": true,
            "size_bytes": 17,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "storage_key": "challenge-drafts/dddddddd-dddd-4ddd-8ddd-dddddddddddd/private-assets/official-cases.bin",
            "uploader_agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "created_at": "2026-05-01T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let asset_path = temp.path().join("official-cases.zip");
    std::fs::write(&asset_path, b"private zip bytes").expect("asset file");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--api-base-url",
        &server.uri(),
        "--token",
        "test-token",
        "challenge-creator",
        "draft",
        "upload-private-asset",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--asset-name",
        "official-cases",
        "--kind",
        "private_benchmark_data",
        "--file",
        asset_path.to_str().expect("utf8 path"),
        "--required",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("asset upload should succeed");

    assert!(output.contains("private_asset: eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"));
    assert!(output.contains("asset_name: official-cases"));
}

/// Verifies that challenge creator validates draft with admin auth.
#[tokio::test]
async fn challenge_creator_validates_draft_with_admin_auth() {
    let server = MockServer::start().await;
    let admin_auth = format!("Basic {}", {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.encode("admin:secret")
    });
    Mock::given(method("POST"))
        .and(path(
            "/admin/challenge-drafts/dddddddd-dddd-4ddd-8ddd-dddddddddddd/validate",
        ))
        .and(header("authorization", admin_auth))
        .and(body_json(json!({ "repository_path": "/tmp/challenges" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_draft_json("validated")))
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
        "challenge-creator",
        "draft",
        "validate",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--repository-path",
        "/tmp/challenges",
        "--admin-username",
        "admin",
        "--admin-password",
        "secret",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("admin validation should succeed");

    assert!(output.contains("status: validated"));
}

/// Handles challenge detail json for this module.
fn challenge_detail_json(validation_enabled: bool) -> serde_json::Value {
    json!({
        "name": "sample-sum",
        "title": "Sample Sum",
        "summary": "Add numbers",
        "spec": {
            "schema_version": 1,
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "challenge_summary": "Add numbers",
            "eligibility": { "type": "open" },
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_live"
            },
            "solution_publication": "public",
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "scorer": {
                "command": ["python", "scorer/run.py"],
                "result_file": "result.json"
            },
            "targets": [
                {
                    "name": "linux-arm64-cpu",
                    "docker_platform": "linux/arm64",
                    "accelerator": "cpu",
                    "validation_enabled": validation_enabled,
                    "resource_profile": {
                        "name": "python-cpu-small",
                        "solution_image": "agentics-linux-arm64-cpu:ubuntu26.04-local",
                        "scorer_image": "agentics-linux-arm64-cpu:ubuntu26.04-local",
                        "timeout_sec": 30,
                        "memory_limit_mb": 512,
                        "cpu_limit_millis": 1000,
                        "disk_limit_mb": 1024,
                        "setup_network_access": "enabled",
                        "build_network_access": "disabled",
                        "run_network_access": "disabled",
                        "scorer_network_access": "disabled"
                    }
                }
            ],
            "execution": {
                "validation_runs": "public/runs.json",
                "official_runs": "private-benchmark/runs.json"
            },
            "datasets": {
                "public_dir": "public",
                "private_benchmark_dir": "private-benchmark",
                "public_policy": "full",
                "private_benchmark_policy": "score_only",
                "private_benchmark_enabled": true
            },
            "metric_schema": {
                "metrics": [
                    {
                        "name": "score",
                        "label": "Score",
                        "direction": "maximize",
                        "visibility": "public"
                    },
                    {
                        "name": "passed_cases",
                        "label": "Passed Cases",
                        "unit": "cases",
                        "direction": "maximize",
                        "visibility": "public"
                    }
                ],
                "ranking": {
                    "primary_metric_name": "score",
                    "tie_breaker_metric_names": ["passed_cases"]
                }
            }
        },
        "statement_markdown": "# Sample Sum"
    })
}

/// Handles challenge manifest json for this module.
fn challenge_manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "new_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": "Add numbers",
        "readme_path": "README.md",
        "bundle_path": "v1",
        "private_assets": [
            {
                "asset_name": "official-cases",
                "kind": "private_benchmark_data",
                "required": true
            }
        ]
    })
}

/// Handles challenge draft json for this module.
fn challenge_draft_json(status: &str) -> serde_json::Value {
    json!({
        "id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "challenge_name": "sample-sum",
        "request": "new_challenge",
        "status": status,
        "creator_agent_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "creator_github_user_id": 1001,
        "creator_github_login": "creator",
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 7,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "commit_sha": "0123456789abcdef0123456789abcdef01234567",
        "challenge_path": "challenges/sample-sum",
        "manifest_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "manifest": challenge_manifest_json(),
        "private_assets": [],
        "validation_records": [],
        "created_at": "2026-05-01T00:00:00Z",
        "updated_at": "2026-05-01T00:00:00Z"
    })
}
