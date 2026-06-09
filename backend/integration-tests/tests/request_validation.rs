//! Integration tests for strict request validation and error response shape.

mod helpers;

use std::path::Path;

use helpers::{
    TestCreatorSession, admin_service_token_header, api_url, copy_dir_all, create_creator_session,
    examples_challenges_root, published_challenge_name, run_worker_once, sample_sum_solution,
    solution_zip_base64, spawn_app, spawn_app_with_config, test_config, zip_project_zip_base64,
};

/// Handles creator auth for this module.
fn creator_auth(
    request: reqwest::RequestBuilder,
    creator: &TestCreatorSession,
) -> reqwest::RequestBuilder {
    request
        .header("Cookie", &creator.cookie_header)
        .header("X-Agentics-CSRF-Token", &creator.csrf_token)
}

/// Verifies that request validation returns contract error shape.
#[sqlx::test(migrations = "../migrations")]
async fn request_validation_returns_contract_error_shape(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;
    let client = reqwest::Client::new();

    let empty_display_name = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "   " }))
        .send()
        .await
        .expect("failed to send empty-display-name request");
    assert_eq!(
        empty_display_name.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    let empty_display_name_body: serde_json::Value = empty_display_name
        .json()
        .await
        .expect("failed to decode empty-display-name response");
    assert_eq!(
        empty_display_name_body["error"]["code"],
        "validation_failed"
    );
    assert!(
        empty_display_name_body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("display_name")
    );
    assert_eq!(
        empty_display_name_body["error"]["details"][0]["field"],
        "display_name"
    );
    assert_eq!(
        empty_display_name_body["error"]["details"][0]["message"],
        "must not be empty"
    );

    let unknown_field = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "display_name": "strict-agent",
            "unexpected": true
        }))
        .send()
        .await
        .expect("failed to send unknown-field request");
    assert_eq!(
        unknown_field.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    let unknown_field_body: serde_json::Value = unknown_field
        .json()
        .await
        .expect("failed to decode unknown-field response");
    assert_eq!(unknown_field_body["error"]["code"], "validation_failed");

    let invalid_challenge_name = client
        .get(api_url(&app, "/api/public/challenges/bad%20id"))
        .send()
        .await
        .expect("failed to send invalid challenge name request");
    assert_eq!(invalid_challenge_name.status(), 400);
    let invalid_challenge_name_body: serde_json::Value = invalid_challenge_name
        .json()
        .await
        .expect("failed to decode invalid challenge name response");
    assert_eq!(invalid_challenge_name_body["error"]["code"], "bad_request");
    assert!(
        invalid_challenge_name_body["error"]["message"]
            .as_str()
            .expect("message should be a string")
            .contains("challenge_name")
    );
}

/// Verifies admin mutations reject invalid service tokens.
#[sqlx::test(migrations = "../migrations")]
async fn admin_mutation_rejects_invalid_service_token(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = helpers::test_config(storage.path(), &helpers::examples_challenges_root());
    let app = helpers::spawn_app_with_config(pool, config).await;
    let client = reqwest::Client::new();

    let response = client
        .post(helpers::api_url(
            &app,
            "/admin/challenge-review-records/cleanup",
        ))
        .header("Authorization", "Bearer agentics_admin_missing")
        .send()
        .await
        .expect("failed to send admin mutation");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json().await.expect("failed to decode error");
    assert_eq!(body["error"]["code"], "unauthorized");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("message should be a string")
            .contains("admin service token")
    );
}

/// Verifies that zip submission routes accept declared large json bodies.
#[sqlx::test(migrations = "../migrations")]
async fn zip_submission_routes_accept_declared_large_json_bodies(pool: sqlx::PgPool) {
    let app = helpers::spawn_app(pool).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(helpers::api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "large-body-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let oversized_for_axum_default = vec![0_u8; 3 * 1024 * 1024];
    let artifact_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        oversized_for_axum_default,
    );

    let response = client
        .post(helpers::api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64
        }))
        .send()
        .await
        .expect("failed to submit large request body");

    assert_ne!(
        response.status(),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE,
        "route-specific body limit should exceed Axum's small JSON default"
    );
}

