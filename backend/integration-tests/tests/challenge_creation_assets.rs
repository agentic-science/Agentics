//! Challenge private asset upload and quota integration tests.

mod helpers;

use helpers::{
    TestCreatorSession, api_url, create_creator_session, spawn_app_with_config, test_config,
    zip_project_zip_base64,
};
use serde_json::json;
use shared::{
    db,
    error::AppError,
    models::{
        challenge_creation::ChallengePrivateAssetKind,
        hashes::Sha256Digest,
        ids::{
            AgentId, ChallengeDraftId, ChallengeDraftValidationRecordId, ChallengePrivateAssetId,
        },
        names::AssetName,
    },
    storage::StorageKey,
};

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
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let draft = create_draft(&client, &app, &creator, 9, manifest_json()).await;
    let draft_id = draft["id"].as_str().expect("draft id");

    let first_response = creator_auth(
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
    .expect("asset request");
    assert_eq!(first_response.status(), reqwest::StatusCode::CREATED);
    let first_asset: serde_json::Value = first_response.json().await.expect("asset json");
    let storage_key = first_asset["storage_key"]
        .as_str()
        .expect("storage key")
        .to_string();
    assert!(storage.path().join(&storage_key).exists());

    let duplicate_response = creator_auth(
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
    .expect("duplicate asset request");
    assert_eq!(duplicate_response.status(), reqwest::StatusCode::CONFLICT);
    assert!(
        storage.path().join(&storage_key).exists(),
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

/// Verifies active draft validation blocks asset mutation until its lease expires.
#[sqlx::test(migrations = "../migrations")]
async fn private_asset_upload_rejects_active_validation_and_recovers_stale_claim(
    pool: sqlx::PgPool,
) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let mut config = test_config(storage.path(), seeded_challenges.path());
    config.challenge_draft_validation_timeout_minutes = 30;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;

    let draft = create_draft(&client, &app, &creator, 11, manifest_json()).await;
    let draft_id =
        ChallengeDraftId::try_new(draft["id"].as_str().expect("draft id")).expect("valid draft id");
    let manifest_sha256 = Sha256Digest::try_new(
        draft["manifest_sha256"]
            .as_str()
            .expect("manifest sha should exist"),
    )
    .expect("manifest sha should parse");
    let validation_record_id = ChallengeDraftValidationRecordId::generate();
    db::begin_challenge_draft_validation(
        &pool,
        &db::BeginChallengeDraftValidationInput {
            validation_record_id: validation_record_id.clone(),
            draft_id: draft_id.clone(),
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
        upload_private_asset(&client, &app, &creator, draft_id.as_str(), 11).await;
    assert_eq!(active_response.status(), reqwest::StatusCode::CONFLICT);

    sqlx::query(
        "UPDATE challenge_draft_validation_records SET created_at = NOW() - INTERVAL '60 minutes' WHERE id = $1::uuid",
    )
    .bind(validation_record_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age validation claim");
    let recovered_response =
        upload_private_asset(&client, &app, &creator, draft_id.as_str(), 12).await;
    assert_eq!(recovered_response.status(), reqwest::StatusCode::CREATED);
    let active_validation: Option<String> = sqlx::query_scalar(
        "SELECT active_validation_record_id::text FROM challenge_drafts WHERE id = $1::uuid",
    )
    .bind(draft_id.as_str())
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
    let draft = create_draft(&client, &app, &creator, 10, manifest).await;
    let draft_id =
        ChallengeDraftId::try_new(draft["id"].as_str().expect("draft id")).expect("valid draft id");
    let uploader_agent_id = AgentId::try_new(&creator.agent_id).expect("valid creator agent id");

    let input_a = db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        draft_id: draft_id.clone(),
        asset_name: AssetName::try_new("official-cases-a".to_string())
            .expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: false,
        size_bytes: 8,
        sha256: Sha256Digest::try_new("a".repeat(64)).expect("test digest is valid"),
        storage_key: StorageKey::try_new("challenge-drafts/test/private-assets/a.bin")
            .expect("test storage key is valid"),
        temporary_storage_key: StorageKey::try_new("_tmp/challenge-private-assets/a.bin")
            .expect("test temporary storage key is valid"),
        uploader_agent_id: uploader_agent_id.clone(),
    };
    let input_b = db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        draft_id: draft_id.clone(),
        asset_name: AssetName::try_new("official-cases-b".to_string())
            .expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: false,
        size_bytes: 8,
        sha256: Sha256Digest::try_new("b".repeat(64)).expect("test digest is valid"),
        storage_key: StorageKey::try_new("challenge-drafts/test/private-assets/b.bin")
            .expect("test storage key is valid"),
        temporary_storage_key: StorageKey::try_new("_tmp/challenge-private-assets/b.bin")
            .expect("test temporary storage key is valid"),
        uploader_agent_id,
    };

    let create_a = db::reserve_challenge_private_asset(&pool, &input_a, 12, 30, 30);
    let create_b = db::reserve_challenge_private_asset(&pool, &input_b, 12, 30, 30);
    let (result_a, result_b) = tokio::join!(create_a, create_b);

    let mut created = 0;
    let mut rejected = 0;
    for result in [result_a, result_b] {
        match result {
            Ok(_) => created += 1,
            Err(AppError::TooManyRequests(message)) => {
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
        "SELECT COUNT(*)::BIGINT FROM challenge_private_assets WHERE draft_id = $1::uuid",
    )
    .bind(draft_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("asset count query");
    let stored_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes), 0)::BIGINT FROM challenge_private_assets WHERE draft_id = $1::uuid",
    )
    .bind(draft_id.as_str())
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

    let draft = create_draft(&client, &app, &creator, 12, manifest_json()).await;
    let draft_id =
        ChallengeDraftId::try_new(draft["id"].as_str().expect("draft id")).expect("valid draft id");
    let uploader_agent_id = AgentId::try_new(&creator.agent_id).expect("valid creator agent id");

    let first = private_asset_input(&draft_id, &uploader_agent_id, "official-cases", "first");
    db::reserve_challenge_private_asset(&pool, &first, 64, 30, 30)
        .await
        .expect("first pending asset should reserve");
    sqlx::query(
        "UPDATE challenge_private_assets SET created_at = NOW() - INTERVAL '60 minutes' WHERE id = $1::uuid",
    )
    .bind(first.asset_row_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age pending asset");

    let second = private_asset_input(&draft_id, &uploader_agent_id, "official-cases", "second");
    db::reserve_challenge_private_asset(&pool, &second, 64, 30, 30)
        .await
        .expect("stale pending asset should not block retry");

    let states: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM challenge_private_assets WHERE draft_id = $1::uuid ORDER BY created_at ASC",
    )
    .bind(draft_id.as_str())
    .fetch_all(&pool)
    .await
    .expect("failed to query asset states");
    assert_eq!(states, vec!["failed".to_string(), "pending".to_string()]);
}

/// Upload a declared private benchmark asset to a draft.
async fn upload_private_asset(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    draft_id: &str,
    nonce: i32,
) -> reqwest::Response {
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
        "asset_base64": private_benchmark_asset_zip_base64_with_nonce(nonce)
    }))
    .send()
    .await
    .expect("private asset request")
}

