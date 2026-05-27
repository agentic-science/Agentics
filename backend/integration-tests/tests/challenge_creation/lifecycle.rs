use super::helpers::{sample_sum_solution, solution_zip_base64};
use super::*;

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
        &config.auth.admin_username,
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
    let sample_sum_name: String = sqlx::query_scalar(
        "SELECT challenge_name::text FROM challenges WHERE challenge_name = 'sample-sum'",
    )
    .fetch_one(&pool)
    .await
    .expect("published sample-sum name");

    let archived_submission_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO solution_submissions (
            id, challenge_name, target, agent_id, artifact_key, status,
            explanation, credit_text, visible_after_eval, note
        )
        VALUES ($1, $2, 'linux-arm64-cpu', $3, $4, 'completed',
                'archived public surface probe', '', TRUE, '')
        "#,
    )
    .bind(archived_submission_id)
    .bind(&sample_sum_name)
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
            $1, 'linux-arm64-cpu', $2, $3,
            0.95, '[]'::jsonb, $4, $4
        )
        "#,
    )
    .bind(&sample_sum_name)
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
            .all(|item| item["challenge_name"] != "sample-sum")
    );

    let sample_sum_name: String = sqlx::query_scalar(
        "SELECT challenge_name::text FROM challenges WHERE challenge_name = 'sample-sum'",
    )
    .fetch_one(&pool)
    .await
    .expect("sample-sum challenge name");

    client
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{sample_sum_name}"),
        ))
        .send()
        .await
        .expect("archived detail")
        .error_for_status()
        .expect("archived direct detail should remain readable");

    let leaderboard: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{sample_sum_name}/leaderboard?target=linux-arm64-cpu"),
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
                "/api/public/solution-submissions/{archived_submission_id}/ranking-context?challenge_name={sample_sum_name}&target=linux-arm64-cpu"
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
            &format!("/api/public/challenges/{sample_sum_name}/score-distributions?target=linux-arm64-cpu&metric=score"),
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
            "challenge_name": sample_sum_name,
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
        &config.auth.admin_username,
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
    write_public_challenge(public_repo.path());
    let mut manifest = manifest_json();
    manifest["private_assets"]
        .as_array_mut()
        .expect("private assets array")
        .push(json!({
            "asset_name": "extra-cases",
            "kind": "private_reference_outputs",
            "required": false
        }));
    write_file(
        &public_repo
            .path()
            .join("challenges/sample-sum/agentics.challenge.json"),
        &manifest.to_string(),
    );
    let commit_sha = commit_all(public_repo.path(), "add optional asset");
    let valid_asset_base64 = private_benchmark_asset_zip_base64();
    let valid_asset_len = u64::try_from(
        STANDARD
            .decode(&valid_asset_base64)
            .expect("valid asset base64")
            .len(),
    )
    .expect("asset length fits u64");

    let mut config = test_config(storage.path(), seeded_challenges.path());
    config.quotas.max_active_challenge_drafts_per_agent = 1;
    config.quotas.challenge_draft_validations_per_day = 1;
    config.quotas.challenge_private_asset_bytes_per_draft = valid_asset_len;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.auth.admin_username,
        config.expose_admin_password_for_http_basic(),
    );

    let draft: serde_json::Value =
        create_draft_with_commit(&client, &app, &creator, 41, manifest.clone(), &commit_sha).await;
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
        "manifest": manifest
    }))
    .send()
    .await
    .expect("draft quota request");
    assert_eq!(
        quota_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );

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
        "asset_base64": valid_asset_base64
    }))
    .send()
    .await
    .expect("required asset request")
    .error_for_status()
    .expect("required asset should upload");

    let asset_response = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&json!({
        "asset_name": "extra-cases",
        "kind": "private_reference_outputs",
        "required": false,
        "asset_base64": private_benchmark_asset_zip_base64()
    }))
    .send()
    .await
    .expect("asset quota request");
    assert_eq!(
        asset_response.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );

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
    config.quotas.unpublished_challenge_asset_grace_days = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let creator = create_creator_session(&pool, 1001, "creator").await;
    let admin_auth = basic_auth_header(
        &config.auth.admin_username,
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
    assert!(helpers::storage_key_exists(&config, &storage_key).await);

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
    assert!(cleanup["purged_temporary_storage_objects"].is_i64());
    assert!(!helpers::storage_key_exists(&config, &storage_key).await);
}
