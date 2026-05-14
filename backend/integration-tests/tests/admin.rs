//! Basic admin route integration tests.

mod helpers;

use helpers::{
    api_url, copy_dir_all, examples_challenges_root, spawn_app, spawn_app_with_config, test_config,
};
use shared::config::Config;

#[sqlx::test(migrations = "../migrations")]
async fn admin_read_models_power_operator_console(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let auth = helpers::basic_auth_header(&config.admin_username, &config.admin_password);
    let client = reqwest::Client::new();

    shared::db::upsert_service_heartbeat(
        &pool,
        "test-worker",
        &shared::db::HeartbeatPayload {
            status: "idle".to_string(),
            job_id: None,
            solution_submission_id: None,
            last_completed_job_id: None,
            last_failed_job_id: None,
        },
    )
    .await
    .expect("failed to insert heartbeat");

    let challenges: serde_json::Value = client
        .get(api_url(&app, "/admin/challenges"))
        .header("Authorization", auth.clone())
        .send()
        .await
        .expect("failed to list admin challenges")
        .json()
        .await
        .expect("failed to decode admin challenges");
    assert!(challenges["items"].as_array().expect("items").len() >= 2);
    assert!(challenges["items"][0].get("status").is_some());
    let sample_sum = challenges["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["id"] == "sample-sum")
        .expect("sample-sum should be seeded");
    assert_eq!(sample_sum["benchmark_targets"][0]["id"], "linux-arm64-cpu");
    assert_eq!(
        sample_sum["benchmark_targets"][0]["validation_enabled"],
        true
    );
    assert_eq!(sample_sum["eligibility"]["type"], "open");
    assert_eq!(sample_sum["private_benchmark_enabled"], true);

    let submissions: serde_json::Value = client
        .get(api_url(&app, "/admin/solution-submissions"))
        .header("Authorization", auth.clone())
        .send()
        .await
        .expect("failed to list admin solution submissions")
        .json()
        .await
        .expect("failed to decode admin solution submissions");
    assert!(submissions["items"].as_array().is_some());

    let capacity: serde_json::Value = client
        .get(api_url(&app, "/admin/capacity"))
        .header("Authorization", auth.clone())
        .send()
        .await
        .expect("failed to fetch admin capacity")
        .json()
        .await
        .expect("failed to decode admin capacity");
    assert_eq!(
        capacity["quotas"]["validation_runs_per_agent_challenge_day"],
        20
    );
    assert_eq!(
        capacity["quotas"]["official_runs_per_agent_challenge_day"],
        5
    );
    assert_eq!(capacity["quotas"]["max_active_official_jobs"], 20);
    assert_eq!(capacity["usage"]["active_agents"], 0);
    assert_eq!(capacity["usage"]["active_official_jobs"], 0);

    let heartbeats: serde_json::Value = client
        .get(api_url(&app, "/admin/service-heartbeats"))
        .header("Authorization", auth)
        .send()
        .await
        .expect("failed to list admin service heartbeats")
        .json()
        .await
        .expect("failed to decode admin service heartbeats");
    assert_eq!(heartbeats["items"][0]["service_name"], "test-worker");
    assert_eq!(heartbeats["items"][0]["payload"]["status"], "idle");
}

#[sqlx::test(migrations = "../migrations")]
async fn create_challenge_and_publish_contract(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;
    let config = Config::from_env().expect("failed to load config");

    // Successful admin creation verifies basic-auth extraction and challenge upsert.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .header(
            "Authorization",
            helpers::basic_auth_header(&config.admin_username, &config.admin_password),
        )
        .json(&serde_json::json!({
            "id": "test-challenge",
            "slug": "test-challenge",
            "title": "Test Challenge",
            "summary": "A test challenge"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    assert_eq!(body["id"], "test-challenge");
    assert_eq!(body["title"], "Test Challenge");

    // Publishing still validates bundle paths before writing the benchmark contract.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges/test-challenge/publish"))
        .header(
            "Authorization",
            helpers::basic_auth_header(&config.admin_username, &config.admin_password),
        )
        .json(&serde_json::json!({
            "bundle_path": "/nonexistent/bundle"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(!response.status().is_success());
}

#[sqlx::test(migrations = "../migrations")]
async fn publishing_contract_exposes_challenge_policy(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let auth = helpers::basic_auth_header(&config.admin_username, &config.admin_password);
    let client = reqwest::Client::new();

    let bundle = challenges.path().join("published-contract/v1");
    write_admin_publish_bundle(
        &bundle,
        "published-contract",
        "Published Contract",
        "Published contract summary",
    );

    client
        .post(api_url(&app, "/admin/challenges"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "id": "published-contract",
            "slug": "published-contract",
            "title": "Published Contract",
            "summary": "Published contract challenge"
        }))
        .send()
        .await
        .expect("failed to create challenge")
        .error_for_status()
        .expect("challenge create should succeed");

    client
        .post(api_url(
            &app,
            "/admin/challenges/published-contract/publish",
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "bundle_path": bundle.to_string_lossy() }))
        .send()
        .await
        .expect("failed to publish contract")
        .error_for_status()
        .expect("publish should succeed");

    let public_challenge: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges/published-contract"))
        .send()
        .await
        .expect("failed to fetch public challenge")
        .json()
        .await
        .expect("failed to decode public challenge");

    assert_eq!(public_challenge["spec"]["eligibility"]["type"], "open");
    assert_eq!(public_challenge["summary"], "Published contract summary");

    let spec_json: serde_json::Value =
        sqlx::query_scalar("SELECT spec_json FROM challenges WHERE id = $1")
            .bind("published-contract")
            .fetch_one(&pool)
            .await
            .expect("failed to query published contract");
    assert_eq!(spec_json["eligibility"]["type"], "open");
}

#[sqlx::test(migrations = "../migrations")]
async fn publishing_existing_contract_is_rejected_without_mutating_it(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let auth = helpers::basic_auth_header(&config.admin_username, &config.admin_password);
    let client = reqwest::Client::new();

    let original_bundle = challenges.path().join("immutable-challenge/original");
    let replacement_bundle = challenges.path().join("immutable-challenge/replacement");
    write_admin_publish_bundle(
        &original_bundle,
        "immutable-challenge",
        "Immutable Challenge",
        "Original summary",
    );
    write_admin_publish_bundle(
        &replacement_bundle,
        "immutable-challenge",
        "Immutable Challenge",
        "Replacement summary",
    );

    client
        .post(api_url(&app, "/admin/challenges"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "id": "immutable-challenge",
            "slug": "immutable-challenge",
            "title": "Immutable Challenge",
            "summary": "Immutable challenge"
        }))
        .send()
        .await
        .expect("failed to create challenge")
        .error_for_status()
        .expect("challenge create should succeed");

    client
        .post(api_url(
            &app,
            "/admin/challenges/immutable-challenge/publish",
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "bundle_path": original_bundle.to_string_lossy() }))
        .send()
        .await
        .expect("failed to publish original contract")
        .error_for_status()
        .expect("original contract publish should succeed");

    let duplicate_response = client
        .post(api_url(
            &app,
            "/admin/challenges/immutable-challenge/publish",
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "bundle_path": replacement_bundle.to_string_lossy() }))
        .send()
        .await
        .expect("failed to publish duplicate contract");
    assert_eq!(duplicate_response.status(), reqwest::StatusCode::CONFLICT);

    let row: (String, serde_json::Value) =
        sqlx::query_as("SELECT bundle_path, spec_json FROM challenges WHERE id = $1")
            .bind("immutable-challenge")
            .fetch_one(&pool)
            .await
            .expect("failed to query published contract");

    assert_ne!(row.0, original_bundle.to_string_lossy());
    assert!(
        std::path::Path::new(&row.0).starts_with(storage.path()),
        "admin publish should copy bundles into managed storage"
    );
    assert_eq!(row.1["challenge_summary"], "Original summary");

    let managed_spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&row.0).join("spec.json"))
            .expect("failed to read managed spec"),
    )
    .expect("failed to decode managed spec");
    assert_eq!(managed_spec["challenge_summary"], "Original summary");
}

