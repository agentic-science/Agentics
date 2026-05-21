//! Challenge creation draft lifecycle integration tests.

#[path = "support/challenge_creation.rs"]
mod challenge_creation_helpers;
mod helpers;

use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use challenge_creation_helpers::*;
use helpers::{
    api_url, basic_auth_header, create_creator_session, sample_sum_solution, solution_zip_base64,
    spawn_app_with_config, test_config,
};
use serde_json::json;
use shared::error::AppError;
use shared::models::challenge_creation::ChallengeDraftValidationStatus;
use shared::models::hashes::Sha256Digest;
use shared::models::ids::{
    ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftValidationRecordId,
};

use reqwest::StatusCode;

/// Verifies that challenge draft rejects short commit sha.
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

/// Verifies that challenge draft conflicts on canonical repo key.
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

/// Verifies that private assets are rejected before non-ZIP bytes reach durable storage.
#[sqlx::test(migrations = "../migrations")]
async fn private_asset_upload_rejects_non_zip_payload(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let draft = create_draft(&client, &app, &creator, 8, manifest_json()).await;
    let draft_id = draft["id"].as_str().expect("draft id");

    let missing_required_response = creator_auth(
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
    .expect("missing-required private asset request");
    assert_eq!(missing_required_response.status(), StatusCode::BAD_REQUEST);
    let missing_required_error = missing_required_response
        .text()
        .await
        .expect("missing-required error body");
    assert!(
        missing_required_error.contains("required"),
        "expected missing required field error, got: {missing_required_error}"
    );

    let response = creator_auth(
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
        "asset_base64": STANDARD.encode("not a zip")
    }))
    .send()
    .await
    .expect("non-zip private asset request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let stored_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM challenge_private_assets WHERE draft_id = $1::uuid",
    )
    .bind(draft_id)
    .fetch_one(&pool)
    .await
    .expect("private asset count query should succeed");
    assert_eq!(stored_count, 0);
}

/// Verifies that draft validation records must own the current draft validation claim.
#[sqlx::test(migrations = "../migrations")]
async fn draft_validation_claim_blocks_overlap_and_approval(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let repository = tempfile::tempdir().expect("repository tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );
    let draft = create_draft(&client, &app, &creator, 9, manifest_json()).await;
    let draft_id = ChallengeDraftId::try_new(draft["id"].as_str().expect("draft id"))
        .expect("draft id should parse");
    let manifest_sha256 = Sha256Digest::try_new(
        draft["manifest_sha256"]
            .as_str()
            .expect("manifest sha should exist"),
    )
    .expect("manifest sha should parse");
    let first_validation_id = ChallengeDraftValidationRecordId::generate();
    let second_validation_id = ChallengeDraftValidationRecordId::generate();

    shared::db::begin_challenge_draft_validation(
        &pool,
        &shared::db::BeginChallengeDraftValidationInput {
            validation_record_id: first_validation_id.clone(),
            draft_id: draft_id.clone(),
            repository_path: repository.path().to_string_lossy().to_string(),
            manifest_sha256,
        },
        24 * 60 * 60,
        10,
        30,
    )
    .await
    .expect("first validation claim should reserve");

    let overlapping = shared::db::begin_challenge_draft_validation(
        &pool,
        &shared::db::BeginChallengeDraftValidationInput {
            validation_record_id: second_validation_id.clone(),
            draft_id: draft_id.clone(),
            repository_path: repository.path().to_string_lossy().to_string(),
            manifest_sha256,
        },
        24 * 60 * 60,
        10,
        30,
    )
    .await;
    assert!(
        matches!(overlapping, Err(shared::error::AppError::Conflict)),
        "overlapping validation should conflict"
    );

    let approve_while_running = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "too early",
            "expected_validation_bundle_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }))
        .send()
        .await
        .expect("approve while validation is running");
    assert_eq!(approve_while_running.status(), StatusCode::CONFLICT);

    let reject_while_running = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/reject"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "message": "wait for validation" }))
        .send()
        .await
        .expect("reject while validation is running");
    assert_eq!(reject_while_running.status(), StatusCode::CONFLICT);

    let abandon_while_running = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/abandon"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "message": "wait for validation" }))
        .send()
        .await
        .expect("abandon while validation is running");
    assert_eq!(abandon_while_running.status(), StatusCode::CONFLICT);

    let validation_digest =
        Sha256Digest::try_new("b".repeat(64)).expect("validation digest should parse");
    shared::db::finish_challenge_draft_validation(
        &pool,
        &shared::db::FinishChallengeDraftValidationInput {
            validation_record_id: first_validation_id,
            draft_id: draft_id.clone(),
            status: ChallengeDraftValidationStatus::Passed,
            message: "passed".to_string(),
            bundle_sha256: Some(validation_digest),
        },
        &shared::db::CreateChallengeDraftAuditEventInput {
            event_id: ChallengeDraftAuditEventId::generate(),
            draft_id,
            actor_agent_id: None,
            actor_admin_username: Some("admin".to_string()),
            action: "draft_validated".to_string(),
            message: "passed".to_string(),
            metadata: json!({}),
        },
    )
    .await
    .expect("current validation claim should finish");
}

/// Verifies that challenge draft can be validated approved and published.
#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_can_be_validated_approved_and_published(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let draft: serde_json::Value = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": 7,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
        "commit_sha": commit_sha,
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
        .header("X-Agentics-Admin-Automation", "true")
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
    assert!(
        validated["validation_repository_path"].is_string(),
        "admin validation response should keep checkout path"
    );
    assert!(
        validated["validation_records"][0]["repository_path"].is_string(),
        "admin validation records should keep checkout path"
    );

    let creator_visible_draft: serde_json::Value = creator_auth(
        client.get(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}"),
        )),
        &creator,
    )
    .send()
    .await
    .expect("creator draft detail request")
    .error_for_status()
    .expect("creator draft detail should be visible")
    .json()
    .await
    .expect("creator draft detail json");
    assert!(
        creator_visible_draft
            .get("validation_repository_path")
            .is_none(),
        "creator draft detail must not expose validation checkout path"
    );
    assert!(
        creator_visible_draft["validation_records"][0]
            .get("repository_path")
            .is_none(),
        "creator validation records must not expose checkout paths"
    );

    let approved: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "looks good",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
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
        .header("X-Agentics-Admin-Automation", "true")
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
        .header("X-Agentics-Admin-Automation", "true")
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