/// Build a private asset DB reservation input for direct admission tests.
fn private_asset_input(
    draft_id: &ChallengeDraftId,
    uploader_agent_id: &AgentId,
    asset_name: &str,
    key_suffix: &str,
) -> db::CreateChallengePrivateAssetInput {
    db::CreateChallengePrivateAssetInput {
        asset_row_id: ChallengePrivateAssetId::generate(),
        draft_id: draft_id.clone(),
        asset_name: AssetName::try_new(asset_name.to_string()).expect("test asset name is valid"),
        kind: ChallengePrivateAssetKind::PrivateBenchmarkData,
        required: false,
        size_bytes: 8,
        sha256: Sha256Digest::try_new("c".repeat(64)).expect("test digest is valid"),
        storage_key: StorageKey::try_new(format!(
            "challenge-drafts/test/private-assets/{key_suffix}.bin"
        ))
        .expect("test storage key is valid"),
        temporary_storage_key: StorageKey::try_new(format!(
            "_tmp/challenge-private-assets/{key_suffix}.bin"
        ))
        .expect("test temporary storage key is valid"),
        uploader_agent_id: uploader_agent_id.clone(),
    }
}

/// Create a draft for the public challenge creation test manifest.
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

/// Return the minimum challenge creation manifest used by asset tests.
fn manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "new_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
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
