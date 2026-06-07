//! Challenge private asset upload and quota integration tests.

mod helpers;

use agentics_domain::models::{
    challenge_creation::ChallengePrivateAssetKind,
    hashes::Sha256Digest,
    ids::{
        ChallengePrivateAssetId, ChallengeReviewRecordId, ChallengeReviewValidationRecordId,
        HumanId,
    },
    names::AssetName,
};
use agentics_error::ServiceError;
use agentics_persistence as db;
use agentics_storage::StorageKey;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use helpers::{
    TestCreatorSession, admin_service_token_header, api_url, create_creator_session,
    put_storage_key, read_storage_key, spawn_app_with_config, storage_key_exists, test_config,
    zip_project_zip_base64,
};
use serde_json::json;

/// Add creator session headers to a request builder.
fn creator_auth(
    request: reqwest::RequestBuilder,
    creator: &TestCreatorSession,
) -> reqwest::RequestBuilder {
    request
        .header("Cookie", &creator.cookie_header)
        .header("X-Agentics-CSRF-Token", &creator.csrf_token)
}

/// Verifies that private asset upload rejects duplicate asset name.
#[sqlx::test(migrations = "../migrations")]
async fn private_asset_upload_rejects_duplicate_asset_name(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let review_record = create_review_record(&client, &app, &creator, 9, manifest_json()).await;
    let review_record_id = review_record["id"].as_str().expect("review_record id");

    let first_response = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-review-records/{review_record_id}/private-assets"),
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
    .expect("asset request");
    assert_eq!(first_response.status(), reqwest::StatusCode::CREATED);
    let first_asset: serde_json::Value = first_response.json().await.expect("asset json");
    let storage_key = first_asset["storage_key"]
        .as_str()
        .expect("storage key")
        .to_string();
    assert!(storage_key_exists(&config, &storage_key).await);

    let duplicate_response = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-review-records/{review_record_id}/private-assets"),
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
    .expect("duplicate asset request");
    assert_eq!(duplicate_response.status(), reqwest::StatusCode::CONFLICT);
    assert!(
        storage_key_exists(&config, &storage_key).await,
        "duplicate rejection must not delete the accepted durable asset"
    );
}

/// Build a small valid private benchmark ZIP overlay for upload tests.
fn private_benchmark_asset_zip_base64() -> String {
    private_benchmark_asset_zip_base64_with_nonce(1)
}

/// Build a private benchmark ZIP overlay with a unique run name for retry tests.
fn private_benchmark_asset_zip_base64_with_nonce(nonce: i32) -> String {
    zip_project_zip_base64(vec![(
        "private-benchmark/runs.json",
        json!({
            "runs": [
                {
                    "run_name": format!("official-case-{nonce}"),
                    "interface": "stdio",
                    "stdin_json": { "a": 1, "b": 2 },
                    "expected": "3",
                    "output_files": []
                }
            ]
        })
        .to_string(),
    )])
}

