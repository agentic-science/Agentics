//! Basic agent registration and authenticated route integration tests.

mod helpers;

use helpers::{api_url, examples_challenges_root, spawn_app, spawn_app_with_config, test_config};

/// Verifies that register agent and list challenges.
#[sqlx::test(migrations = "../migrations")]
async fn register_agent_and_list_challenges(pool: sqlx::PgPool) {
    let app = spawn_app(pool).await;

    // Registration returns the only copy of the agent bearer token.
    let response = reqwest::Client::new()
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({
            "display_name": "test-agent",
            "agent_description": "A test agent",
            "owner": "test-owner",
            "model_info": { "model": "gpt-4" }
        }))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    let token = body["token"].as_str().expect("token should exist");
    assert_eq!(body["display_name"], "test-agent");

    // Authenticated agent routes use the bearer token extractor.
    let response = reqwest::Client::new()
        .get(api_url(&app, "/api/agent/challenges"))
        .header("Authorization", format!("Bearer {}", token))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());

    let body: serde_json::Value = response.json().await.expect("failed to parse response");
    let items = body["items"].as_array().expect("items should be an array");
    assert!(items.is_empty());
    assert_eq!(body["total_count"], 0);
    assert_eq!(body["limit"], 100);
    assert_eq!(body["offset"], 0);
    assert_eq!(body["has_more"], false);

    // The same route must reject unauthenticated access.
    let response = reqwest::Client::new()
        .get(api_url(&app, "/api/agent/challenges"))
        .send()
        .await
        .expect("failed to execute request");

    assert_eq!(response.status(), 401);
}

/// Verifies that the public challenge catalog supports bounded offset pagination.
#[sqlx::test(migrations = "../migrations")]
async fn public_challenge_catalog_supports_pagination(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool, config).await;
    let client = reqwest::Client::new();

    let first_page: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges?limit=1&offset=0"))
        .send()
        .await
        .expect("failed to list first page")
        .error_for_status()
        .expect("first page should succeed")
        .json()
        .await
        .expect("first page JSON should decode");
    assert_eq!(first_page["items"].as_array().expect("items").len(), 1);
    assert_eq!(first_page["total_count"], 2);
    assert_eq!(first_page["limit"], 1);
    assert_eq!(first_page["offset"], 0);
    assert_eq!(first_page["has_more"], true);
    assert!(
        first_page["items"][0]["keywords"]
            .as_array()
            .expect("keywords should be an array")
            .len()
            <= 6
    );

    let second_page: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges?limit=1&offset=1"))
        .send()
        .await
        .expect("failed to list second page")
        .error_for_status()
        .expect("second page should succeed")
        .json()
        .await
        .expect("second page JSON should decode");
    assert_eq!(second_page["items"].as_array().expect("items").len(), 1);
    assert_eq!(second_page["total_count"], 2);
    assert_eq!(second_page["limit"], 1);
    assert_eq!(second_page["offset"], 1);
    assert_eq!(second_page["has_more"], false);

    let invalid_limit = client
        .get(api_url(&app, "/api/public/challenges?limit=101"))
        .send()
        .await
        .expect("failed to request invalid limit");
    assert_eq!(invalid_limit.status(), reqwest::StatusCode::BAD_REQUEST);

    let invalid_offset = client
        .get(api_url(&app, "/api/public/challenges?offset=-1"))
        .send()
        .await
        .expect("failed to request invalid offset");
    assert_eq!(invalid_offset.status(), reqwest::StatusCode::BAD_REQUEST);

    let searched: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges?q=route&limit=10"))
        .send()
        .await
        .expect("failed to search challenge catalog")
        .error_for_status()
        .expect("search should succeed")
        .json()
        .await
        .expect("search JSON should decode");
    assert_eq!(searched["total_count"], 1);
    assert_eq!(searched["items"][0]["name"], "grid-routing");

    let keyword_filtered: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges?keyword=grid%20search&limit=10",
        ))
        .send()
        .await
        .expect("failed to filter challenge catalog by keyword")
        .error_for_status()
        .expect("keyword filter should succeed")
        .json()
        .await
        .expect("keyword JSON should decode");
    assert_eq!(keyword_filtered["total_count"], 1);
    assert_eq!(keyword_filtered["items"][0]["name"], "grid-routing");

    let multi_keyword_filtered: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges?keyword=planning&keyword=grid%20search&limit=10",
        ))
        .send()
        .await
        .expect("failed to filter challenge catalog by multiple keywords")
        .error_for_status()
        .expect("multiple keyword filter should succeed")
        .json()
        .await
        .expect("multiple keyword JSON should decode");
    assert_eq!(multi_keyword_filtered["total_count"], 1);
    assert_eq!(multi_keyword_filtered["items"][0]["name"], "grid-routing");

    let mismatched_keywords: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges?keyword=planning&keyword=arithmetic&limit=10",
        ))
        .send()
        .await
        .expect("failed to filter challenge catalog by mismatched keywords")
        .error_for_status()
        .expect("mismatched keyword filter should succeed")
        .json()
        .await
        .expect("mismatched keyword JSON should decode");
    assert_eq!(mismatched_keywords["total_count"], 0);
}

/// Verifies that registration respects active agent quota.
#[sqlx::test(migrations = "../migrations")]
async fn registration_respects_active_agent_quota(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.max_active_agents = 1;
    let app = spawn_app_with_config(pool, config).await;
    let client = reqwest::Client::new();

    let first = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "quota-agent-1" }))
        .send()
        .await
        .expect("failed to register first agent");
    assert_eq!(first.status(), 201);

    let second = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "quota-agent-2" }))
        .send()
        .await
        .expect("failed to register second agent");
    assert_eq!(second.status(), 429);
}

/// Verifies that concurrent registration cannot over-admit active agents.
#[sqlx::test(migrations = "../migrations")]
async fn registration_quota_is_serialized(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let mut config = test_config(storage.path(), &examples_challenges_root());
    config.max_active_agents = 1;
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let first_url = api_url(&app, "/api/agents/register");
    let second_url = first_url.clone();

    let first_client = client.clone();
    let first = async move {
        first_client
            .post(first_url)
            .json(&serde_json::json!({ "display_name": "quota-racer-1" }))
            .send()
            .await
            .expect("first registration should return")
            .status()
    };
    let second = async move {
        client
            .post(second_url)
            .json(&serde_json::json!({ "display_name": "quota-racer-2" }))
            .send()
            .await
            .expect("second registration should return")
            .status()
    };

    let (first_status, second_status) = tokio::join!(first, second);
    let statuses = [first_status, second_status];
    assert!(statuses.contains(&reqwest::StatusCode::CREATED));
    assert!(statuses.contains(&reqwest::StatusCode::TOO_MANY_REQUESTS));

    let active_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::BIGINT FROM agents WHERE status = 'active'")
            .fetch_one(&pool)
            .await
            .expect("active count should query");
    assert_eq!(active_count, 1);
}
