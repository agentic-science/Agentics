use super::fixtures::*;
use super::*;

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
        "--token",
        "test-token",
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

    let error = execute(cli, Environment::default())
        .await
        .expect_err("creator review record creation requires web-session auth");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(requests.is_empty());
    assert!(
        error
            .to_string()
            .contains("creator review record creation requires")
    );
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
        "--token",
        "test-token",
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

    let error = execute(cli, Environment::default())
        .await
        .expect_err("creator private asset upload requires web-session auth");
    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");

    assert!(requests.is_empty());
    assert!(
        error
            .to_string()
            .contains("creator private asset upload requires")
    );
}

/// Verifies that challenge creator validates a review record with admin auth.
#[tokio::test]
async fn challenge_creator_validates_review_record_with_admin_auth() {
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
        "challenge-creator",
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
}

/// Verifies admin review record validation rejects non-UTF-8 repository paths before the API request.
#[cfg(unix)]
#[tokio::test]
async fn challenge_creator_rejects_non_utf8_admin_repository_path() {
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
        OsString::from("challenge-creator"),
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