/// Verifies active review_record validation blocks asset mutation until its lease expires.
#[sqlx::test(migrations = "../migrations")]
async fn private_asset_upload_rejects_active_validation_and_recovers_stale_claim(
    pool: sqlx::PgPool,
) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let mut config = test_config(storage.path(), seeded_challenges.path());
    config
        .quotas
        .challenge_review_record_validation_timeout_minutes = 30;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let review_record = create_review_record(&client, &app, &creator, 11, manifest_json()).await;
    let review_record_id =
        ChallengeReviewRecordId::try_new(review_record["id"].as_str().expect("review_record id"))
            .expect("valid review_record id");
    let manifest_sha256 = Sha256Digest::try_new(
        review_record["manifest_sha256"]
            .as_str()
            .expect("manifest sha should exist"),
    )
    .expect("manifest sha should parse");
    let validation_record_id = ChallengeReviewValidationRecordId::generate();
    db::Repositories::new(&pool)
        .challenge_review_records()
        .begin_validation(
            &db::BeginChallengeReviewRecordValidationInput {
                validation_record_id: validation_record_id.clone(),
                review_record_id: review_record_id.clone(),
                repository_path: storage.path().to_string_lossy().to_string(),
                manifest_sha256,
            },
            24 * 60 * 60,
            10,
            30,
        )
        .await
        .expect("validation claim should reserve");

    let active_response =
        upload_private_asset(&client, &app, &creator, review_record_id.as_str(), 11).await;
    assert_eq!(active_response.status(), reqwest::StatusCode::CONFLICT);

    sqlx::query(
        "UPDATE challenge_review_validation_records SET created_at = NOW() - INTERVAL '60 minutes' WHERE id = $1::uuid",
    )
    .bind(validation_record_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age validation claim");
    let recovered_response =
        upload_private_asset(&client, &app, &creator, review_record_id.as_str(), 12).await;
    assert_eq!(recovered_response.status(), reqwest::StatusCode::CREATED);
    let active_validation: Option<String> = sqlx::query_scalar(
        "SELECT active_validation_record_id::text FROM challenge_review_records WHERE id = $1::uuid",
    )
    .bind(review_record_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to query active validation");
    assert!(active_validation.is_none());
}

/// Verifies that private asset quota admission serializes concurrent inserts.
#[sqlx::test(migrations = "../migrations")]
async fn private_asset_quota_admission_serializes_concurrent_inserts(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let mut manifest = manifest_json();
    manifest["private_assets"] = json!([
        {
            "asset_name": "official-cases-a",
            "kind": "private_benchmark_data",
            "required": false
        },
        {
            "asset_name": "official-cases-b",
            "kind": "private_benchmark_data",
            "required": false
        }
    ]);
    let review_record = create_review_record(&client, &app, &creator, 10, manifest).await;
    let review_record_id =
        ChallengeReviewRecordId::try_new(review_record["id"].as_str().expect("review_record id"))
            .expect("valid review_record id");
    let uploader_human_id = HumanId::try_new(&creator.human_id).expect("valid creator human id");

    let input_a = db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        review_record_id: review_record_id.clone(),
        asset_name: AssetName::try_new("official-cases-a".to_string())
            .expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: false,
        size_bytes: 8,
        sha256: Sha256Digest::try_new("a".repeat(64)).expect("test digest is valid"),
        storage_key: StorageKey::try_new("challenge-review-records/test/private-assets/a.bin")
            .expect("test storage key is valid"),
        temporary_storage_key: StorageKey::try_new("_tmp/challenge-private-assets/a.bin")
            .expect("test temporary storage key is valid"),
        uploader_human_id: uploader_human_id.clone(),
    };
    let input_b = db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        review_record_id: review_record_id.clone(),
        asset_name: AssetName::try_new("official-cases-b".to_string())
            .expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: false,
        size_bytes: 8,
        sha256: Sha256Digest::try_new("b".repeat(64)).expect("test digest is valid"),
        storage_key: StorageKey::try_new("challenge-review-records/test/private-assets/b.bin")
            .expect("test storage key is valid"),
        temporary_storage_key: StorageKey::try_new("_tmp/challenge-private-assets/b.bin")
            .expect("test temporary storage key is valid"),
        uploader_human_id,
    };

    let repos_a = db::Repositories::new(&pool);
    let repos_b = db::Repositories::new(&pool);
    let drafts_a = repos_a.challenge_review_records();
    let drafts_b = repos_b.challenge_review_records();
    let create_a = drafts_a.reserve_private_asset(&input_a, 12, 30, 30);
    let create_b = drafts_b.reserve_private_asset(&input_b, 12, 30, 30);
    let (result_a, result_b) = tokio::join!(create_a, create_b);

    let mut created = 0;
    let mut rejected = 0;
    for result in [result_a, result_b] {
        match result {
            Ok(_) => created += 1,
            Err(ServiceError::TooManyRequests(message)) => {
                assert!(
                    message.contains("private asset quota exceeded"),
                    "unexpected quota message: {message}"
                );
                rejected += 1;
            }
            Err(error) => panic!("unexpected private asset admission error: {error:?}"),
        }
    }
    assert_eq!(created, 1);
    assert_eq!(rejected, 1);

    let stored_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM challenge_private_assets WHERE review_record_id = $1::uuid",
    )
    .bind(review_record_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("asset count query");
    let stored_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes), 0)::BIGINT FROM challenge_private_assets WHERE review_record_id = $1::uuid",
    )
    .bind(review_record_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("asset byte query");
    assert_eq!(stored_count, 1);
    assert_eq!(stored_bytes, 8);
}

