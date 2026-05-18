use std::path::Path;

use clap::Parser;
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::cli::Cli;
use crate::config::Environment;
use crate::execute;

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
                    "starts_at": "2026-01-01T00:00:00Z",
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

/// Verifies that global json output renders structured command data.
#[tokio::test]
async fn global_json_flag_renders_structured_output() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {
                    "name": "sample-sum",
                    "title": "Sample Sum",
                    "summary": "Add numbers",
                    "starts_at": "2026-01-01T00:00:00Z",
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
        "--json",
        "challenges",
        "list",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("challenge list should succeed");
    let value: serde_json::Value = serde_json::from_str(&output).expect("output should be JSON");

    assert_eq!(value["items"][0]["name"], "sample-sum");
}

/// Verifies that the pre-MVP CLI rejects the old output-format flag.
#[test]
fn old_output_json_flag_is_removed() {
    let result = Cli::try_parse_from(["agentics", "--output", "json", "challenges", "list"]);

    assert!(result.is_err());
}

/// Verifies that challenge stats combines public result surfaces.
#[tokio::test]
async fn challenges_stats_combines_public_surfaces() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum/leaderboard"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(leaderboard_json()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/challenges/sample-sum/score-distributions",
        ))
        .and(query_param("target", "linux-arm64-cpu"))
        .and(query_param("metric", "score"))
        .respond_with(ResponseTemplate::new(200).set_body_json(score_distribution_json()))
        .mount(&server)
        .await;
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
        "challenges",
        "stats",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("challenge stats should succeed");

    assert!(output.contains("challenge: sample-sum (Sample Sum)"));
    assert!(output.contains("ranked_agents: 2"));
    assert!(output.contains("visible_submissions: 3"));
    assert!(output.contains("p90_score: 1.8000"));
    assert!(output.contains("solver"));
}

/// Verifies that challenge stats still renders when details are not public.
#[tokio::test]
async fn challenges_stats_tolerates_hidden_submission_details() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum/leaderboard"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(200).set_body_json(leaderboard_json()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/challenges/sample-sum/score-distributions",
        ))
        .and(query_param("target", "linux-arm64-cpu"))
        .and(query_param("metric", "score"))
        .respond_with(ResponseTemplate::new(200).set_body_json(score_distribution_json()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/public/challenges/sample-sum/solution-submissions",
        ))
        .and(query_param("target", "linux-arm64-cpu"))
        .and(query_param("limit", "20"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": "forbidden",
            "message": "result details are hidden"
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
        "stats",
        "sample-sum",
        "--target",
        "linux-arm64-cpu",
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("challenge stats should tolerate hidden submission details");

    assert!(output.contains("ranked_agents: 2"));
    assert!(output.contains("visible_submissions: unavailable"));
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
            .all(|request| request.url.path() != "/api/solution-submissions")
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
            "note": "",
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
            "note": "Remote smoke",
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
            "error": "forbidden",
            "message": "leaderboard is hidden"
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
            "/api/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
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
            "/api/solution-submissions/11111111-1111-4111-8111-111111111111/ranking-context",
        ))
        .and(header("authorization", "Bearer test-token"))
        .and(query_param("challenge_name", "sample-sum"))
        .and(query_param("target", "linux-arm64-cpu"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": "forbidden",
            "message": "ranking context hidden"
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

/// Verifies that admin challenge-draft commands parse draft IDs before HTTP execution.
#[test]
fn invalid_challenge_draft_id_fails_during_cli_parse() {
    let result = Cli::try_parse_from([
        "agentics",
        "challenge-creator",
        "draft",
        "validate",
        "draft-1",
        "--repository-path",
        ".",
        "--admin-username",
        "admin",
        "--admin-password-stdin",
    ]);

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
            "note": "",
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
            "note": "Remote smoke",
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
    let mut spec = challenge_detail_json(false)
        .get("spec")
        .expect("spec")
        .clone();
    spec["datasets"]["private_benchmark_enabled"] = json!(false);
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

mod challenge_creator;
mod fixtures;
mod registration;

use fixtures::*;