/// Verifies that approved draft publish rejects changed review content.
#[sqlx::test(migrations = "../migrations")]
async fn approved_draft_publish_rejects_changed_review_content(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let draft =
        create_draft_with_commit(&client, &app, &creator, 17, manifest_json(), &commit_sha).await;
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
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("private asset request")
    .error_for_status()
    .expect("private asset should upload");

    let validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate")
        .json()
        .await
        .expect("validated draft json");
    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "approved",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
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
        .header("X-Agentics-Admin-Automation", "true")
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

/// Verifies concurrent publish requests do not corrupt the final runtime bundle.
#[sqlx::test(migrations = "../migrations")]
async fn concurrent_publish_requests_leave_one_published_bundle(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let draft =
        create_draft_with_commit(&client, &app, &creator, 23, manifest_json(), &commit_sha).await;
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
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("private asset request")
    .error_for_status()
    .expect("private asset should upload");
    let validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate")
        .json()
        .await
        .expect("validated draft json");
    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "approved",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve");

    let publish_url = api_url(&app, &format!("/admin/challenge-drafts/{draft_id}/publish"));
    let repository_path = public_repo.path().to_string_lossy().to_string();
    let publish_a = client
        .post(publish_url.clone())
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": repository_path }));
    let publish_b = client
        .post(publish_url)
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": repository_path }));

    let (response_a, response_b) = tokio::join!(publish_a.send(), publish_b.send());
    let statuses = [
        response_a.expect("first publish request").status(),
        response_b.expect("second publish request").status(),
    ];
    assert!(
        statuses.contains(&reqwest::StatusCode::OK),
        "one concurrent publish should succeed, got {statuses:?}"
    );
    assert!(
        statuses.iter().all(|status| matches!(
            *status,
            reqwest::StatusCode::OK | reqwest::StatusCode::CONFLICT
        )),
        "concurrent publish should either succeed or observe the active publish claim: {statuses:?}"
    );

    let challenge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM challenges WHERE name = $1")
            .bind("sample-sum")
            .fetch_one(&pool)
            .await
            .expect("challenge count");
    assert_eq!(challenge_count, 1);
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
        "published bundle should include promoted private benchmark data"
    );
    let draft_status: String =
        sqlx::query_scalar("SELECT status FROM challenge_drafts WHERE id = $1::uuid")
            .bind(draft_id)
            .fetch_one(&pool)
            .await
            .expect("draft status");
    assert_eq!(draft_status, "published");
}

