//! Basic admin route integration tests.

mod helpers;

use helpers::{api_url, examples_challenges_root, spawn_app, spawn_app_with_config, test_config};
use shared::config::Config;

/// Verifies that admin read models power operator console.
#[sqlx::test(migrations = "../migrations")]
async fn admin_read_models_power_operator_console(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let auth = helpers::basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );
    let client = reqwest::Client::new();

    shared::db::upsert_service_heartbeat(
        &pool,
        "test-worker",
        &shared::db::HeartbeatPayload {
            status: "idle".to_string(),
            accelerators: vec!["none".to_string()],
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
        .header("X-Agentics-Admin-Automation", "true")
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
        .find(|item| item["name"] == "sample-sum")
        .expect("sample-sum should be seeded");
    assert_eq!(sample_sum["targets"][0]["name"], "linux-arm64-cpu");
    assert_eq!(sample_sum["targets"][0]["validation_enabled"], true);
    assert_eq!(sample_sum["eligibility"]["type"], "open");
    assert_eq!(sample_sum["private_benchmark_enabled"], true);

    let submissions: serde_json::Value = client
        .get(api_url(&app, "/admin/solution-submissions"))
        .header("Authorization", auth.clone())
        .header("X-Agentics-Admin-Automation", "true")
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
        .header("X-Agentics-Admin-Automation", "true")
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
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to list admin service heartbeats")
        .json()
        .await
        .expect("failed to decode admin service heartbeats");
    assert_eq!(heartbeats["items"][0]["service_name"], "test-worker");
    assert_eq!(heartbeats["items"][0]["payload"]["status"], "idle");
}

/// Verifies admins can attach and clear Moltbook discussion anchors by challenge name.
#[sqlx::test(migrations = "../migrations")]
async fn admin_manages_challenge_moltbook_discussion_anchor(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool, config.clone()).await;
    let auth = helpers::basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );
    let client = reqwest::Client::new();

    let response: serde_json::Value = client
        .post(api_url(
            &app,
            "/admin/challenges/sample-sum/moltbook-discussion",
        ))
        .header("Authorization", auth.clone())
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "discussion_url": "https://www.moltbook.com/post/sample-sum"
        }))
        .send()
        .await
        .expect("failed to set Moltbook discussion")
        .json()
        .await
        .expect("failed to decode Moltbook discussion response");
    assert_eq!(response["challenge_name"], "sample-sum");
    assert_eq!(response["moltbook"]["submolt_name"], "agentics-platform");
    assert_eq!(
        response["moltbook"]["submolt_url"],
        "https://www.moltbook.com/m/agentics-platform"
    );
    assert_eq!(
        response["moltbook"]["discussion_url"],
        "https://www.moltbook.com/post/sample-sum"
    );

    let public_detail: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges/sample-sum"))
        .send()
        .await
        .expect("failed to fetch public challenge detail")
        .json()
        .await
        .expect("failed to decode public challenge detail");
    assert_eq!(
        public_detail["moltbook"]["discussion_url"],
        "https://www.moltbook.com/post/sample-sum"
    );

    let response: serde_json::Value = client
        .delete(api_url(
            &app,
            "/admin/challenges/sample-sum/moltbook-discussion",
        ))
        .header("Authorization", auth)
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to clear Moltbook discussion")
        .json()
        .await
        .expect("failed to decode Moltbook discussion clear response");
    assert!(response["moltbook"].get("discussion_url").is_none());
}

