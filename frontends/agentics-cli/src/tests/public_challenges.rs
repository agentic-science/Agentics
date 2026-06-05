use super::*;

/// Verifies that challenges list uses public api and renders table.
#[tokio::test]
async fn challenges_list_uses_public_api_and_renders_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/public/challenges"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {
                    "challenge_name": "sample-sum",
                    "title": "Sample Sum",
                    "summary": { "en": "Add numbers", "zh": "数字求和" },
                    "keywords": ["math"],
                    "starts_at": "2026-01-01T00:00:00Z",
                    "eligibility": { "type": "open" }
                }
            ],
            "total_count": 1,
            "limit": 100,
            "offset": 0,
            "has_more": false
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

    assert!(output.contains("sample-sum"));
    assert!(output.contains("Sample Sum"));
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
                    "challenge_name": "sample-sum",
                    "title": "Sample Sum",
                    "summary": { "en": "Add numbers", "zh": "数字求和" },
                    "keywords": ["math"],
                    "starts_at": "2026-01-01T00:00:00Z",
                    "eligibility": { "type": "open" }
                }
            ],
            "total_count": 1,
            "limit": 100,
            "offset": 0,
            "has_more": false
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

    assert_eq!(value["items"][0]["challenge_name"], "sample-sum");
}

/// Verifies that challenge stats combines public result surfaces.
#[tokio::test]
async fn challenges_stats_combines_public_result_surfaces() {
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
            "error": {
                "code": "forbidden",
                "message": "result details are hidden"
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
