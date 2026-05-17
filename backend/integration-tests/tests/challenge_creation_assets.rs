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
        ids::{AgentId, ChallengeDraftId, ChallengePrivateAssetId},
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
    zip_project_zip_base64(vec![(
        "private-benchmark/runs.json",
        json!({
            "runs": [
                {
                    "run_name": "official-case-1",
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
        uploader_agent_id,
    };

    let create_a = db::create_challenge_private_asset(&pool, &input_a, 12);
    let create_b = db::create_challenge_private_asset(&pool, &input_b, 12);
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