/// Verifies that create challenge and publish contract.
#[sqlx::test(migrations = "../migrations")]
async fn create_challenge_and_publish_contract(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;
    let config = Config::from_env().expect("failed to load config");

    // Successful admin creation verifies basic-auth extraction and challenge upsert.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .header(
            "Authorization",
            helpers::basic_auth_header(
                &config.admin_username,
                config.expose_admin_password_for_http_basic(),
            ),
        )
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "name": "test-challenge",
            "title": "Test Challenge",
            "summary": { "en": "A test challenge", "zh": "测试挑战" }
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    assert_eq!(body["name"], "test-challenge");
    assert_eq!(body["title"], "Test Challenge");

    // Legacy direct publishing is disabled for MVP.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges/test-challenge/publish"))
        .header(
            "Authorization",
            helpers::basic_auth_header(
                &config.admin_username,
                config.expose_admin_password_for_http_basic(),
            ),
        )
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "bundle_path": "/nonexistent/bundle"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    let body: serde_json::Value = response.json().await.expect("failed to parse error");
    assert!(
        body["message"]
            .as_str()
            .expect("error message")
            .contains("GitHub-backed challenge draft")
    );
}

/// Verifies that admin routes require auth.
#[sqlx::test(migrations = "../migrations")]
async fn admin_routes_require_auth(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .json(&serde_json::json!({
            "name": "test-challenge",
            "title": "Test Challenge"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}

/// Verifies that admin session cookie authenticates admin routes.
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
            "password": config.expose_admin_password_for_http_basic()
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
            "name": "session-admin-missing-csrf",
            "title": "Session Admin Missing CSRF",
            "summary": { "en": "Session admin challenge", "zh": "会话管理员挑战" }
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
            "name": "session-admin",
            "title": "Session Admin",
            "summary": { "en": "Session admin challenge", "zh": "会话管理员挑战" }
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

/// Verifies process-local throttling for repeated failed admin authentication.
#[sqlx::test(migrations = "../migrations")]
async fn failed_admin_authentication_is_throttled(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool, config).await;
    let client = reqwest::Client::new();

    for _ in 0..5 {
        let response = client
            .post(api_url(&app, "/api/auth/admin/login"))
            .json(&serde_json::json!({
                "username": "admin",
                "password": "wrong-password"
            }))
            .send()
            .await
            .expect("failed admin login should receive response");
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    let throttled_login = client
        .post(api_url(&app, "/api/auth/admin/login"))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "wrong-password"
        }))
        .send()
        .await
        .expect("throttled admin login should receive response");
    assert_eq!(
        throttled_login.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );

    let bad_basic = helpers::basic_auth_header("other", "wrong-password");
    for _ in 0..5 {
        let response = client
            .get(api_url(&app, "/admin/challenges"))
            .header("Authorization", bad_basic.clone())
            .header("X-Agentics-Admin-Automation", "true")
            .send()
            .await
            .expect("failed basic auth should receive response");
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    let throttled_basic = client
        .get(api_url(&app, "/admin/challenges"))
        .header("Authorization", bad_basic)
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("throttled basic auth should receive response");
    assert_eq!(
        throttled_basic.status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
}

/// Verifies that admin official run cannot overlap an active validation job.
#[sqlx::test(migrations = "../migrations")]
async fn admin_official_run_rejects_submission_with_active_job(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.max_active_official_jobs = 1;
    config.official_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let admin_auth = helpers::basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "admin-override-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let official = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": helpers::solution_zip_base64(&helpers::sample_sum_solution("payload['a'] + payload['b']")),
            "explanation": "fills official queue"
        }))
        .send()
        .await
        .expect("failed to submit official run");
    assert_eq!(official.status(), 201);

    let validation_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
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
    let admin_submissions: serde_json::Value = client
        .get(api_url(&app, "/admin/solution-submissions"))
        .header("Authorization", admin_auth.clone())
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to list admin submissions")
        .json()
        .await
        .expect("failed to decode admin submissions");
    assert!(
        admin_submissions["items"]
            .as_array()
            .expect("admin submission items")
            .iter()
            .any(|item| item["id"] == validation_id && item["note"] == "sample-sum smoke solution"),
        "admin solution submission list should expose stored manifest note"
    );

    let public_quota_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
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
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to request admin official run");
    assert_eq!(admin_response.status(), 409);

    let active_official_jobs: i64 = shared::db::count_active_evaluation_jobs(
        &pool,
        shared::models::evaluation::ScoringMode::Official,
    )
    .await
    .expect("failed to count official jobs");
    assert_eq!(active_official_jobs, 1);
}