/// Verifies stale pending asset reservations are failed before a retry reserves the same name.
#[sqlx::test(migrations = "../migrations")]
async fn stale_pending_private_asset_reservation_can_be_retried(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let review_record = create_review_record(&client, &app, &creator, 12, manifest_json()).await;
    let review_record_id =
        ChallengeReviewRecordId::try_new(review_record["id"].as_str().expect("review_record id"))
            .expect("valid review_record id");
    let uploader_human_id = HumanId::try_new(&creator.human_id).expect("valid creator human id");

    let first = private_asset_input(
        &review_record_id,
        &uploader_human_id,
        "official-cases",
        "first",
    );
    let repos = db::Repositories::new(&pool);
    repos
        .challenge_review_records()
        .reserve_private_asset(&first, 64, 30, 30)
        .await
        .expect("first pending asset should reserve");
    sqlx::query(
        "UPDATE challenge_private_assets SET created_at = NOW() - INTERVAL '60 minutes' WHERE id = $1::uuid",
    )
    .bind(first.asset_row_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age pending asset");

    let second = private_asset_input(
        &review_record_id,
        &uploader_human_id,
        "official-cases",
        "second",
    );
    repos
        .challenge_review_records()
        .reserve_private_asset(&second, 64, 30, 30)
        .await
        .expect("stale pending asset should not block retry");

    let states: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM challenge_private_assets WHERE review_record_id = $1::uuid ORDER BY created_at ASC",
    )
    .bind(review_record_id.as_str())
    .fetch_all(&pool)
    .await
    .expect("failed to query asset states");
    assert_eq!(states, vec!["failed".to_string(), "pending".to_string()]);
}

/// Verifies exact retries repair stale pending rows whose durable object was already promoted.
#[sqlx::test(migrations = "../migrations")]
async fn stale_pending_private_asset_retry_replaces_unreferenced_object(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let admin_auth = admin_service_token_header(&app);
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let review_record = create_review_record(&client, &app, &creator, 14, manifest_json()).await;
    let review_record_id =
        ChallengeReviewRecordId::try_new(review_record["id"].as_str().expect("review_record id"))
            .expect("valid review_record id");
    let uploader_human_id = HumanId::try_new(&creator.human_id).expect("valid creator human id");
    let asset_base64 = private_benchmark_asset_zip_base64();
    let asset_bytes = STANDARD
        .decode(&asset_base64)
        .expect("test asset base64 should decode");
    let sha256 = agentics_contracts::challenge_creation::sha256_digest(&asset_bytes);
    let storage_key = StorageKey::try_new(format!(
        "challenge-review-records/{}/private-assets/official-cases-{}.bin",
        review_record_id, sha256
    ))
    .expect("deterministic private asset storage key should be valid");
    let first = db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        review_record_id: review_record_id.clone(),
        asset_name: AssetName::try_new("official-cases".to_string())
            .expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: true,
        size_bytes: i64::try_from(asset_bytes.len()).expect("test asset size fits"),
        sha256,
        storage_key: storage_key.clone(),
        temporary_storage_key: StorageKey::try_new("_tmp/challenge-private-assets/stale.bin")
            .expect("test temporary storage key is valid"),
        uploader_human_id,
    };
    db::Repositories::new(&pool)
        .challenge_review_records()
        .reserve_private_asset(&first, 10_000_000, 30, 30)
        .await
        .expect("first pending asset should reserve");
    put_storage_key(&config, &storage_key, &asset_bytes).await;
    sqlx::query(
        "UPDATE challenge_private_assets SET created_at = NOW() - INTERVAL '60 minutes' WHERE id = $1::uuid",
    )
    .bind(first.asset_row_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age pending asset");

    let response = creator_auth(
        client.post(api_url(
            &app,
            &format!(
                "/api/creator/challenge-review-records/{}/private-assets",
                review_record_id.as_str()
            ),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "required": false,
        "asset_base64": asset_base64
    }))
    .send()
    .await
    .expect("private asset retry request");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let asset: serde_json::Value = response.json().await.expect("asset json");
    assert_eq!(
        asset["storage_key"].as_str().expect("storage key"),
        storage_key.as_str()
    );

    let states: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM challenge_private_assets WHERE review_record_id = $1::uuid ORDER BY created_at ASC",
    )
    .bind(review_record_id.as_str())
    .fetch_all(&pool)
    .await
    .expect("failed to query asset states");
    assert_eq!(states, vec!["failed".to_string(), "active".to_string()]);
    assert_eq!(
        read_storage_key(
            &config,
            storage_key.as_str(),
            agentics_storage::StorageWriteIntent::new(
                "private benchmark asset",
                u64::try_from(asset_bytes.len()).expect("test asset size fits u64"),
            ),
        )
        .await,
        asset_bytes
    );

    let admin_assets: serde_json::Value = client
        .get(api_url(
            &app,
            &format!(
                "/admin/challenge-review-records/{}/private-assets",
                review_record_id.as_str()
            ),
        ))
        .header("Authorization", &admin_auth)
        .send()
        .await
        .expect("admin private asset lifecycle request")
        .error_for_status()
        .expect("admin should read private asset lifecycle")
        .json()
        .await
        .expect("admin private asset lifecycle json");
    let items = admin_assets["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["status"], "failed");
    assert_eq!(
        items[0]["failure_message"],
        "private asset pending lease expired"
    );
    assert_eq!(items[1]["status"], "active");
    assert_eq!(
        items[1]["storage_key"].as_str().expect("storage key"),
        storage_key.as_str()
    );
}