/// Verifies that solution submission rejects invalid target before artifact decode.
#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_invalid_target_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = helpers::test_config(storage.path(), &helpers::examples_challenges_root());
    let app = helpers::spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(helpers::api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "invalid-target-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;

    let malformed_response = client
        .post(helpers::api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "target": "linux arm64",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send malformed-target submission");
    assert_eq!(malformed_response.status(), 422);

    let malformed_error: serde_json::Value = malformed_response
        .json()
        .await
        .expect("failed to decode malformed-target error");
    assert_eq!(malformed_error["error"]["code"], "validation_failed");
    assert!(
        malformed_error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("target")
    );

    let response = client
        .post(helpers::api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "target": "cpu-linux-ppc64le",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send invalid-target submission");
    assert_eq!(response.status(), 400);

    let error: serde_json::Value = response.json().await.expect("failed to decode error");
    assert_eq!(error["error"]["code"], "bad_request");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("target")
    );

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 0);
}

/// Verifies that oversized manifest notes are rejected before artifact storage.
#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_oversized_manifest_note_before_storage(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = helpers::test_config(storage.path(), &helpers::examples_challenges_root());
    let app = helpers::spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(helpers::api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "note-limit-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;
    let artifact_base64 = zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "a".repeat(1025),
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\npython main.py\n".to_string(),
        ),
        (
            "main.py",
            sample_sum_solution("payload['a'] + payload['b']"),
        ),
    ]);

    let response = client
        .post(helpers::api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64
        }))
        .send()
        .await
        .expect("failed to send oversized-note submission");
    assert_eq!(response.status(), 422);
    let body: serde_json::Value = response.json().await.expect("failed to decode error");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("note must be at most 1024 UTF-8 bytes")
    );

    let solution_submission_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count, 0);
    assert!(helpers::storage_prefix_is_empty(&config, "solution-submissions").await);
}

/// Verifies that invalid solution submission path ids return bad request.
#[sqlx::test(migrations = "../migrations")]
async fn invalid_solution_submission_path_ids_return_bad_request(pool: sqlx::PgPool) {
    let app = helpers::spawn_app(pool).await;
    let client = reqwest::Client::new();

    for path in [
        "/api/agent/solution-submissions/not-a-uuid",
        "/api/agent/solution-submissions/not-a-uuid/logs",
    ] {
        let response = client
            .get(helpers::api_url(&app, path))
            .send()
            .await
            .expect("failed to send invalid path-id request");
        assert_eq!(response.status(), 400);
        let error: serde_json::Value = response.json().await.expect("failed to decode error");
        assert_eq!(error["error"]["code"], "bad_request");
        assert!(
            error["error"]["message"]
                .as_str()
                .expect("error message")
                .contains("solution_submission_id")
        );
    }
}

/// Verifies that solution submission rejects legacy round field before artifact decode.
#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_legacy_round_field_before_artifact_decode(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "legacy-round-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;

    let no_round_field = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send no-round submission");
    assert_eq!(no_round_field.status(), 400);

    let unknown_round_field = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "round_id": "missing-round",
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send submission with legacy round_id");
    assert_eq!(unknown_round_field.status(), 422);
    let unknown_error: serde_json::Value = unknown_round_field
        .json()
        .await
        .expect("failed to decode error");
    assert!(
        unknown_error["error"]["message"]
            .as_str()
            .expect("message")
            .contains("round")
    );

    let malformed_round_field = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "round_id": "Main Round!",
            "target": "linux-arm64-cpu",
            "artifact_base64": "not-base64"
        }))
        .send()
        .await
        .expect("failed to send malformed legacy round_id");
    assert_eq!(malformed_round_field.status(), 422);
    let malformed_error: serde_json::Value = malformed_round_field
        .json()
        .await
        .expect("failed to decode error");
    assert!(
        malformed_error["error"]["message"]
            .as_str()
            .expect("message")
            .contains("round_id")
    );

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 0);
}

