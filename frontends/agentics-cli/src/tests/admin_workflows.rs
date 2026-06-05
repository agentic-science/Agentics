use super::*;

fn temp_config_path(temp: &tempfile::TempDir) -> String {
    temp.path()
        .join("config.toml")
        .to_str()
        .expect("utf8 config path")
        .to_string()
}

fn admin_env(token: &str) -> Environment {
    Environment {
        admin_service_token: Some(SecretString::from(token)),
        ..Environment::default()
    }
}

/// Verifies admin pioneer-code list/show/create/revoke workflows.
#[tokio::test]
async fn admin_pioneer_code_workflows_use_admin_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/pioneer-codes"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pioneer_code_list_json()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/admin/pioneer-codes/11111111-1111-4111-8111-111111111111",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pioneer_code_detail_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/admin/pioneer-codes"))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({
            "label": "jack",
            "note": "early access",
            "max_uses": 2,
            "expires_at": "2026-06-01T00:00:00Z"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(pioneer_code_detail_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/pioneer-codes/11111111-1111-4111-8111-111111111111/revoke",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(pioneer_code_revoke_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let base = [
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
    ];
    let list = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "pioneer-code",
        "list",
    ]);
    let show = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "pioneer-code",
        "show",
        "11111111-1111-4111-8111-111111111111",
    ]);
    let create = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "pioneer-code",
        "create",
        "--label",
        "jack",
        "--note",
        "early access",
        "--max-uses",
        "2",
        "--expires-at",
        "2026-06-01T00:00:00Z",
    ]);
    let revoke = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "pioneer-code",
        "revoke",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let list_output = execute(list, admin_env("admin-token"))
        .await
        .expect("pioneer code list");
    let show_output = execute(show, admin_env("admin-token"))
        .await
        .expect("pioneer code show");
    let create_output = execute(create, admin_env("admin-token"))
        .await
        .expect("pioneer code create");
    let revoke_output = execute(revoke, admin_env("admin-token"))
        .await
        .expect("pioneer code revoke");

    assert!(list_output.contains("pioneer_codes: 1"));
    assert!(show_output.contains("pioneer_code: jack-7f9eb67a"));
    assert!(create_output.contains("status: active"));
    assert!(revoke_output.contains("revoked_creator_api_tokens: 3"));
}

/// Verifies admin pioneer-code create supports JSON output.
#[tokio::test]
async fn admin_pioneer_code_create_renders_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/admin/pioneer-codes"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(pioneer_code_detail_json()))
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
        "admin",
        "pioneer-code",
        "create",
        "--max-uses",
        "2",
    ]);

    let output = execute(cli, admin_env("admin-token"))
        .await
        .expect("pioneer code create");
    let value: serde_json::Value = serde_json::from_str(&output).expect("json output");

    assert_eq!(value["code"]["code_display"], "jack-7f9eb67a");
}

/// Verifies admin challenge and Moltbook workflows.
#[tokio::test]
async fn admin_challenge_and_moltbook_workflows() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/challenges"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(admin_challenge_list_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/admin/challenges/sample-sum/moltbook-discussion"))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({
            "discussion_url": "https://www.moltbook.com/post/sample-sum"
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(moltbook_discussion_json(Some(
                "https://www.moltbook.com/post/sample-sum",
            ))),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/admin/challenges/sample-sum/moltbook-discussion"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(moltbook_discussion_json(None)))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let list = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "challenges",
        "list",
    ]);
    let set = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "moltbook",
        "set",
        "sample-sum",
        "--discussion-url",
        "https://www.moltbook.com/post/sample-sum",
    ]);
    let clear = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "moltbook",
        "clear",
        "sample-sum",
    ]);

    let list_output = execute(list, admin_env("admin-token"))
        .await
        .expect("challenge list");
    let set_output = execute(set, admin_env("admin-token"))
        .await
        .expect("moltbook set");
    let clear_output = execute(clear, admin_env("admin-token"))
        .await
        .expect("moltbook clear");

    assert!(list_output.contains("challenges: 1"));
    assert!(set_output.contains("https://www.moltbook.com/post/sample-sum"));
    assert!(clear_output.contains("moltbook_discussion: none"));
}

/// Verifies admin submission list, rejudge, and official-run workflows.
#[tokio::test]
async fn admin_submission_workflows() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/solution-submissions"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(admin_solution_submission_list_json()),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/solution-submissions/11111111-1111-4111-8111-111111111111/rejudge",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(evaluation_job_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/solution-submissions/11111111-1111-4111-8111-111111111111/official-run",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(evaluation_job_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let list = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "submissions",
        "list",
    ]);
    let rejudge = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "submissions",
        "rejudge",
        "11111111-1111-4111-8111-111111111111",
    ]);
    let official = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "submissions",
        "official-run",
        "11111111-1111-4111-8111-111111111111",
    ]);

    let list_output = execute(list, admin_env("admin-token"))
        .await
        .expect("submission list");
    let rejudge_output = execute(rejudge, admin_env("admin-token"))
        .await
        .expect("rejudge");
    let official_output = execute(official, admin_env("admin-token"))
        .await
        .expect("official run");

    assert!(list_output.contains("solution_submissions: 1"));
    assert!(rejudge_output.contains("job: 22222222-2222-4222-8222-222222222222"));
    assert!(official_output.contains("status: queued"));
}

