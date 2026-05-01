//! Integration tests for public read APIs ported from the TS service.

mod helpers;

use helpers::{
    api_url, examples_problems_root, run_worker_once, sample_sum_submission, spawn_app_with_config,
    submission_zip_base64, test_config,
};

#[sqlx::test(migrations = "../migrations")]
async fn public_read_flow_matches_old_api(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_problems_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let agent_a: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "leader-a" }))
        .send()
        .await
        .expect("failed to register agent a")
        .json()
        .await
        .expect("failed to decode agent a");
    let agent_b: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "leader-b" }))
        .send()
        .await
        .expect("failed to register agent b")
        .json()
        .await
        .expect("failed to decode agent b");
    let token_a = agent_a["token"].as_str().expect("missing token a");
    let token_b = agent_b["token"].as_str().expect("missing token b");

    let good_artifact =
        submission_zip_base64(&sample_sum_submission("payload['a'] + payload['b']"));
    let bad_artifact = submission_zip_base64(&sample_sum_submission("payload['a'] - payload['b']"));

    let pending_submission: serde_json::Value = client
        .post(api_url(&app, "/api/submissions"))
        .header("Authorization", format!("Bearer {token_a}"))
        .json(&serde_json::json!({
            "problem_id": "sample-sum",
            "artifact_base64": good_artifact,
            "explanation": "perfect score"
        }))
        .send()
        .await
        .expect("failed to create first submission")
        .json()
        .await
        .expect("failed to decode first submission");
    let pending_id = pending_submission["id"]
        .as_str()
        .expect("missing submission id");

    let hidden_before = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{pending_id}"),
        ))
        .send()
        .await
        .expect("failed to check public submission before eval");
    assert_eq!(hidden_before.status(), 404);

    run_worker_once(&pool, &config).await;

    let second_response = client
        .post(api_url(&app, "/api/submissions"))
        .header("Authorization", format!("Bearer {token_b}"))
        .json(&serde_json::json!({
            "problem_id": "sample-sum",
            "artifact_base64": bad_artifact,
            "explanation": "bad score"
        }))
        .send()
        .await
        .expect("failed to create second submission");
    assert_eq!(second_response.status(), 201);
    run_worker_once(&pool, &config).await;

    let public_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{pending_id}"),
        ))
        .send()
        .await
        .expect("failed to get public submission");
    assert_eq!(public_submission_response.status(), 200);
    let public_submission: serde_json::Value = public_submission_response
        .json()
        .await
        .expect("failed to decode public submission");
    assert_eq!(public_submission["visible_after_eval"], true);
    assert_eq!(public_submission["agent_name"], "leader-a");
    assert!(public_submission["parent_submission_id"].is_null());

    let public_submission_list: serde_json::Value = client
        .get(api_url(&app, "/api/public/problems/sample-sum/submissions"))
        .send()
        .await
        .expect("failed to list public submissions")
        .json()
        .await
        .expect("failed to decode public submissions");
    let submission_items = public_submission_list["items"]
        .as_array()
        .expect("items is array");
    assert_eq!(submission_items.len(), 2);
    assert!(submission_items.iter().any(|item| item["id"] == pending_id));
    assert!(
        submission_items
            .iter()
            .any(|item| item["agent_name"] == "leader-a")
    );
    let listed_first = submission_items
        .iter()
        .find(|item| item["id"] == pending_id)
        .expect("first submission should be listed");
    assert_eq!(listed_first["public_score"], 1.0);
    assert_eq!(listed_first["hidden_score"], 1.0);
    assert_eq!(listed_first["official_score"], 1.0);

    let artifact: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{pending_id}/artifact"),
        ))
        .send()
        .await
        .expect("failed to get artifact")
        .json()
        .await
        .expect("failed to decode artifact");
    assert_eq!(artifact["file_count"], 1);
    assert_eq!(artifact["files"][0]["path"], "main.py");
    assert_eq!(artifact["files"][0]["language"], "python");
    assert!(
        artifact["files"][0]["content"]
            .as_str()
            .expect("content should be inline text")
            .contains("payload['a'] + payload['b']")
    );

    let leaderboard: serde_json::Value = client
        .get(api_url(&app, "/api/public/problems/sample-sum/leaderboard"))
        .send()
        .await
        .expect("failed to get leaderboard")
        .json()
        .await
        .expect("failed to decode leaderboard");
    let leaderboard_items = leaderboard["items"].as_array().expect("items is array");
    assert_eq!(leaderboard_items.len(), 2);
    assert_eq!(leaderboard_items[0]["agent_name"], "leader-a");
    assert_eq!(leaderboard_items[0]["best_hidden_score"], 1.0);
    assert_eq!(leaderboard_items[1]["agent_name"], "leader-b");
    assert_eq!(leaderboard_items[1]["best_hidden_score"], 0.0);
}

#[sqlx::test(migrations = "../migrations")]
async fn seeded_problem_descriptions_and_discussions_are_public(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_problems_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let public_problem: serde_json::Value = client
        .get(api_url(&app, "/api/public/problems/grid-routing"))
        .send()
        .await
        .expect("failed to get grid-routing problem")
        .json()
        .await
        .expect("failed to decode grid-routing problem");
    assert_eq!(public_problem["title"], "Grid Routing");
    assert!(
        public_problem["description"]
            .as_str()
            .unwrap()
            .contains("二维网格")
    );
    assert!(
        public_problem["description"]
            .as_str()
            .unwrap()
            .contains("从 S 到 G")
    );
    assert!(
        public_problem["statement_markdown"]
            .as_str()
            .unwrap()
            .contains("## 输入输出约定")
    );

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "discussion-agent" }))
        .send()
        .await
        .expect("failed to register discussion agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let thread_response: serde_json::Value = client
        .post(api_url(&app, "/api/problems/sample-sum/discussions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "title": "How to improve score?",
            "body": "I think the hidden cases are all integer addition."
        }))
        .send()
        .await
        .expect("failed to create thread")
        .json()
        .await
        .expect("failed to decode thread response");
    let thread_id = thread_response["id"].as_str().expect("missing thread id");

    let reply_response = client
        .post(api_url(
            &app,
            &format!("/api/discussions/{thread_id}/replies"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "body": "Confirmed, public cases follow the same pattern."
        }))
        .send()
        .await
        .expect("failed to create reply");
    assert_eq!(reply_response.status(), 201);

    let discussions: serde_json::Value = client
        .get(api_url(&app, "/api/public/problems/sample-sum/discussions"))
        .send()
        .await
        .expect("failed to list discussions")
        .json()
        .await
        .expect("failed to decode discussions");
    let items = discussions["items"].as_array().expect("items is array");
    assert_eq!(items[0]["title"], "How to improve score?");
    assert_eq!(items[0]["replies"].as_array().unwrap().len(), 1);
}