/// Verifies that solution submission rejects unstarted and closed challenges before artifact decode.
#[sqlx::test(migrations = "../migrations")]
async fn solution_submission_rejects_unstarted_and_closed_challenges_before_artifact_decode(
    pool: sqlx::PgPool,
) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    write_challenge_window_challenge(
        challenges.path(),
        "future-challenge",
        Some("2999-01-01T00:00:00Z"),
        None,
    );
    write_challenge_window_challenge(
        challenges.path(),
        "closed-challenge",
        Some("2000-01-01T00:00:00Z"),
        Some("2000-01-02T00:00:00Z"),
    );
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "challenge-window-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    for (challenge_name, expected_message) in [
        ("future-challenge", "not started"),
        ("closed-challenge", "closed"),
    ] {
        let challenge_name = published_challenge_name(&pool, challenge_name).await;
        let response = client
            .post(api_url(&app, "/api/agent/solution-submissions"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "challenge_name": challenge_name,
                "target": "linux-arm64-cpu",
                "artifact_base64": "not-base64"
            }))
            .send()
            .await
            .expect("failed to send challenge-window submission");
        assert_eq!(response.status(), 403);
        let error: serde_json::Value = response.json().await.expect("failed to decode error");
        assert!(
            error["error"]["message"]
                .as_str()
                .expect("message")
                .contains(expected_message)
        );
    }

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 0);
}

/// Verifies that private shortlist challenge requires owner delta before artifact decode.
#[sqlx::test(migrations = "../migrations")]
async fn private_shortlist_challenge_requires_owner_delta_before_artifact_decode(
    pool: sqlx::PgPool,
) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    write_private_shortlist_challenge(challenges.path(), "shortlist-challenge");
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let owner = create_creator_session(&pool, 2001, "shortlist-owner").await;
    let non_owner = create_creator_session(&pool, 2002, "not-owner").await;
    let challenge_name = agentics_domain::models::names::ChallengeName::try_new(
        published_challenge_name(&pool, "shortlist-challenge").await,
    )
    .expect("test challenge name is valid");
    let owner_human_id = agentics_domain::models::ids::HumanId::try_new(&owner.human_id)
        .expect("valid owner human id");
    agentics_persistence::Repositories::new(&pool)
        .challenges()
        .add_owner(&challenge_name, &owner_human_id)
        .await
        .expect("owner should be granted");

    let shortlisted = register_api_agent(&client, &app, "shortlisted-agent").await;
    let outsider = register_api_agent(&client, &app, "outsider-agent").await;

    let missing_shortlist = submit_solution(
        &client,
        &app,
        &shortlisted.token,
        challenge_name.as_str(),
        "not-base64",
    )
    .await;
    assert_eq!(missing_shortlist.status(), reqwest::StatusCode::FORBIDDEN);
    let missing_error: serde_json::Value = missing_shortlist
        .json()
        .await
        .expect("failed to decode missing-shortlist error");
    assert_eq!(
        missing_error["error"]["message"],
        "challenge requires a shortlist, but no shortlist has been uploaded yet"
    );

    let non_owner_upload = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenges/{challenge_name}/shortlist-revisions"),
        )),
        &non_owner,
    )
    .json(&serde_json::json!({ "agent_ids_to_add": [shortlisted.agent_id] }))
    .send()
    .await
    .expect("failed to upload shortlist as non-owner");
    assert_eq!(non_owner_upload.status(), reqwest::StatusCode::FORBIDDEN);

    let unknown_agent_upload = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenges/{challenge_name}/shortlist-revisions"),
        )),
        &owner,
    )
    .json(&serde_json::json!({ "agent_ids_to_add": ["agent_missing"] }))
    .send()
    .await
    .expect("failed to upload unknown shortlist agent");
    assert_eq!(
        unknown_agent_upload.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );

    let revision: serde_json::Value = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenges/{challenge_name}/shortlist-revisions"),
        )),
        &owner,
    )
    .json(&serde_json::json!({ "agent_ids_to_add": [
        shortlisted.agent_id,
        shortlisted.agent_id
    ] }))
    .send()
    .await
    .expect("failed to upload shortlist")
    .error_for_status()
    .expect("shortlist upload should succeed")
    .json()
    .await
    .expect("failed to decode shortlist revision");
    assert_eq!(revision["requested_count"], 2);
    assert_eq!(revision["added_count"], 1);

    let duplicate_revision: serde_json::Value = creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenges/{challenge_name}/shortlist-revisions"),
        )),
        &owner,
    )
    .json(&serde_json::json!({ "agent_ids_to_add": [shortlisted.agent_id] }))
    .send()
    .await
    .expect("failed to upload duplicate shortlist")
    .error_for_status()
    .expect("duplicate shortlist upload should succeed")
    .json()
    .await
    .expect("failed to decode duplicate shortlist revision");
    assert_eq!(duplicate_revision["added_count"], 0);

    let shortlist: serde_json::Value = creator_auth(
        client.get(api_url(
            &app,
            &format!("/api/creator/challenges/{challenge_name}/shortlist"),
        )),
        &owner,
    )
    .send()
    .await
    .expect("failed to fetch shortlist")
    .error_for_status()
    .expect("shortlist fetch should succeed")
    .json()
    .await
    .expect("failed to decode shortlist");
    assert_eq!(shortlist["items"].as_array().expect("items").len(), 1);
    assert_eq!(shortlist["items"][0]["agent_id"], shortlisted.agent_id);

    let outsider_response = submit_solution(
        &client,
        &app,
        &outsider.token,
        challenge_name.as_str(),
        "not-base64",
    )
    .await;
    assert_eq!(outsider_response.status(), reqwest::StatusCode::FORBIDDEN);
    let outsider_error: serde_json::Value = outsider_response
        .json()
        .await
        .expect("failed to decode outsider error");
    assert_eq!(
        outsider_error["error"]["message"],
        "agent is not eligible for this challenge"
    );

    let accepted = submit_solution(
        &client,
        &app,
        &shortlisted.token,
        challenge_name.as_str(),
        &solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']")),
    )
    .await;
    assert_eq!(accepted.status(), reqwest::StatusCode::CREATED);
}