/// Verifies private asset lifecycle work refreshes the parent review_record activity.
#[sqlx::test(migrations = "../migrations")]
async fn private_asset_lifecycle_refreshes_draft_activity(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let mut manifest = manifest_json();
    manifest["private_assets"] = json!([
        {
            "asset_name": "official-cases-a",
            "kind": "private_benchmark_data",
            "required": false
        },
        {
            "asset_name": "official-cases-b",
            "kind": "private_benchmark_data",
            "required": false
        }
    ]);
    let review_record = create_review_record(&client, &app, &creator, 13, manifest).await;
    let review_record_id =
        ChallengeReviewRecordId::try_new(review_record["id"].as_str().expect("review_record id"))
            .expect("valid review_record id");
    let uploader_human_id = HumanId::try_new(&creator.human_id).expect("valid creator human id");

    age_draft_for_cleanup(&pool, &review_record_id).await;
    let input_a = private_asset_input(
        &review_record_id,
        &uploader_human_id,
        "official-cases-a",
        "first",
    );
    let repos = db::Repositories::new(&pool);
    repos
        .challenge_review_records()
        .reserve_private_asset(&input_a, 64, 30, 30)
        .await
        .expect("pending asset should reserve");
    assert_draft_survives_stale_cleanup(&pool, &review_record_id).await;

    age_draft_for_cleanup(&pool, &review_record_id).await;
    repos
        .challenge_review_records()
        .activate_private_asset(&input_a.asset_row_id)
        .await
        .expect("pending asset should activate");
    assert_draft_survives_stale_cleanup(&pool, &review_record_id).await;

    let input_b = private_asset_input(
        &review_record_id,
        &uploader_human_id,
        "official-cases-b",
        "second",
    );
    repos
        .challenge_review_records()
        .reserve_private_asset(&input_b, 64, 30, 30)
        .await
        .expect("second pending asset should reserve");
    age_draft_for_cleanup(&pool, &review_record_id).await;
    repos
        .challenge_review_records()
        .fail_private_asset(&input_b.asset_row_id, "test failure")
        .await
        .expect("pending asset should fail");
    assert_draft_survives_stale_cleanup(&pool, &review_record_id).await;

    age_draft_for_cleanup(&pool, &review_record_id).await;
    repos
        .challenge_review_records()
        .delete_private_asset(&input_a.asset_row_id)
        .await
        .expect("active asset should delete");
    assert_draft_survives_stale_cleanup(&pool, &review_record_id).await;
}

