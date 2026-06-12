use super::fixtures::*;
use super::*;

/// Verifies local challenge proposal checks validate one proposal directory.
#[tokio::test]
async fn challenge_creator_check_validates_single_proposal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proposal = temp.path().join("sample-sum");
    write_valid_check_proposal(&proposal, "sample-sum");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--json",
        "challenge-creator",
        "check",
        proposal.to_str().expect("utf8 path"),
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("local proposal check should pass");
    let value: serde_json::Value =
        serde_json::from_str(&output).expect("check output should be JSON");

    assert_eq!(value["checked_count"], 1);
    assert_eq!(value["passed_count"], 1);
    assert_eq!(value["failed_count"], 0);
    assert_eq!(value["results"][0]["challenge_name"], "sample-sum");
}

/// Verifies local challenge proposal checks discover direct child proposal directories.
#[tokio::test]
async fn challenge_creator_check_validates_proposal_collection() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_valid_check_proposal(&temp.path().join("sample-sum"), "sample-sum");
    write_valid_check_proposal(&temp.path().join("sample-max"), "sample-max");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--json",
        "challenge-creator",
        "check",
        temp.path().to_str().expect("utf8 path"),
    ]);

    let output = execute(cli, Environment::default())
        .await
        .expect("local proposal collection check should pass");
    let value: serde_json::Value =
        serde_json::from_str(&output).expect("check output should be JSON");

    assert_eq!(value["checked_count"], 2);
    assert_eq!(value["passed_count"], 2);
    assert_eq!(value["failed_count"], 0);
}

/// Verifies local checks reject source specs with missing required nullable fields.
#[tokio::test]
async fn challenge_creator_check_rejects_missing_required_nullable_field() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proposal = temp.path().join("sample-sum");
    write_valid_check_proposal(&proposal, "sample-sum");
    let spec_path = proposal.join("v1/spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec file"))
            .expect("spec should be JSON");
    spec["targets"][0]["resource_profile"]
        .as_object_mut()
        .expect("resource profile should be an object")
        .remove("hardware_metadata");
    std::fs::write(&spec_path, spec.to_string()).expect("spec update");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "challenge-creator",
        "check",
        proposal.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("missing required nullable field should fail check");

    assert!(error.to_string().contains("hardware_metadata"));
}

/// Verifies JSON check failures still produce a machine-readable report.
#[tokio::test]
async fn challenge_creator_check_json_failure_carries_report_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proposal = temp.path().join("sample-sum");
    write_valid_check_proposal(&proposal, "sample-sum");
    let spec_path = proposal.join("v1/spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec file"))
            .expect("spec should be JSON");
    spec["targets"][0]["resource_profile"]
        .as_object_mut()
        .expect("resource profile should be an object")
        .remove("hardware_metadata");
    std::fs::write(&spec_path, spec.to_string()).expect("spec update");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "--json",
        "challenge-creator",
        "check",
        proposal.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("missing required nullable field should fail check");
    let failure = error
        .downcast_ref::<crate::CommandFailureWithOutput>()
        .expect("JSON check failure should carry structured output");
    let value: serde_json::Value =
        serde_json::from_str(failure.output()).expect("failure output should be JSON");

    assert_eq!(value["failed_count"], 1);
    assert!(
        value["results"][0]["error"]
            .as_str()
            .expect("error string")
            .contains("hardware_metadata")
    );
}

/// Verifies local challenge proposal checks reject directories that are not recognizable proposals.
#[tokio::test]
async fn challenge_creator_check_rejects_invalid_path_shape() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "challenge-creator",
        "check",
        temp.path().to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("invalid challenge proposal shape should fail");

    assert!(error.to_string().contains("accepted layouts"));
}

/// Verifies local checks reject unsafe run manifest locators before publishing.
#[tokio::test]
async fn challenge_creator_check_rejects_unsafe_run_locator() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proposal = temp.path().join("sample-sum");
    write_valid_check_proposal(&proposal, "sample-sum");
    let spec_path = proposal.join("v1/spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec file"))
            .expect("spec should be JSON");
    spec["execution"]["validation_runs"] = json!("../public/runs.json");
    std::fs::write(&spec_path, spec.to_string()).expect("spec update");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "challenge-creator",
        "check",
        proposal.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("unsafe run locator should fail check");

    assert!(error.to_string().contains("safe relative paths"));
}