/// Verifies that challenge submission limit rejects before extra artifact work.
#[sqlx::test(migrations = "../migrations")]
async fn challenge_submission_limit_rejects_before_extra_artifact_work(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    write_limited_submission_challenge(challenges.path(), "limited-sum", 1);
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let agent = register_api_agent(&client, &app, "limited-agent").await;
    let artifact = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let challenge_name = published_challenge_name(&pool, "limited-sum").await;

    let accepted = submit_solution_with_target(
        &client,
        &app,
        &agent.token,
        &challenge_name,
        "linux-arm64-cpu",
        &artifact,
    )
    .await;
    assert_eq!(accepted.status(), reqwest::StatusCode::CREATED);

    let rejected = submit_solution_with_target(
        &client,
        &app,
        &agent.token,
        &challenge_name,
        "linux-arm64-cpu",
        &artifact,
    )
    .await;
    assert_eq!(rejected.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
    let error: serde_json::Value = rejected.json().await.expect("failed to decode quota error");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("message")
            .contains("challenge limit exceeded")
    );

    let solution_submission_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM solution_submissions")
            .fetch_one(&pool)
            .await
            .expect("failed to query solution submission count");
    assert_eq!(solution_submission_count.0, 1);
}

/// Verifies that admin direct publish is disabled before bundle-specific validation.
#[sqlx::test(migrations = "../migrations")]
async fn admin_direct_publish_is_disabled(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    write_private_shortlist_challenge(challenges.path(), "shortlist-direct");
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let admin_auth = admin_service_token_header(&app);

    let response = client
        .post(api_url(&app, "/admin/challenges/shortlist-direct/publish"))
        .header("Authorization", admin_auth)
        .json(&serde_json::json!({
            "bundle_path": challenges
                .path()
                .join("shortlist-direct/v1")
                .to_string_lossy()
                .to_string()
        }))
        .send()
        .await
        .expect("failed to publish private shortlist challenge directly");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

/// Verifies that parent submissions cannot cross agent ownership boundaries.
#[sqlx::test(migrations = "../migrations")]
async fn parent_solution_submission_must_match_agent_and_scope(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let parent_agent = register_api_agent(&client, &app, "parent-agent").await;
    let child_agent = register_api_agent(&client, &app, "child-agent").await;
    let artifact = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;

    let parent: serde_json::Value = submit_solution(
        &client,
        &app,
        &parent_agent.token,
        &sample_sum_id,
        &artifact,
    )
    .await
    .error_for_status()
    .expect("parent submission should queue")
    .json()
    .await
    .expect("parent json");
    run_worker_once(&pool, &config).await;

    let response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {}", child_agent.token))
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
            "target": "linux-arm64-cpu",
            "parent_solution_submission_id": parent["id"],
            "artifact_base64": "not base64"
        }))
        .send()
        .await
        .expect("child submission request");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let error: serde_json::Value = response.json().await.expect("error json");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("message")
            .contains("same agent, challenge_name, and target"),
        "parent scope validation must run before artifact decoding"
    );
}

/// Carries api agent data across this module boundary.
struct ApiAgent {
    agent_id: String,
    token: String,
}

