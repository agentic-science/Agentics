//! Challenge creation draft lifecycle integration tests.

mod helpers;

use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use helpers::{
    TestCreatorSession, api_url, basic_auth_header, create_creator_session, sample_sum_solution,
    solution_zip_base64, spawn_app_with_config, test_config, zip_project_zip_base64,
};
use serde_json::json;

use reqwest::StatusCode;

fn creator_auth(
    request: reqwest::RequestBuilder,
    creator: &TestCreatorSession,
) -> reqwest::RequestBuilder {
    request
        .header("Cookie", &creator.cookie_header)
        .header("X-Agentics-CSRF-Token", &creator.csrf_token)
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_rejects_short_commit_sha(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let response = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 7,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "commit_sha": "0123456789abcdef",
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": 1001,
        "manifest": manifest_json()
    }))
    .send()
    .await
    .expect("draft request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.expect("error json");
    assert!(
        body["message"]
            .as_str()
            .expect("error message")
            .contains("commit_sha must be a full")
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_conflicts_on_canonical_repo_key(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let _draft = create_draft(&client, &app, &creator, 7, manifest_json()).await;

    let response = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "git@github.com:agentics-reifying/agentics-challenges.git",
        "pr_number": 7,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "commit_sha": "0123456789abcdef0123456789abcdef00000008",
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": 1001,
        "manifest": manifest_json()
    }))
    .send()
    .await
    .expect("duplicate canonical repo draft request");

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_can_be_validated_approved_and_published(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    let draft: serde_json::Value = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 7,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "commit_sha": "0123456789abcdef0123456789abcdef01234567",
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": 1001,
        "manifest": manifest_json()
    }))
    .send()
    .await
    .expect("draft request")
    .error_for_status()
    .expect("draft should create")
    .json()
    .await
    .expect("draft json");
    assert_eq!(draft["status"], "draft");
    let draft_id = draft["id"].as_str().expect("draft id");

    let asset: serde_json::Value = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("asset request")
    .error_for_status()
    .expect("asset should upload")
    .json()
    .await
    .expect("asset json");
    assert_eq!(asset["required"], true);
    assert!(asset["size_bytes"].as_i64().expect("asset size") > 2);

    let validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate")
        .json()
        .await
        .expect("validated json");
    assert_eq!(validated["status"], "validated");
    assert_eq!(
        validated["validation_bundle_sha256"]
            .as_str()
            .expect("validation digest")
            .len(),
        64
    );
    assert_eq!(
        validated["validation_records"][0]["status"], "passed",
        "validation record should be persisted"
    );
    assert_eq!(
        validated["validation_records"][0]["bundle_sha256"]
            .as_str()
            .expect("validation record digest")
            .len(),
        64
    );

    let approved: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "message": "looks good" }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve")
        .json()
        .await
        .expect("approve json");
    assert_eq!(
        approved["approved_bundle_sha256"],
        validated["validation_bundle_sha256"]
    );

    let upload_after_approval = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("post-approval asset request");
    assert_eq!(
        upload_after_approval.status(),
        reqwest::StatusCode::CONFLICT
    );

    let validate_after_approval = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("post-approval validate request");
    assert_eq!(
        validate_after_approval.status(),
        reqwest::StatusCode::CONFLICT
    );

    let published: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("publish request")
        .error_for_status()
        .expect("draft should publish")
        .json()
        .await
        .expect("published json");
    assert_eq!(published["status"], "published");
    assert_eq!(published["published_challenge_name"], "sample-sum");
    let bundle_path: String =
        sqlx::query_scalar("SELECT bundle_path FROM challenges WHERE name = $1")
            .bind("sample-sum")
            .fetch_one(&pool)
            .await
            .expect("bundle path");
    assert!(
        std::path::Path::new(&bundle_path)
            .join("private-benchmark/runs.json")
            .exists(),
        "publish should assemble a runtime bundle with uploaded private benchmark data"
    );

    let public_challenge: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges/sample-sum"))
        .send()
        .await
        .expect("public challenge request")
        .error_for_status()
        .expect("published challenge should be visible")
        .json()
        .await
        .expect("public challenge json");
    assert_eq!(public_challenge["spec"]["eligibility"]["type"], "open");

    let owner_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM challenge_owners WHERE challenge_name = $1 AND agent_id = $2::uuid",
    )
    .bind("sample-sum")
    .bind(&creator.agent_id)
    .fetch_one(&pool)
    .await
    .expect("owner count");
    assert_eq!(owner_count, 1);

    let stats: serde_json::Value = creator_auth(
        client.get(api_url(
            &app,
            "/api/creator/challenges/sample-sum/stats?target=linux-arm64-cpu",
        )),
        &creator,
    )
    .send()
    .await
    .expect("creator stats request")
    .error_for_status()
    .expect("creator stats should be readable")
    .json()
    .await
    .expect("creator stats json");
    assert_eq!(stats["challenge_name"], "sample-sum");
    assert_eq!(stats["target"], "linux-arm64-cpu");
    assert_eq!(stats["solution_submission_count"], 0);

    let participants: serde_json::Value = creator_auth(
        client.get(api_url(
            &app,
            "/api/creator/challenges/sample-sum/participants?target=linux-arm64-cpu",
        )),
        &creator,
    )
    .send()
    .await
    .expect("creator participants request")
    .error_for_status()
    .expect("creator participants should be readable")
    .json()
    .await
    .expect("creator participants json");
    assert_eq!(participants["items"].as_array().expect("items").len(), 0);

    let non_owner = create_creator_session(&pool, 1002, "not-owner").await;
    let non_owner_stats = creator_auth(
        client.get(api_url(&app, "/api/creator/challenges/sample-sum/stats")),
        &non_owner,
    )
    .send()
    .await
    .expect("non-owner stats request");
    assert_eq!(non_owner_stats.status(), reqwest::StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../migrations")]
async fn approved_draft_publish_rejects_changed_review_content(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    let draft = create_draft(&client, &app, &creator, 17, manifest_json()).await;
    let draft_id = draft["id"].as_str().expect("draft id");

    creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("private asset request")
    .error_for_status()
    .expect("private asset should upload");

    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate");
    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "message": "approved" }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve");

    write_file(
        &public_repo
            .path()
            .join("challenges/sample-sum/v1/statement.md"),
        "# Sample Sum\n\nChanged after approval.\n",
    );

    let publish_response = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("publish request");
    assert_eq!(publish_response.status(), reqwest::StatusCode::BAD_REQUEST);

    let published_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM challenges WHERE name = $1 AND spec_json IS NOT NULL",
    )
    .bind("sample-sum")
    .fetch_one(&pool)
    .await
    .expect("published count");
    assert_eq!(published_count, 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_rejects_new_version_manifest(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let response = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 22,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/22",
        "commit_sha": "0123456789abcde220123456789abcde22012345",
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": 1001,
        "manifest": {
            "schema_version": 1,
            "request": "new_version",
            "challenge_name": "sample-sum",
            "title": "Sample Sum",
            "summary": "Add numbers",
            "readme_path": "README.md",
            "version": {
                "version": "v2",
                "bundle_path": "v2",
                "supersedes_version": "v1"
            }
        }
    }))
    .send()
    .await
    .expect("new_version draft request");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "new_version manifests are no longer accepted"
    )
}

