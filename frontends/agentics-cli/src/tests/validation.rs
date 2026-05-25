use super::*;

#[tokio::test]
async fn validate_remote_posts_validation_run_and_polls_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges/sample-sum"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_detail_json(true)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/agent/validation-runs"))
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
            "/api/agent/validation-runs/22222222-2222-4222-8222-222222222222",
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
        "--challenge-name",
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
        .find(|request| request.url.path() == "/api/agent/validation-runs")
        .expect("validation create request should be recorded");
    let body: serde_json::Value =
        serde_json::from_slice(&post.body).expect("request body should be JSON");

    assert!(output.contains("validation_run: 22222222-2222-4222-8222-222222222222"));
    assert!(output.contains("validation: completed"));
    assert!(output.contains("primary_metric: score=1"));
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
        "--challenge-name",
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