/// Verifies local checks reject unknown nested challenge-owned fields.
#[tokio::test]
async fn challenge_creator_check_rejects_unknown_nested_field() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proposal = temp.path().join("sample-sum");
    write_valid_check_proposal(&proposal, "sample-sum");
    let spec_path = proposal.join("v1/spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec file"))
            .expect("spec should be JSON");
    spec["metric_schema"]["metrics"][0]["unexpected"] = json!("residue");
    std::fs::write(&spec_path, spec.to_string()).expect("spec update");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "challenge-creator",
        "check",
        proposal.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("unknown nested field should fail check");

    assert!(error.to_string().contains("unexpected"));
}

/// Verifies local checks reject private benchmark files in the public proposal checkout.
#[tokio::test]
async fn challenge_creator_check_rejects_private_file_leakage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proposal = temp.path().join("sample-sum");
    write_valid_check_proposal(&proposal, "sample-sum");
    let private_dir = proposal.join("v1/private-benchmark");
    std::fs::create_dir_all(&private_dir).expect("private dir");
    std::fs::write(private_dir.join("secret.txt"), "hidden answer").expect("private marker");
    let config_path = temp.path().join("config.toml");
    let cli = Cli::parse_from([
        "agentics",
        "--config",
        config_path.to_str().expect("utf8 path"),
        "challenge-creator",
        "check",
        proposal.to_str().expect("utf8 path"),
    ]);

    let error = execute(cli, Environment::default())
        .await
        .expect_err("private benchmark material should fail check");

    assert!(error.to_string().contains("private"));
}

/// Verifies that challenge creator creates a review record from repo manifest.
#[tokio::test]
async fn challenge_creator_creates_review_record_from_repo_manifest() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/creator/challenge-review-records"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(challenge_review_record_json("pending_review")),
        )
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
        "challenge-creator",
        "review-record",
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

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("test-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator review record creation should succeed with creator token");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert_eq!(requests.len(), 1);
    assert!(output.contains("status: pending_review"));
}