#[sqlx::test(migrations = "../migrations")]
async fn publish_rejects_tag_only_images_when_digest_policy_is_enabled(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.require_digest_pinned_images = true;
    let app = spawn_app_with_config(pool, config.clone()).await;
    let auth = helpers::basic_auth_header(&config.admin_username, &config.admin_password);

    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges/sample-sum/publish"))
        .header("Authorization", auth)
        .json(&serde_json::json!({ "bundle_path": "sample-sum/v1" }))
        .send()
        .await
        .expect("failed to publish tag-only bundle");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response
        .json()
        .await
        .expect("failed to decode digest policy error");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("@sha256:<digest>")
    );
}

fn write_admin_publish_bundle(
    target: &std::path::Path,
    challenge_id: &str,
    challenge_title: &str,
    challenge_summary: &str,
) {
    copy_dir_all(&examples_challenges_root().join("sample-sum/v1"), target);
    let spec_path = target.join("spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("failed to read spec"))
            .expect("failed to parse spec");
    spec["challenge_id"] = serde_json::json!(challenge_id);
    spec["challenge_title"] = serde_json::json!(challenge_title);
    spec["challenge_summary"] = serde_json::json!(challenge_summary);
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
}

#[sqlx::test(migrations = "../migrations")]
async fn admin_routes_require_auth(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .json(&serde_json::json!({
            "id": "test-challenge",
            "title": "Test Challenge"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}

#[sqlx::test(migrations = "../migrations")]
async fn admin_session_cookie_authenticates_admin_routes(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool, config.clone()).await;
    let client = reqwest::Client::new();

    let login_response = client
        .post(api_url(&app, "/api/auth/admin/login"))
        .json(&serde_json::json!({
            "username": config.admin_username,
            "password": config.admin_password
        }))
        .send()
        .await
        .expect("failed to login as admin");
    assert_eq!(login_response.status(), reqwest::StatusCode::OK);

    let set_cookies = login_response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert!(
        set_cookies.iter().any(|value| {
            value.starts_with(&format!("{}=", config.web_session_cookie_name))
                && value.contains("HttpOnly")
        }),
        "admin login should set an HttpOnly session cookie"
    );
    let session_cookie = set_cookies
        .iter()
        .find_map(|value| {
            value
                .strip_prefix(&format!("{}=", config.web_session_cookie_name))
                .map(|_| value.split(';').next().expect("cookie pair").to_string())
        })
        .expect("admin login should set the session cookie");
    let login_body: serde_json::Value = login_response
        .json()
        .await
        .expect("failed to decode admin login response");
    let csrf_token = login_body["csrf_token"]
        .as_str()
        .expect("admin login should return csrf token");

    let list_response = client
        .get(api_url(&app, "/admin/challenges"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .send()
        .await
        .expect("failed to list admin challenges");
    assert_eq!(list_response.status(), reqwest::StatusCode::OK);

    let missing_csrf_response = client
        .post(api_url(&app, "/admin/challenges"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .json(&serde_json::json!({
            "id": "session-admin-missing-csrf",
            "slug": "session-admin-missing-csrf",
            "title": "Session Admin Missing CSRF",
            "summary": "Session admin challenge"
        }))
        .send()
        .await
        .expect("failed to create challenge without csrf");
    assert_eq!(
        missing_csrf_response.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    let create_response = client
        .post(api_url(&app, "/admin/challenges"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .header("x-agentics-csrf-token", csrf_token)
        .json(&serde_json::json!({
            "id": "session-admin",
            "slug": "session-admin",
            "title": "Session Admin",
            "summary": "Session admin challenge"
        }))
        .send()
        .await
        .expect("failed to create challenge with session auth");
    assert_eq!(create_response.status(), reqwest::StatusCode::CREATED);

    let logout_response = client
        .post(api_url(&app, "/api/auth/admin/logout"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .header("x-agentics-csrf-token", csrf_token)
        .send()
        .await
        .expect("failed to logout admin session");
    assert_eq!(logout_response.status(), reqwest::StatusCode::NO_CONTENT);
    assert!(
        logout_response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|value| {
                value.starts_with(&format!("{}=", config.web_session_cookie_name))
                    && value.contains("Max-Age=0")
            }),
        "admin logout should expire the session cookie"
    );

    let after_logout_response = client
        .get(api_url(&app, "/admin/challenges"))
        .header(reqwest::header::COOKIE, &session_cookie)
        .send()
        .await
        .expect("failed to list admin challenges after logout");
    assert_eq!(
        after_logout_response.status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn admin_official_run_bypasses_public_official_queue_limit(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.max_active_official_jobs = 1;
    config.official_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let admin_auth = helpers::basic_auth_header(&config.admin_username, &config.admin_password);
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "admin-override-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let official = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "benchmark_target_id": "linux-arm64-cpu",
            "artifact_base64": helpers::solution_zip_base64(&helpers::sample_sum_solution("payload['a'] + payload['b']")),
            "explanation": "fills official queue"
        }))
        .send()
        .await
        .expect("failed to submit official run");
    assert_eq!(official.status(), 201);

    let validation_response: serde_json::Value = client
        .post(api_url(&app, "/api/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "benchmark_target_id": "linux-arm64-cpu",
            "artifact_base64": helpers::solution_zip_base64(&helpers::sample_sum_solution("payload['a'] + payload['b']")),
            "explanation": "admin promotes this validation run"
        }))
        .send()
        .await
        .expect("failed to submit validation run")
        .error_for_status()
        .expect("validation should be accepted")
        .json()
        .await
        .expect("failed to decode validation response");
    let validation_id = validation_response["id"].as_str().expect("missing id");

    let public_quota_response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_id": "sample-sum",
            "benchmark_target_id": "linux-arm64-cpu",
            "artifact_base64": "not-base64",
            "explanation": "public official run should still be rejected"
        }))
        .send()
        .await
        .expect("failed to request public official run");
    assert_eq!(public_quota_response.status(), 429);

    let admin_response = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{validation_id}/official-run"),
        ))
        .header("Authorization", admin_auth)
        .send()
        .await
        .expect("failed to request admin official run");
    assert_eq!(admin_response.status(), 202);

    let active_official_jobs: i64 = shared::db::count_active_evaluation_jobs(
        &pool,
        shared::models::evaluation::ScoringMode::Official,
    )
    .await
    .expect("failed to count official jobs");
    assert_eq!(active_official_jobs, 2);
}