/// Verifies admin review-record lifecycle workflows.
#[tokio::test]
async fn admin_review_record_workflows() {
    let server = MockServer::start().await;
    let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    Mock::given(method("GET"))
        .and(path("/admin/challenge-review-records"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [challenge_review_record_json("validated")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/admin/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/private-assets",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(admin_private_assets_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/approve",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({
            "message": "looks good",
            "expected_validation_bundle_sha256": digest
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(challenge_review_record_json("approved")),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/reject",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({ "message": "needs work" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(challenge_review_record_json("rejected")),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/abandon",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({ "message": "withdrawn" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(challenge_review_record_json("abandoned")),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/publish",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({ "repository_path": "/tmp/challenges" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(challenge_review_record_json("published")),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/admin/challenge-review-records/cleanup"))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(review_record_cleanup_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let base = [
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
    ];
    let list = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "review-record",
        "list",
    ]);
    let private_assets = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "review-record",
        "private-assets",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
    ]);
    let approve = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "--json",
        "admin",
        "review-record",
        "approve",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--expected-validation-bundle-sha256",
        digest,
        "--message",
        "looks good",
    ]);
    let reject = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "review-record",
        "reject",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--message",
        "needs work",
    ]);
    let abandon = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "review-record",
        "abandon",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--message",
        "withdrawn",
    ]);
    let publish = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "review-record",
        "publish",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--repository-path",
        "/tmp/challenges",
    ]);
    let cleanup = Cli::parse_from([
        base[0],
        base[1],
        base[2],
        base[3],
        base[4],
        "admin",
        "review-record",
        "cleanup",
    ]);

    assert!(
        execute(list, admin_env("admin-token"))
            .await
            .expect("review record list")
            .contains("review_records: 1")
    );
    assert!(
        execute(private_assets, admin_env("admin-token"))
            .await
            .expect("private assets")
            .contains("private_assets: 1")
    );
    let approve_json = execute(approve, admin_env("admin-token"))
        .await
        .expect("approve");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&approve_json).expect("json")["status"],
        "approved"
    );
    assert!(
        execute(reject, admin_env("admin-token"))
            .await
            .expect("reject")
            .contains("status: rejected")
    );
    assert!(
        execute(abandon, admin_env("admin-token"))
            .await
            .expect("abandon")
            .contains("status: abandoned")
    );
    assert!(
        execute(publish, admin_env("admin-token"))
            .await
            .expect("publish")
            .contains("status: published")
    );
    assert!(
        execute(cleanup, admin_env("admin-token"))
            .await
            .expect("cleanup")
            .contains("purged_private_assets: 3")
    );
}

/// Verifies admin operations and agent-disable workflows.
#[tokio::test]
async fn admin_operations_workflows() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/service-heartbeats"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(service_heartbeat_list_json()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/admin/capacity"))
        .and(header("authorization", "Bearer admin-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(admin_capacity_json()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/admin/agents/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/disable",
        ))
        .and(header("authorization", "Bearer admin-token"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(disable_agent_json()))
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let heartbeats = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "service-heartbeats",
        "list",
    ]);
    let capacity = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "capacity",
    ]);
    let disable = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "agents",
        "disable",
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    ]);

    assert!(
        execute(heartbeats, admin_env("admin-token"))
            .await
            .expect("heartbeats")
            .contains("service_heartbeats: 1")
    );
    assert!(
        execute(capacity, admin_env("admin-token"))
            .await
            .expect("capacity")
            .contains("active_agents: 7")
    );
    assert!(
        execute(disable, admin_env("admin-token"))
            .await
            .expect("disable agent")
            .contains("status: disabled")
    );
}

/// Verifies env admin service tokens are used by admin commands.
#[tokio::test]
async fn admin_token_env_source_is_used() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/capacity"))
        .and(header("authorization", "Bearer env-admin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(admin_capacity_json()))
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
        "admin",
        "capacity",
    ]);

    execute(cli, admin_env("env-admin"))
        .await
        .expect("admin capacity should use env token");
}

/// Verifies stdin admin service tokens override env tokens.
#[tokio::test]
async fn admin_token_stdin_overrides_env_source() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/admin/capacity"))
        .and(header("authorization", "Bearer stdin-admin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(admin_capacity_json()))
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
        "admin",
        "--admin-service-token-stdin",
        "capacity",
    ]);

    execute_with_input(
        cli,
        admin_env("env-admin"),
        CommandInput::test_stdin("stdin-admin\n"),
    )
    .await
    .expect("admin capacity should use stdin token");
}

/// Verifies missing admin tokens fail before HTTP.
#[tokio::test]
async fn admin_command_rejects_missing_token_before_http() {
    let server = MockServer::start().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp_config_path(&temp);
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        &config_path,
        "--api-base-url",
        &server.uri(),
        "admin",
        "capacity",
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("missing admin token should fail");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(requests.is_empty());
    assert!(error.to_string().contains("AGENTICS_ADMIN_SERVICE_TOKEN"));
}