/// Verifies stale cleanup preserves explicit reviewer rejection outcomes.
#[sqlx::test(migrations = "../migrations")]
async fn stale_cleanup_preserves_rejected_draft_review_outcome(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let review_record = create_review_record(&client, &app, &creator, 14, manifest_json()).await;
    let review_record_id =
        ChallengeReviewRecordId::try_new(review_record["id"].as_str().expect("review_record id"))
            .expect("valid review_record id");

    sqlx::query(
        r#"
        UPDATE challenge_review_records
        SET status = 'rejected',
            validation_message = 'reviewer rejected this review_record',
            updated_at = NOW() - INTERVAL '2 days'
        WHERE id = $1::uuid
        "#,
    )
    .bind(review_record_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to reject and age review_record");

    let abandoned = db::Repositories::new(&pool)
        .challenge_review_records()
        .abandon_stale(1)
        .await
        .expect("stale cleanup should run");
    assert_eq!(abandoned, 0);

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT status, validation_message FROM challenge_review_records WHERE id = $1::uuid",
    )
    .bind(review_record_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to query review_record status");
    assert_eq!(row.0, "rejected");
    assert_eq!(
        row.1.as_deref(),
        Some("reviewer rejected this review_record"),
        "stale cleanup must not erase review feedback"
    );
}

/// Upload a declared private benchmark asset to a review_record.
async fn upload_private_asset(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    review_record_id: &str,
    nonce: i32,
) -> reqwest::Response {
    creator_auth(
        client.post(api_url(
            app,
            &format!("/api/creator/challenge-review-records/{review_record_id}/private-assets"),
        )),
        creator,
    )
    .json(&json!({
        "asset_name": "official-cases",
        "kind": "private_benchmark_data",
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64_with_nonce(nonce)
    }))
    .send()
    .await
    .expect("private asset request")
}

/// Age a review_record enough that stale cleanup would abandon it without a later activity write.
async fn age_draft_for_cleanup(pool: &sqlx::PgPool, review_record_id: &ChallengeReviewRecordId) {
    sqlx::query(
        "UPDATE challenge_review_records SET updated_at = NOW() - INTERVAL '2 days' WHERE id = $1::uuid",
    )
    .bind(review_record_id.as_str())
    .execute(pool)
    .await
    .expect("failed to age review_record");
}

/// Run stale cleanup and verify the review_record remained active.
async fn assert_draft_survives_stale_cleanup(
    pool: &sqlx::PgPool,
    review_record_id: &ChallengeReviewRecordId,
) {
    db::Repositories::new(pool)
        .challenge_review_records()
        .abandon_stale(1)
        .await
        .expect("stale cleanup should run");
    let status: String =
        sqlx::query_scalar("SELECT status FROM challenge_review_records WHERE id = $1::uuid")
            .bind(review_record_id.as_str())
            .fetch_one(pool)
            .await
            .expect("failed to query review_record status");
    assert_eq!(status, "pending_review");
}

/// Build a private asset DB reservation input for direct admission tests.
fn private_asset_input(
    review_record_id: &ChallengeReviewRecordId,
    uploader_human_id: &HumanId,
    asset_name: &str,
    key_suffix: &str,
) -> db::CreateChallengePrivateAssetInput {
    db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        review_record_id: review_record_id.clone(),
        asset_name: AssetName::try_new(asset_name.to_string()).expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: false,
        size_bytes: 8,
        sha256: Sha256Digest::try_new("c".repeat(64)).expect("test digest is valid"),
        storage_key: StorageKey::try_new(format!(
            "challenge-review-records/test/private-assets/{key_suffix}.bin"
        ))
        .expect("test storage key is valid"),
        temporary_storage_key: StorageKey::try_new(format!(
            "_tmp/challenge-private-assets/{key_suffix}.bin"
        ))
        .expect("test temporary storage key is valid"),
        uploader_human_id: uploader_human_id.clone(),
    }
}

/// Create a review_record for the public challenge creation test manifest.
async fn create_review_record(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    pr_number: i32,
    manifest: serde_json::Value,
) -> serde_json::Value {
    creator_auth(
        client.post(api_url(app, "/api/creator/challenge-review-records")),
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
    .expect("review_record request")
    .error_for_status()
    .expect("review_record should create")
    .json()
    .await
    .expect("review_record json")
}

/// Return the minimum challenge creation manifest used by asset tests.
fn manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "new_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
        "keywords": ["math"],
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
