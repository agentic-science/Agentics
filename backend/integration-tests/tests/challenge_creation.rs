//! Challenge creation draft lifecycle integration tests.

#[path = "support/challenge_creation.rs"]
mod challenge_creation_helpers;
mod helpers;

use agentics_domain::error::ServiceError;
use agentics_domain::models::challenge_creation::ChallengeDraftValidationStatus;
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::ids::{
    ChallengeDraftAuditEventId, ChallengeDraftId, ChallengeDraftValidationRecordId,
};
use agentics_domain::models::names::ChallengeName;
use agentics_domain::storage::StorageKey;
use agentics_storage::{StorageWriteIntent, build_storage, unpack_tar_to_directory};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use challenge_creation_helpers::*;
use helpers::{
    api_url, basic_auth_header, create_creator_session, spawn_app_with_config, test_config,
};
use serde_json::json;

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
        body["error"]["message"]
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
        &config.auth.admin_username,
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

    let repos = agentics_persistence::Repositories::new(&pool);
    repos
        .challenge_drafts()
        .begin_validation(
            &agentics_persistence::BeginChallengeDraftValidationInput {
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

    let overlapping = repos
        .challenge_drafts()
        .begin_validation(
            &agentics_persistence::BeginChallengeDraftValidationInput {
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
        matches!(
            overlapping,
            Err(agentics_domain::error::ServiceError::Conflict)
        ),
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
    repos
        .challenge_drafts()
        .finish_validation(
            &agentics_persistence::FinishChallengeDraftValidationInput {
                validation_record_id: first_validation_id,
                draft_id: draft_id.clone(),
                status: ChallengeDraftValidationStatus::Passed,
                message: "passed".to_string(),
                bundle_sha256: Some(validation_digest),
            },
            &agentics_persistence::CreateChallengeDraftAuditEventInput {
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
        &config.auth.admin_username,
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
    let published_challenge_name = published["published_challenge_name"]
        .as_str()
        .expect("published challenge name");
    let (bundle_key, public_bundle_key): (String, String) = sqlx::query_as(
        "SELECT bundle_key, public_bundle_key FROM challenges WHERE challenge_name = $1",
    )
    .bind("sample-sum")
    .fetch_one(&pool)
    .await
    .expect("bundle keys");
    let (_private_temp, private_dir) =
        materialize_bundle_key(&config, &bundle_key, "private").await;
    let (_public_temp, public_dir) =
        materialize_bundle_key(&config, &public_bundle_key, "public").await;
    assert!(
        private_dir.join("private-benchmark/runs.json").exists(),
        "publish should assemble a runtime bundle with uploaded private benchmark data"
    );
    assert!(
        !public_dir.join("private-benchmark/runs.json").exists(),
        "publish should also store a public-only bundle without private overlays"
    );

    let public_challenge: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{published_challenge_name}"),
        ))
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
    .bind(published_challenge_name)
    .bind(&creator.agent_id)
    .fetch_one(&pool)
    .await
    .expect("owner count");
    assert_eq!(owner_count, 1);

    let stats: serde_json::Value = creator_auth(
        client.get(api_url(
            &app,
            &format!(
                "/api/creator/challenges/{published_challenge_name}/stats?target=linux-arm64-cpu"
            ),
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
            &format!("/api/creator/challenges/{published_challenge_name}/participants?target=linux-arm64-cpu"),
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
        client.get(api_url(
            &app,
            &format!("/api/creator/challenges/{published_challenge_name}/stats"),
        )),
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
        &config.auth.admin_username,
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
        "SELECT COUNT(*)::BIGINT FROM challenges WHERE challenge_name = $1 AND spec_json IS NOT NULL",
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
        &config.auth.admin_username,
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
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM challenges WHERE challenge_name = $1")
            .bind("sample-sum")
            .fetch_one(&pool)
            .await
            .expect("challenge count");
    assert_eq!(challenge_count, 1);
    let bundle_key: String =
        sqlx::query_scalar("SELECT bundle_key FROM challenges WHERE challenge_name = $1")
            .bind("sample-sum")
            .fetch_one(&pool)
            .await
            .expect("bundle key");
    let (_bundle_temp, bundle_dir) =
        materialize_bundle_key(&config, &bundle_key, "concurrent-private").await;
    assert!(
        bundle_dir.join("private-benchmark/runs.json").exists(),
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
        &config.auth.admin_username,
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

    let existing_bundle_key = StorageKey::try_new("challenge-bundles/sample-sum/existing.tar")
        .expect("existing bundle key");
    let existing_public_bundle_key =
        StorageKey::try_new("challenge-public-bundles/sample-sum/existing.tar")
            .expect("existing public bundle key");
    let existing_statement_key = StorageKey::try_new("challenge-statements/sample-sum/existing.md")
        .expect("existing statement key");
    let storage_backend = build_storage(&config).await.expect("storage backend");
    storage_backend
        .put(
            &existing_statement_key,
            b"# Existing\n",
            StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
        )
        .await
        .expect("existing statement should store");
    let existing_challenge_name =
        ChallengeName::try_new("sample-sum".to_string()).expect("sample-sum name is valid");
    sqlx::query(
        r#"
        INSERT INTO challenges (
            challenge_name, title, summary, bundle_key, public_bundle_key, statement_key, spec_json, starts_at, status
        )
        VALUES (
            $3,
            'Existing Sample Sum',
            '{"en":"Existing","zh":"Existing"}'::jsonb,
            $1,
            $4,
            $2,
            '{"already":"published"}'::jsonb,
            '2026-01-01T00:00:00Z'::timestamptz,
            'active'
        )
        "#,
    )
    .bind(existing_bundle_key.as_str())
    .bind(existing_statement_key.as_str())
    .bind(existing_challenge_name.as_str())
    .bind(existing_public_bundle_key.as_str())
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

    assert!(
        helpers::storage_prefix_is_empty(
            &config,
            &format!("challenge-bundles/sample-sum/{draft_id}")
        )
        .await,
        "failed DB publish must remove the claim-scoped runtime bundle"
    );
    assert!(
        helpers::storage_prefix_is_empty(&config, "_tmp/challenge-bundles").await,
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

    let repos = agentics_persistence::Repositories::new(&pool);
    let first = repos
        .challenge_drafts()
        .claim_for_publish(draft_id, 30)
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

    let second = repos
        .challenge_drafts()
        .claim_for_publish(draft_id, 30)
        .await
        .expect("second publish claim should reserve after stale reset");
    let second_claim = second
        .publish_claim_id
        .expect("second publish claim id should exist");
    assert_ne!(first_claim, second_claim);

    let stale_fail = repos
        .challenge_drafts()
        .fail_publish(draft_id, &first_claim, "stale failure")
        .await
        .expect_err("stale claim should not fail newer publish");
    assert!(matches!(stale_fail, ServiceError::Conflict));

    let stale_complete = repos
        .challenge_drafts()
        .mark_published(draft_id, &first_claim, None)
        .await
        .expect_err("stale claim should not complete newer publish");
    assert!(matches!(stale_complete, ServiceError::Conflict));

    let claim_after_stale: Option<String> = sqlx::query_scalar(
        "SELECT publish_claim_id::text FROM challenge_drafts WHERE id = $1::uuid",
    )
    .bind(draft_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query publish claim");
    assert_eq!(claim_after_stale.as_deref(), Some(second_claim.as_str()));

    repos
        .challenge_drafts()
        .mark_published(draft_id, &second_claim, None)
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

/// Downloads and unpacks a stored challenge bundle archive for filesystem assertions.
async fn materialize_bundle_key(
    config: &agentics_config::Config,
    bundle_key: &str,
    label: &str,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let storage_backend = build_storage(config).await.expect("storage backend");
    let materialized = tempfile::tempdir().expect("materialized bundle tempdir");
    let archive = materialized.path().join(format!("{label}.tar"));
    storage_backend
        .get_to_file(
            &StorageKey::try_new(bundle_key).expect("valid bundle key"),
            &archive,
            StorageWriteIntent::new(
                "challenge bundle archive",
                config.storage.max_bundle_archive_bytes,
            ),
        )
        .await
        .expect("download challenge bundle");
    let bundle_dir = materialized.path().join("bundle");
    unpack_tar_to_directory(&archive, &bundle_dir)
        .await
        .expect("unpack challenge bundle");
    (materialized, bundle_dir)
}

#[path = "challenge_creation/lifecycle.rs"]
mod lifecycle;