#[sqlx::test(migrations = "../migrations")]
async fn archive_draft_hides_challenge_and_rejects_new_submissions(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let participant_token = register_agent(&pool, "participant-agent").await;
    let participant_bearer = format!("Bearer {participant_token}");
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    create_validate_approve_publish_draft(
        &client,
        &app,
        &creator,
        &admin_auth,
        public_repo.path(),
        31,
        manifest_json(),
    )
    .await;

    write_archive_manifest(public_repo.path());
    create_validate_approve_publish_draft(
        &client,
        &app,
        &creator,
        &admin_auth,
        public_repo.path(),
        32,
        archive_manifest_json(),
    )
    .await;

    let list: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges"))
        .send()
        .await
        .expect("challenge list")
        .error_for_status()
        .expect("list should succeed")
        .json()
        .await
        .expect("list json");
    assert!(
        list["items"]
            .as_array()
            .expect("items")
            .iter()
            .all(|item| item["name"] != "sample-sum")
    );

    client
        .get(api_url(&app, "/api/public/challenges/sample-sum"))
        .send()
        .await
        .expect("archived detail")
        .error_for_status()
        .expect("archived direct detail should remain readable");

    let response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", participant_bearer)
        .json(&json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"))
        }))
        .send()
        .await
        .expect("submission request");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_rejects_mismatched_pr_author(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let response = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 8,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/8",
        "commit_sha": "0123456789abcdef0123456789abcdef01234567",
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": 2002,
        "manifest": manifest_json()
    }))
    .send()
    .await
    .expect("draft request");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_creator_routes_require_oauth_session_and_csrf(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let unauthenticated = client
        .post(api_url(&app, "/api/creator/challenge-drafts"))
        .json(&json!({
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": 8,
            "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/8",
            "commit_sha": "0123456789abcdef0123456789abcdef01234567",
            "challenge_path": "challenges/sample-sum",
            "pr_author_github_user_id": 1001,
            "manifest": manifest_json()
        }))
        .send()
        .await
        .expect("draft request without session");
    assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);

    let missing_csrf = client
        .post(api_url(&app, "/api/creator/challenge-drafts"))
        .header("Cookie", &creator.cookie_header)
        .json(&json!({
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": 8,
            "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/8",
            "commit_sha": "0123456789abcdef0123456789abcdef01234567",
            "challenge_path": "challenges/sample-sum",
            "pr_author_github_user_id": 1001,
            "manifest": manifest_json()
        }))
        .send()
        .await
        .expect("draft request without csrf");
    assert_eq!(missing_csrf.status(), reqwest::StatusCode::FORBIDDEN);

    let old_self_link_route = client
        .post(api_url(&app, "/api/challenge-creator/github-identity"))
        .header("Authorization", "Bearer self-asserted-token")
        .json(&json!({
            "github_user_id": 1001,
            "github_login": "creator"
        }))
        .send()
        .await
        .expect("old identity link request");
    assert_eq!(old_self_link_route.status(), reqwest::StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_creation_quotas_reject_excess_work(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    write_public_challenge(public_repo.path());

    let mut config = test_config(storage.path(), seeded_challenges.path());
    config.max_active_challenge_drafts_per_agent = 1;
    config.challenge_draft_validations_per_day = 1;
    config.challenge_private_asset_bytes_per_draft = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    let draft: serde_json::Value = create_draft(&client, &app, &creator, 41, manifest_json()).await;
    let draft_id = draft["id"].as_str().expect("draft id");

    let quota_response = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 42,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/42",
        "commit_sha": "0123456789abcde420123456789abcde42012345",
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": 1001,
        "manifest": manifest_json()
    }))
    .send()
    .await
    .expect("draft quota request");
    assert_eq!(
        quota_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );

    let asset_response = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "asset_base64": STANDARD.encode(b"[]")
    }))
    .send()
    .await
    .expect("asset quota request");
    assert_eq!(asset_response.status(), reqwest::StatusCode::BAD_REQUEST);

    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("first validation")
        .error_for_status()
        .expect("first validation should pass");
    let validation_quota_response = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("second validation");
    assert_eq!(
        validation_quota_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn cleanup_purges_abandoned_draft_private_assets(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let mut config = test_config(storage.path(), seeded_challenges.path());
    config.unpublished_challenge_asset_grace_days = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    let draft = create_draft(&client, &app, &creator, 51, manifest_json()).await;
    let draft_id = draft["id"].as_str().expect("draft id");

    let asset: serde_json::Value = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "asset_base64": STANDARD.encode(b"private")
    }))
    .send()
    .await
    .expect("asset upload")
    .error_for_status()
    .expect("asset should upload")
    .json()
    .await
    .expect("asset json");
    let storage_key = asset["storage_key"]
        .as_str()
        .expect("storage key")
        .to_string();
    assert!(storage.path().join(&storage_key).exists());

    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/abandon"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "message": "closed PR" }))
        .send()
        .await
        .expect("abandon")
        .error_for_status()
        .expect("abandon should succeed");
    sqlx::query(
        "UPDATE challenge_drafts SET updated_at = NOW() - INTERVAL '2 days' WHERE id = $1::uuid",
    )
    .bind(draft_id)
    .execute(&pool)
    .await
    .expect("age draft");

    let cleanup: serde_json::Value = client
        .post(api_url(&app, "/admin/challenge-drafts/cleanup"))
        .header("Authorization", &admin_auth)
        .send()
        .await
        .expect("cleanup")
        .error_for_status()
        .expect("cleanup should succeed")
        .json()
        .await
        .expect("cleanup json");
    assert_eq!(cleanup["purged_private_assets"], 1);
    assert!(!storage.path().join(&storage_key).exists());
}