/// Verifies that challenge creator rejects invalid commit sha during cli parse.
#[test]
fn challenge_creator_rejects_invalid_commit_sha_during_cli_parse() {
    let result = Cli::try_parse_from([
        "agentics",
        "challenge-creator",
        "review-record",
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
        .and(path("/api/creator/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/private-assets"))
        .and(header("authorization", "Bearer test-token"))
        .and(body_json(json!({
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "required": true,
            "asset_base64": encoded_asset
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            "review_record_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "required": true,
            "size_bytes": 17,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "storage_key": "challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/private-assets/official-cases.bin",
            "uploader_human_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
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
        "challenge-creator",
        "review-record",
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

    let output = execute(
        cli,
        Environment {
            creator_api_token: Some(SecretString::from("test-token")),
            ..Environment::default()
        },
    )
    .await
    .expect("creator private asset upload should succeed with creator token");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert_eq!(requests.len(), 1);
    assert!(output.contains("official-cases"));
}

/// Writes a minimal valid challenge proposal for local checker tests.
fn write_valid_check_proposal(proposal: &std::path::Path, challenge_name: &str) {
    let bundle = proposal.join("v1");
    std::fs::create_dir_all(bundle.join("public")).expect("public dir");
    std::fs::write(proposal.join("README.md"), "# Sample\n").expect("readme");
    std::fs::write(bundle.join("statement.md"), "# Sample\n").expect("statement");
    std::fs::write(
        bundle.join("public/runs.json"),
        json!({
            "runs": [
                {
                    "run_name": "case-1",
                    "interface": "stdio",
                    "stdin_json": {"a": 1, "b": 2},
                    "stdin_text": null,
                    "input_files": null,
                    "output_files": null,
                    "metadata": null
                }
            ]
        })
        .to_string(),
    )
    .expect("runs");
    std::fs::write(
        bundle.join("spec.json"),
        json!({
            "schema_version": 1,
            "challenge_name": challenge_name,
            "challenge_title": "Sample Sum",
            "summary": {"en": "Add numbers", "zh": "数字求和"},
            "keywords": ["arithmetic"],
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "targets": [
                {
                    "name": "linux-arm64-cpu",
                    "docker_platform": "linux/arm64",
                    "accelerator": null,
                    "validation_enabled": true,
                    "resource_profile": {
                        "name": "agentics-cpu-small",
                        "resource_description": null,
                        "solution_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "evaluator_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "solution": {
                            "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                            "build": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                            "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                        },
                        "evaluator": {
                            "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                            "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                        },
                        "hardware_metadata": null
                    }
                }
            ],
            "starts_at": "2026-01-01T00:00:00Z",
            "closes_at": null,
            "eligibility": {"type": "open"},
            "validation_submission_limit": 20,
            "official_submission_limit": null,
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_after_close"
            },
            "solution_publication": "public",
            "execution": {
                "mode": "separated_evaluator",
                "separated_evaluator": {
                    "command": ["python", "separated-evaluator/run.py"],
                    "result_file": "result.json"
                },
                "validation_runs": "public/runs.json",
                "validation_setup": null,
                "official_runs": null,
                "official_evaluation_setup": null
            },
            "datasets": {
                "public_dir": "public",
                "private_benchmark_dir": null,
                "public_policy": "full",
                "private_benchmark_policy": "score_only",
                "private_benchmark_enabled": false
            },
            "metric_schema": {
                "metrics": [
                    {
                        "name": "score",
                        "label": "Score",
                        "unit": null,
                        "direction": "maximize",
                        "visibility": "public",
                        "metric_description": "Challenge-defined compatibility score."
                    }
                ],
                "ranking": {
                    "primary_metric_name": "score",
                    "tie_breaker_metric_names": null
                }
            }
        })
        .to_string(),
    )
    .expect("spec");
    std::fs::write(
        proposal.join("agentics.challenge.json"),
        json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": challenge_name,
            "title": "Sample Sum",
            "summary": {"en": "Add numbers", "zh": "数字求和"},
            "keywords": ["arithmetic"],
            "readme_path": "README.md",
            "bundle_path": "v1",
            "private_assets": []
        })
        .to_string(),
    )
    .expect("manifest");
}

/// Verifies that admin validates a review record with admin auth.
#[tokio::test]
async fn admin_validates_review_record_with_admin_auth() {
    let server = MockServer::start().await;
    let admin_token = "agentics_admin_secret";
    let admin_auth = format!("Bearer {admin_token}");
    Mock::given(method("POST"))
        .and(path(
            "/admin/challenge-review-records/dddddddd-dddd-4ddd-8ddd-dddddddddddd/validate",
        ))
        .and(header("authorization", admin_auth))
        .and(body_json(json!({ "repository_path": "/tmp/challenges" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(challenge_review_record_json("validated")),
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
        "admin",
        "review-record",
        "validate",
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "--repository-path",
        "/tmp/challenges",
    ]);

    let output = execute(
        cli,
        Environment {
            admin_service_token: Some(SecretString::from(admin_token)),
            ..Environment::default()
        },
    )
    .await
    .expect("admin validation should succeed");

    assert!(output.contains("status: validated"));
    assert!(output.contains(
        "validation_bundle_sha256: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    ));
}

/// Verifies admin review record validation rejects non-UTF-8 repository paths before the API request.
#[cfg(unix)]
#[tokio::test]
async fn admin_rejects_non_utf8_repository_path() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let server = MockServer::start().await;
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.toml");
    let args = vec![
        OsString::from("agentics"),
        OsString::from("--config"),
        config_path.as_os_str().to_owned(),
        OsString::from("--api-base-url"),
        OsString::from(server.uri()),
        OsString::from("admin"),
        OsString::from("review-record"),
        OsString::from("validate"),
        OsString::from("dddddddd-dddd-4ddd-8ddd-dddddddddddd"),
        OsString::from("--repository-path"),
        OsString::from_vec(b"/tmp/challenges-\xff".to_vec()),
    ];
    let cli = Cli::parse_from(args);

    let error = execute(
        cli,
        Environment {
            admin_service_token: Some(SecretString::from("agentics_admin_secret")),
            ..Environment::default()
        },
    )
    .await
    .expect_err("non-UTF-8 repository path should be rejected");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(requests.is_empty());
    assert!(error.to_string().contains("not valid UTF-8"));
}

/// Verifies top-level admin review-record list uses admin service-token auth.
#[tokio::test]
async fn admin_review_record_list_uses_admin_service_token() {
    let server = MockServer::start().await;
    let admin_token = "agentics_admin_secret";
    Mock::given(method("GET"))
        .and(path("/admin/challenge-review-records"))
        .and(header("authorization", format!("Bearer {admin_token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": []
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
        "admin",
        "review-record",
        "list",
    ]);

    let output = execute(
        cli,
        Environment {
            admin_service_token: Some(SecretString::from(admin_token)),
            ..Environment::default()
        },
    )
    .await
    .expect("admin review-record list should succeed");

    assert!(output.contains("review_records: 0"));
}