/// Verifies DB publish conflicts clean up the runtime bundle produced by that publish claim.
#[sqlx::test(migrations = "../migrations")]
async fn failed_publish_removes_claim_scoped_runtime_bundle(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let draft =
        create_draft_with_commit(&client, &app, &creator, 24, manifest_json(), &commit_sha).await;
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
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("private asset request")
    .error_for_status()
    .expect("private asset should upload");
    let validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate")
        .json()
        .await
        .expect("validated draft json");
    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "approved",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve");

    let existing_bundle = storage.path().join("existing-bundle");
    std::fs::create_dir_all(&existing_bundle).expect("existing bundle dir");
    let existing_statement = existing_bundle.join("statement.md");
    std::fs::write(&existing_statement, "# Existing\n").expect("existing statement");
    sqlx::query(
        r#"
        INSERT INTO challenges (
            name, title, summary, bundle_path, statement_path, spec_json, starts_at, status
        )
        VALUES (
            'sample-sum',
            'Existing Sample Sum',
            '{"en":"Existing","zh":"Existing"}'::jsonb,
            $1,
            $2,
            '{"already":"published"}'::jsonb,
            '2026-01-01T00:00:00Z'::timestamptz,
            'active'
        )
        "#,
    )
    .bind(existing_bundle.to_string_lossy().to_string())
    .bind(existing_statement.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("existing active challenge should insert");

    let response = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("publish request");
    assert_eq!(response.status(), reqwest::StatusCode::CONFLICT);

    let (draft_status, publish_claim_id): (String, Option<String>) = sqlx::query_as(
        "SELECT status, publish_claim_id::text FROM challenge_drafts WHERE id = $1::uuid",
    )
    .bind(draft_id)
    .fetch_one(&pool)
    .await
    .expect("draft status after failed publish");
    assert_eq!(draft_status, "approved");
    assert!(publish_claim_id.is_none());

    let draft_bundle_root = storage
        .path()
        .join("challenge-bundles")
        .join("sample-sum")
        .join(draft_id);
    assert!(
        directory_is_empty_or_absent(&draft_bundle_root),
        "failed DB publish must remove the claim-scoped runtime bundle"
    );
    assert!(
        directory_is_empty_or_absent(&storage.path().join("_tmp").join("challenge-bundles")),
        "failed DB publish must remove temporary runtime bundle directories"
    );
}

/// Verifies stale publish claims cannot complete or fail a newer publish attempt.
#[sqlx::test(migrations = "../migrations")]
async fn stale_publish_claim_cannot_mutate_newer_publish_claim(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let draft = create_draft(&client, &app, &creator, 34, manifest_json()).await;
    let draft_id = draft["id"].as_str().expect("draft id");
    sqlx::query(
        r#"
        UPDATE challenge_drafts
        SET status = 'approved',
            approved_bundle_sha256 = manifest_sha256,
            validation_repository_path = $2
        WHERE id = $1::uuid
        "#,
    )
    .bind(draft_id)
    .bind(public_repo.path().to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("failed to approve draft directly");

    let first = shared::db::claim_challenge_draft_for_publish(&pool, draft_id, 30)
        .await
        .expect("first publish claim should reserve");
    let first_claim = first
        .publish_claim_id
        .expect("first publish claim id should exist");
    sqlx::query(
        "UPDATE challenge_drafts SET updated_at = NOW() - INTERVAL '60 minutes' WHERE id = $1::uuid",
    )
    .bind(draft_id)
    .execute(&pool)
    .await
    .expect("failed to age publish claim");

    let second = shared::db::claim_challenge_draft_for_publish(&pool, draft_id, 30)
        .await
        .expect("second publish claim should reserve after stale reset");
    let second_claim = second
        .publish_claim_id
        .expect("second publish claim id should exist");
    assert_ne!(first_claim, second_claim);

    let stale_fail =
        shared::db::fail_challenge_draft_publish(&pool, draft_id, &first_claim, "stale failure")
            .await
            .expect_err("stale claim should not fail newer publish");
    assert!(matches!(stale_fail, AppError::Conflict));

    let stale_complete =
        shared::db::mark_challenge_draft_published(&pool, draft_id, &first_claim, None)
            .await
            .expect_err("stale claim should not complete newer publish");
    assert!(matches!(stale_complete, AppError::Conflict));

    let claim_after_stale: Option<String> = sqlx::query_scalar(
        "SELECT publish_claim_id::text FROM challenge_drafts WHERE id = $1::uuid",
    )
    .bind(draft_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query publish claim");
    assert_eq!(claim_after_stale.as_deref(), Some(second_claim.as_str()));

    shared::db::mark_challenge_draft_published(&pool, draft_id, &second_claim, None)
        .await
        .expect("newer claim should complete publish");
    let status_and_claim: (String, Option<String>) = sqlx::query_as(
        "SELECT status, publish_claim_id::text FROM challenge_drafts WHERE id = $1::uuid",
    )
    .bind(draft_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query published draft");
    assert_eq!(status_and_claim, ("published".to_string(), None));
}

/// Returns true for a missing or empty directory, and panics on other filesystem errors.
fn directory_is_empty_or_absent(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => panic!("failed to inspect {}: {error}", path.display()),
    }
}

/// Verifies that challenge draft rejects new version manifest.
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
            "summary": { "en": "Add numbers", "zh": "数字求和" },
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

/// Verifies that archive draft hides challenge and rejects new submissions.
#[sqlx::test(migrations = "../migrations")]
async fn archive_draft_hides_challenge_and_rejects_new_submissions(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let participant_token = register_agent(&pool, "participant-agent").await;
    let participant_bearer = format!("Bearer {participant_token}");
    let participant_agent_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM agents WHERE display_name = $1")
            .bind("participant-agent")
            .fetch_one(&pool)
            .await
            .expect("participant agent id");
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let publish_flow = DraftPublishFlow {
        client: &client,
        app: &app,
        creator: &creator,
        admin_auth: &admin_auth,
        public_repo: public_repo.path(),
    };
    create_validate_approve_publish_draft(&publish_flow, &commit_sha, 31, manifest_json()).await;

    let archived_submission_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO solution_submissions (
            id, challenge_name, target, agent_id, artifact_key, status,
            explanation, credit_text, visible_after_eval, note
        )
        VALUES ($1, 'sample-sum', 'linux-arm64-cpu', $2, $3, 'completed',
                'archived public surface probe', '', TRUE, '')
        "#,
    )
    .bind(archived_submission_id)
    .bind(participant_agent_id)
    .bind(format!("solution-submissions/{archived_submission_id}.zip"))
    .execute(&pool)
    .await
    .expect("seed visible submission");
    sqlx::query(
        r#"
        INSERT INTO leaderboard_entries (
            challenge_name, target, agent_id, best_solution_submission_id,
            best_rank_score, public_results_json, aggregate_metrics_json,
            official_metrics_json
        )
        VALUES (
            'sample-sum', 'linux-arm64-cpu', $1, $2,
            0.95, '[]'::jsonb, $3, $3
        )
        "#,
    )
    .bind(participant_agent_id)
    .bind(archived_submission_id)
    .bind(json!([{ "metric_name": "score", "value": 0.95 }]))
    .execute(&pool)
    .await
    .expect("seed leaderboard entry");

    write_archive_manifest(public_repo.path());
    let archive_commit_sha = commit_all(public_repo.path(), "archive sample-sum");
    create_validate_approve_publish_draft(
        &publish_flow,
        &archive_commit_sha,
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

    let leaderboard: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/leaderboard?target=linux-arm64-cpu",
        ))
        .send()
        .await
        .expect("archived leaderboard")
        .error_for_status()
        .expect("archived leaderboard should remain readable")
        .json()
        .await
        .expect("leaderboard json");
    let archived_submission_id_string = archived_submission_id.to_string();
    assert_eq!(
        leaderboard["items"][0]["best_solution_submission_id"].as_str(),
        Some(archived_submission_id_string.as_str())
    );

    let ranking_context: serde_json::Value = client
        .get(api_url(
            &app,
            &format!(
                "/api/public/solution-submissions/{archived_submission_id}/ranking-context?challenge_name=sample-sum&target=linux-arm64-cpu"
            ),
        ))
        .send()
        .await
        .expect("archived ranking context")
        .error_for_status()
        .expect("archived ranking context should remain readable")
        .json()
        .await
        .expect("ranking context json");
    assert_eq!(ranking_context["rank"], 1);

    let distribution: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/score-distributions?target=linux-arm64-cpu&metric=score",
        ))
        .send()
        .await
        .expect("archived score distribution")
        .error_for_status()
        .expect("archived score distribution should remain readable")
        .json()
        .await
        .expect("score distribution json");
    assert_eq!(distribution["count"], 1);

    let response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", participant_bearer)
        .header("X-Agentics-Admin-Automation", "true")
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

