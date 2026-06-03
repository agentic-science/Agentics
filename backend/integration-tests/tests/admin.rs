//! Admin route integration tests.

mod helpers;

use agentics_domain::models::ids::HumanId;
use helpers::{
    admin_service_token_header, api_url, create_creator_session, examples_challenges_root,
    published_challenge_name, spawn_app, spawn_app_with_config, test_config,
};

/// Verifies that admin read models power operator console.
#[sqlx::test(migrations = "../migrations")]
async fn admin_read_models_power_operator_console(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let auth = admin_service_token_header(&app);
    let client = reqwest::Client::new();
    agentics_persistence::Repositories::new(&pool)
        .maintenance()
        .upsert_service_heartbeat(
            "test-worker",
            &agentics_persistence::HeartbeatPayload {
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
        .find(|item| item["challenge_name"] == "sample-sum")
        .expect("sample-sum should be seeded");
    assert_eq!(sample_sum["targets"][0]["name"], "linux-arm64-cpu");
    assert_eq!(sample_sum["targets"][0]["validation_enabled"], true);
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

/// Verifies admins can attach and clear Moltbook discussion anchors by challenge name.
#[sqlx::test(migrations = "../migrations")]
async fn admin_manages_challenge_moltbook_discussion_anchor(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let auth = admin_service_token_header(&app);
    let client = reqwest::Client::new();
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;

    let response: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenges/{sample_sum_id}/moltbook-discussion"),
        ))
        .header("Authorization", auth.clone())
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
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{sample_sum_id}"),
        ))
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

    let public_list: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges"))
        .send()
        .await
        .expect("failed to fetch public challenge list")
        .json()
        .await
        .expect("failed to decode public challenge list");
    let listed_sample_sum = public_list["items"]
        .as_array()
        .expect("challenge list items should be an array")
        .iter()
        .find(|item| item["challenge_name"] == sample_sum_id.as_str())
        .expect("sample-sum should be listed");
    assert_eq!(
        listed_sample_sum["moltbook_discussion_url"],
        "https://www.moltbook.com/post/sample-sum"
    );

    let response: serde_json::Value = client
        .delete(api_url(
            &app,
            &format!("/admin/challenges/{sample_sum_id}/moltbook-discussion"),
        ))
        .header("Authorization", auth)
        .send()
        .await
        .expect("failed to clear Moltbook discussion")
        .json()
        .await
        .expect("failed to decode Moltbook discussion clear response");
    assert!(response["moltbook"].get("discussion_url").is_none());
}

/// Verifies that legacy direct challenge creation and publish routes are disabled.
#[sqlx::test(migrations = "../migrations")]
async fn direct_challenge_creation_and_publish_routes_are_disabled(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;
    let auth = admin_service_token_header(&app);

    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges"))
        .header("Authorization", auth.clone())
        .json(&serde_json::json!({
            "name": "test-challenge",
            "title": "Test Challenge",
            "summary": { "en": "A test challenge", "zh": "测试挑战" }
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);

    let response = reqwest::Client::new()
        .post(api_url(&app, "/admin/challenges/test-challenge/publish"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "bundle_path": "/nonexistent/bundle"
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

/// Verifies that admin routes require auth.
#[sqlx::test(migrations = "../migrations")]
async fn admin_routes_require_auth(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    let response = reqwest::Client::new()
        .get(api_url(&app, "/admin/challenges"))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}

/// Verifies identity management cannot revoke the final active human admin.
#[sqlx::test(migrations = "../migrations")]
async fn final_human_admin_role_cannot_be_revoked(pool: sqlx::PgPool) {
    let app = spawn_app(pool.clone()).await;
    let admin = create_creator_session(&pool, 7_001, "last-admin").await;
    let admin_human_id =
        HumanId::try_new(admin.human_id.clone()).expect("test human id should parse");
    agentics_persistence::Repositories::new(&pool)
        .sessions()
        .grant_admin_role(&admin_human_id, &admin_human_id)
        .await
        .expect("test human admin role should grant");
    sqlx::query(
        r#"
        UPDATE humans
        SET status = 'disabled',
            disabled_at = NOW()
        WHERE id IN (
            SELECT human_id
            FROM human_external_identities
            WHERE provider = 'github'
              AND provider_user_id = 9001
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("integration bootstrap admin should disable");

    let response = reqwest::Client::new()
        .post(api_url(
            &app,
            &format!("/admin/humans/{admin_human_id}/roles/admin/revoke"),
        ))
        .header("Cookie", &admin.cookie_header)
        .header("X-Agentics-CSRF-Token", &admin.csrf_token)
        .send()
        .await
        .expect("failed to revoke final admin");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let human = agentics_persistence::Repositories::new(&pool)
        .sessions()
        .get_human_by_id(&admin_human_id)
        .await
        .expect("admin human should still exist");
    assert!(
        human
            .roles
            .contains(&agentics_domain::models::auth::HumanRole::Admin)
    );
}

/// Verifies that admin official run cannot overlap an active validation job.
#[sqlx::test(migrations = "../migrations")]
async fn admin_official_run_rejects_submission_with_active_job(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.quotas.max_active_official_jobs = 1;
    config.quotas.official_runs_per_agent_challenge_day = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let admin_auth = admin_service_token_header(&app);
    let client = reqwest::Client::new();
    let sample_sum_id = published_challenge_name(&pool, "sample-sum").await;

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
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
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
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
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
        .json(&serde_json::json!({
            "challenge_name": &sample_sum_id,
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
        .send()
        .await
        .expect("failed to request admin official run");
    assert_eq!(admin_response.status(), 409);

    let active_official_jobs: i64 = agentics_persistence::Repositories::new(&pool)
        .evaluation_jobs()
        .count_active(agentics_domain::models::evaluation::ScoringMode::Official)
        .await
        .expect("failed to count official jobs");
    assert_eq!(active_official_jobs, 1);
}