async fn create_validate_approve_publish_draft(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    admin_auth: &str,
    public_repo: &Path,
    pr_number: i32,
    manifest: serde_json::Value,
) -> serde_json::Value {
    let draft = create_draft(client, app, creator, pr_number, manifest).await;
    let draft_id = draft["id"].as_str().expect("draft id");
    if draft["request"] != "archive_challenge" {
        creator_auth(
            client.post(api_url(
                app,
                &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
            )),
            creator,
        )
        .json(&json!({
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "asset_base64": private_benchmark_asset_zip_base64()
        }))
        .send()
        .await
        .expect("private asset request")
        .error_for_status()
        .expect("private asset should upload");
    }

    client
        .post(api_url(
            app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", admin_auth)
        .json(&json!({ "repository_path": public_repo.to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate");
    client
        .post(api_url(
            app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", admin_auth)
        .json(&json!({ "message": "approved" }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve");
    client
        .post(api_url(
            app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", admin_auth)
        .json(&json!({ "repository_path": public_repo.to_string_lossy() }))
        .send()
        .await
        .expect("publish request")
        .error_for_status()
        .expect("draft should publish")
        .json()
        .await
        .expect("publish json")
}

async fn create_draft(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    pr_number: i32,
    manifest: serde_json::Value,
) -> serde_json::Value {
    creator_auth(
        client.post(api_url(app, "/api/creator/challenge-drafts")),
        creator,
    )
        .json(&json!({
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": pr_number,
            "pr_url": format!("https://github.com/agentics-reifying/agentics-challenges/pull/{pr_number}"),
            "commit_sha": format!("0123456789abcdef0123456789abcdef{pr_number:08x}"),
            "challenge_path": "challenges/sample-sum",
            "pr_author_github_user_id": 1001,
            "manifest": manifest
        }))
        .send()
        .await
        .expect("draft request")
        .error_for_status()
        .expect("draft should create")
        .json()
        .await
        .expect("draft json")
}

async fn register_agent(pool: &sqlx::PgPool, name: &str) -> String {
    let token = shared::auth::create_agent_token();
    let token_hash = shared::auth::hash_agent_token(&token);
    shared::db::register_agent(
        pool,
        &shared::db::RegisterAgentInput {
            agent_id: uuid::Uuid::new_v4().to_string(),
            token_id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            display_name: name.to_string(),
            agent_description: String::new(),
            owner: String::new(),
            model_info: json!({}),
        },
    )
    .await
    .expect("agent should register");
    token
}

fn write_public_challenge(repo: &Path) {
    let challenge_root = repo.join("challenges/sample-sum");
    std::fs::create_dir_all(challenge_root.join("v1/public")).expect("public dir");
    write_file(&challenge_root.join("README.md"), "# Sample Sum\n");
    write_file(&challenge_root.join("v1/statement.md"), "# Sample Sum\n");
    write_file(
        &challenge_root.join("v1/public/runs.json"),
        &json!({
            "runs": [
                {
                    "run_name": "case-1",
                    "interface": "stdio",
                    "stdin_json": { "a": 1, "b": 2 },
                    "expected": "3",
                    "output_files": []
                }
            ]
        })
        .to_string(),
    );
    write_file(&challenge_root.join("v1/scorer/run.py"), SAMPLE_SUM_SCORER);
    write_file(
        &challenge_root.join("v1/spec.json"),
        &json!({
            "schema_version": 1,
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "challenge_summary": "Add numbers",
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
                    "validation_enabled": true,
                    "resource_profile": {
                        "name": "agentics-cpu-small",
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
            "eligibility": { "type": "open" },
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_live"
            },
            "solution_publication": "public",
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
                    }
                ],
                "ranking": {
                    "primary_metric_name": "score"
                }
            }
        })
        .to_string(),
    );
    write_file(
        &challenge_root.join("agentics.challenge.json"),
        &manifest_json().to_string(),
    );
}

fn write_archive_manifest(repo: &Path) {
    let challenge_root = repo.join("challenges/sample-sum");
    write_file(
        &challenge_root.join("agentics.challenge.json"),
        &archive_manifest_json().to_string(),
    );
}

fn manifest_json() -> serde_json::Value {
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

fn archive_manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "archive_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": "Add numbers",
        "readme_path": "README.md",
        "archive": {
            "reason": "Retired for MVP lifecycle testing"
        }
    })
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, content).expect("write file");
}

fn private_benchmark_asset_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "private-benchmark/runs.json",
            json!({
                "runs": [
                    {
                        "run_name": "private-benchmark-1",
                        "interface": "stdio",
                        "stdin_json": { "a": 20, "b": 22 },
                        "expected": "42",
                        "output_files": []
                    }
                ]
            })
            .to_string(),
        ),
        (
            "private-benchmark/cases.json",
            json!({ "cases": [{ "case_name": "private-benchmark-1" }] }).to_string(),
        ),
    ])
}

const SAMPLE_SUM_SCORER: &str = r#"from __future__ import annotations

import argparse
import json
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--solution-runs-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["validation", "official"], required=True)
    parser.add_argument("--runs-file", required=True)
    parser.add_argument("--challenge-dir", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    runs = json.loads(Path(args.runs_file).read_text(encoding="utf-8"))["runs"]
    results = []
    for run in runs:
        stdout = (Path(args.solution_runs_dir) / run["run_name"] / "stdout.txt").read_text(encoding="utf-8").strip()
        passed = stdout == str(run["expected"])
        results.append({"case_name": run["run_name"], "status": "passed" if passed else "failed", "score": 1 if passed else 0})
    passed_count = sum(1 for result in results if result["status"] == "passed")
    total = len(results)
    score = 0 if total == 0 else passed_count / total
    payload = {
        "status": "passed" if passed_count == total else "failed",
        "mode": args.mode,
        "primary_score": score,
        "rank_score": score,
        "aggregate_metrics": [{"metric_name": "score", "value": score}],
        "run_metrics": [{"run_name": result["case_name"], "metrics": [{"metric_name": "score", "value": result["score"]}]} for result in results],
        "public_results": results if args.mode == "validation" else [],
    }
    if args.mode == "validation":
        payload["validation_summary"] = {"score": score, "passed": passed_count, "total": total}
    else:
        payload["official_summary"] = {"score": score, "passed": passed_count, "total": total}
    Path(args.output_path).write_text(json.dumps(payload), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#;