/// Verifies that archive publication requires the draft creator to own the challenge.
#[sqlx::test(migrations = "../migrations")]
async fn archive_draft_requires_challenge_owner(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let owner = create_creator_session(&pool, 1001, "owner").await;
    let non_owner = create_creator_session(&pool, 1002, "non-owner").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let publish_flow = DraftPublishFlow {
        client: &client,
        app: &app,
        creator: &owner,
        admin_auth: &admin_auth,
        public_repo: public_repo.path(),
    };
    create_validate_approve_publish_draft(&publish_flow, &commit_sha, 61, manifest_json()).await;

    write_archive_manifest(public_repo.path());
    let archive_commit_sha = commit_all(public_repo.path(), "archive sample-sum");
    let archive_draft = create_draft_with_author_and_commit(
        &client,
        &app,
        &non_owner,
        62,
        archive_manifest_json(),
        1002,
        &archive_commit_sha,
    )
    .await;
    let archive_draft_id = archive_draft["id"].as_str().expect("archive draft id");
    let archive_validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{archive_draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate archive request")
        .error_for_status()
        .expect("archive draft should validate")
        .json()
        .await
        .expect("validated archive draft json");
    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{archive_draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "approved",
            "expected_validation_bundle_sha256": archive_validated["validation_bundle_sha256"]
        }))
        .send()
        .await
        .expect("approve archive request")
        .error_for_status()
        .expect("archive draft should approve");

    let publish = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{archive_draft_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("publish archive request");
    assert_eq!(publish.status(), StatusCode::FORBIDDEN);
}