/// Handles register api agent for this module.
async fn register_api_agent(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    name: &str,
) -> ApiAgent {
    let register_response: serde_json::Value = client
        .post(api_url(app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": name }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");

    ApiAgent {
        agent_id: register_response["agent_id"]
            .as_str()
            .expect("missing agent id")
            .to_string(),
        token: register_response["token"]
            .as_str()
            .expect("missing token")
            .to_string(),
    }
}

/// Handles submit solution for this module.
async fn submit_solution(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    token: &str,
    challenge_name: &str,
    artifact_base64: &str,
) -> reqwest::Response {
    submit_solution_with_target(
        client,
        app,
        token,
        challenge_name,
        "linux-arm64-cpu",
        artifact_base64,
    )
    .await
}

/// Handles submit solution with target for this module.
async fn submit_solution_with_target(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    token: &str,
    challenge_name: &str,
    target: &str,
    artifact_base64: &str,
) -> reqwest::Response {
    client
        .post(api_url(app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": challenge_name,
            "target": target,
            "artifact_base64": artifact_base64
        }))
        .send()
        .await
        .expect("failed to submit solution")
}

fn write_copied_sample_sum_manifest(root: &Path, challenge_name: &str, summary_en: &str) {
    std::fs::write(
        root.join(challenge_name).join("agentics.challenge.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": challenge_name,
            "title": challenge_name,
            "summary": {
                "en": summary_en,
                "zh": "用于请求校验集成测试的 Sample Sum 变体。"
            },
            "keywords": ["arithmetic", "request validation", "fixture"],
            "readme_path": "v1/statement.md",
            "bundle_path": "v1",
            "private_assets": [],
            "ci": {
                "validate_manifest": true,
                "validate_public_bundle": true,
                "smoke_test_public_validation": true
            }
        }))
        .expect("failed to serialize copied sample-sum manifest"),
    )
    .expect("failed to write copied sample-sum manifest");
}

/// Writes challenge window challenge to the target path.
fn write_challenge_window_challenge(
    root: &Path,
    challenge_name: &str,
    starts_at: Option<&str>,
    closes_at: Option<&str>,
) {
    let bundle_dir = root.join(challenge_name).join("v1");
    copy_dir_all(
        &examples_challenges_root().join("sample-sum/v1"),
        &bundle_dir,
    );
    write_copied_sample_sum_manifest(
        root,
        challenge_name,
        "A sample sum variant with custom challenge windows.",
    );
    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("failed to read spec"))
            .expect("failed to parse spec");
    spec["challenge_name"] = serde_json::json!(challenge_name);
    spec["challenge_title"] = serde_json::json!(challenge_name);
    if let Some(starts_at) = starts_at {
        spec["starts_at"] = serde_json::json!(starts_at);
    }
    if let Some(closes_at) = closes_at {
        spec["closes_at"] = serde_json::json!(closes_at);
    }
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
}

/// Writes private shortlist challenge to the target path.
fn write_private_shortlist_challenge(root: &Path, challenge_name: &str) {
    let bundle_dir = root.join(challenge_name).join("v1");
    copy_dir_all(
        &examples_challenges_root().join("sample-sum/v1"),
        &bundle_dir,
    );
    write_copied_sample_sum_manifest(
        root,
        challenge_name,
        "A sample sum variant using private shortlist eligibility.",
    );
    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("failed to read spec"))
            .expect("failed to parse spec");
    spec["challenge_name"] = serde_json::json!(challenge_name);
    spec["challenge_title"] = serde_json::json!(challenge_name);
    spec["eligibility"] = serde_json::json!({ "type": "private_shortlist" });
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
}

/// Writes limited submission challenge to the target path.
fn write_limited_submission_challenge(root: &Path, challenge_name: &str, official_limit: i64) {
    let bundle_dir = root.join(challenge_name).join("v1");
    copy_dir_all(
        &examples_challenges_root().join("sample-sum/v1"),
        &bundle_dir,
    );
    write_copied_sample_sum_manifest(
        root,
        challenge_name,
        "A sample sum variant with a custom official submission limit.",
    );
    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("failed to read spec"))
            .expect("failed to parse spec");
    spec["challenge_name"] = serde_json::json!(challenge_name);
    spec["challenge_title"] = serde_json::json!(challenge_name);
    spec["official_submission_limit"] = serde_json::json!(official_limit);
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
}