/// Verifies that challenge draft rejects mismatched pr author.
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

/// Verifies that challenge creator routes require oauth session and csrf.
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
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "github_user_id": 1001,
            "github_login": "creator"
        }))
        .send()
        .await
        .expect("old identity link request");
    assert_eq!(old_self_link_route.status(), reqwest::StatusCode::NOT_FOUND);
}

/// Verifies that challenge creation quotas reject excess work.
#[sqlx::test(migrations = "../migrations")]
async fn challenge_creation_quotas_reject_excess_work(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    let commit_sha = write_public_challenge(public_repo.path());

    let mut config = test_config(storage.path(), seeded_challenges.path());
    config.max_active_challenge_drafts_per_agent = 1;
    config.challenge_draft_validations_per_day = 1;
    config.challenge_private_asset_bytes_per_draft = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let draft: serde_json::Value =
        create_draft_with_commit(&client, &app, &creator, 41, manifest_json(), &commit_sha).await;
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
        "required": false,
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
        .header("X-Agentics-Admin-Automation", "true")
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
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("second validation");
    assert_eq!(
        validation_quota_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
}

/// Verifies that cleanup purges abandoned draft private assets.
#[sqlx::test(migrations = "../migrations")]
async fn cleanup_purges_abandoned_draft_private_assets(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let mut config = test_config(storage.path(), seeded_challenges.path());
    config.unpublished_challenge_asset_grace_days = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

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
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
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
        .header("X-Agentics-Admin-Automation", "true")
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
        .header("X-Agentics-Admin-Automation", "true")
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
