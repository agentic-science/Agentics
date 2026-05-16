//! Integration tests for public read APIs ported from the TS service.

mod helpers;

use std::path::Path;

use helpers::{
    api_url, copy_dir_all, examples_challenges_root, run_worker_once, sample_sum_solution,
    solution_zip_base64, spawn_app_with_config, test_config,
};

/// Verifies that public read flow matches public contract.
#[sqlx::test(migrations = "../migrations")]
async fn public_read_flow_matches_public_contract(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let agent_a: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "leader-a" }))
        .send()
        .await
        .expect("failed to register agent a")
        .json()
        .await
        .expect("failed to decode agent a");
    let agent_b: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "leader-b" }))
        .send()
        .await
        .expect("failed to register agent b")
        .json()
        .await
        .expect("failed to decode agent b");
    let token_a = agent_a["token"].as_str().expect("missing token a");
    let token_b = agent_b["token"].as_str().expect("missing token b");

    let good_artifact = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let bad_artifact = solution_zip_base64(&sample_sum_solution("payload['a'] - payload['b']"));

    let pending_solution_submission: serde_json::Value = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token_a}"))
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": good_artifact,
            "explanation": "perfect score"
        }))
        .send()
        .await
        .expect("failed to create first solution_submission")
        .json()
        .await
        .expect("failed to decode first solution_submission");
    let pending_id = pending_solution_submission["id"]
        .as_str()
        .expect("missing solution submission id");

    let not_visible_before = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{pending_id}"),
        ))
        .send()
        .await
        .expect("failed to check public solution submission before eval");
    assert_eq!(not_visible_before.status(), 404);

    run_worker_once(&pool, &config).await;

    let second_response = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token_b}"))
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": bad_artifact,
            "explanation": "bad score"
        }))
        .send()
        .await
        .expect("failed to create second solution_submission");
    assert_eq!(second_response.status(), 201);
    run_worker_once(&pool, &config).await;

    let public_solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{pending_id}"),
        ))
        .send()
        .await
        .expect("failed to get public solution submission");
    assert_eq!(public_solution_submission_response.status(), 200);
    let public_solution_submission: serde_json::Value = public_solution_submission_response
        .json()
        .await
        .expect("failed to decode public solution submission");
    assert_eq!(public_solution_submission["id"], pending_id);

    let public_solution_submission_list: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/solution-submissions?target=linux-arm64-cpu",
        ))
        .send()
        .await
        .expect("failed to list public solution submissions")
        .json()
        .await
        .expect("failed to decode public solution submissions");
    assert_eq!(public_solution_submission_list["total_count"], 2);
    let solution_submission_items = public_solution_submission_list["items"]
        .as_array()
        .expect("items is array");
    assert_eq!(solution_submission_items.len(), 2);
    assert!(
        solution_submission_items
            .iter()
            .any(|item| item["id"] == pending_id)
    );
    assert!(
        solution_submission_items
            .iter()
            .any(|item| item["agent_display_name"] == "leader-a")
    );
    let listed_first = solution_submission_items
        .iter()
        .find(|item| item["id"] == pending_id)
        .expect("first solution submission should be listed");
    assert!(listed_first["validation_score"].is_null());
    assert_eq!(listed_first["official_score"], 1.0);
    assert_eq!(listed_first["rank_score"], 1.0);

    let limited_solution_submissions: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/solution-submissions?limit=1",
        ))
        .send()
        .await
        .expect("failed to list limited public solution submissions")
        .json()
        .await
        .expect("failed to decode limited public solution submissions");
    assert_eq!(
        limited_solution_submissions["items"]
            .as_array()
            .expect("items is array")
            .len(),
        1
    );
    assert_eq!(limited_solution_submissions["total_count"], 2);

    let oversized_list_response = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/solution-submissions?limit=101",
        ))
        .send()
        .await
        .expect("failed to list oversized public solution submissions");
    assert_eq!(oversized_list_response.status(), 400);

    let artifact: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{pending_id}/artifact"),
        ))
        .send()
        .await
        .expect("failed to get artifact")
        .json()
        .await
        .expect("failed to decode artifact");
    assert_eq!(artifact["file_count"], 5);
    let files = artifact["files"].as_array().expect("artifact files");
    let main_py = files
        .iter()
        .find(|file| file["path"] == "main.py")
        .expect("main.py should be present");
    assert_eq!(main_py["language"], "python");
    assert!(
        main_py["content"]
            .as_str()
            .expect("content should be inline text")
            .contains("payload['a'] + payload['b']")
    );

    let leaderboard: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/leaderboard?target=linux-arm64-cpu",
        ))
        .send()
        .await
        .expect("failed to get leaderboard")
        .json()
        .await
        .expect("failed to decode leaderboard");
    let leaderboard_items = leaderboard["items"].as_array().expect("items is array");
    assert_eq!(leaderboard_items.len(), 2);
    assert_eq!(leaderboard_items[0]["agent_display_name"], "leader-a");
    assert_eq!(leaderboard_items[0]["best_rank_score"], 1.0);
    assert_eq!(leaderboard_items[1]["agent_display_name"], "leader-b");
    assert_eq!(leaderboard_items[1]["best_rank_score"], 0.0);

    let limited_leaderboard: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/leaderboard?target=linux-arm64-cpu&limit=1",
        ))
        .send()
        .await
        .expect("failed to get limited leaderboard")
        .json()
        .await
        .expect("failed to decode limited leaderboard");
    assert_eq!(
        limited_leaderboard["items"]
            .as_array()
            .expect("items is array")
            .len(),
        1
    );

    let distribution: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/score-distributions?target=linux-arm64-cpu&metric=score",
        ))
        .send()
        .await
        .expect("failed to get score distribution")
        .json()
        .await
        .expect("failed to decode score distribution");
    assert_eq!(distribution["challenge_name"], "sample-sum");
    assert_eq!(distribution["target"], "linux-arm64-cpu");
    assert_eq!(distribution["metric_name"], "score");
    assert_eq!(distribution["count"], 2);
    assert_eq!(distribution["min"], 0.0);
    assert_eq!(distribution["max"], 1.0);
}

/// Verifies that public artifact respects solution publication policy.
#[sqlx::test(migrations = "../migrations")]
async fn public_artifact_respects_solution_publication_policy(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    write_private_artifact_challenge(challenges.path(), "private-artifact-sum");
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "private-artifact-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let submission: serde_json::Value = client
        .post(api_url(&app, "/api/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": "private-artifact-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']")),
            "explanation": "artifact should stay private"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .error_for_status()
        .expect("solution submission should create")
        .json()
        .await
        .expect("failed to decode solution submission");
    let submission_id = submission["id"].as_str().expect("submission id");

    run_worker_once(&pool, &config).await;

    let detail = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{submission_id}"),
        ))
        .send()
        .await
        .expect("failed to fetch public submission detail");
    assert_eq!(detail.status(), reqwest::StatusCode::OK);

    let artifact = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{submission_id}/artifact"),
        ))
        .send()
        .await
        .expect("failed to fetch public artifact");
    assert_eq!(artifact.status(), reqwest::StatusCode::NOT_FOUND);
}

/// Verifies that seeded challenge summaries and community links are public.
#[sqlx::test(migrations = "../migrations")]
async fn seeded_challenge_summaries_and_community_links_are_public(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let public_challenge: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges/grid-routing"))
        .send()
        .await
        .expect("failed to get grid-routing challenge")
        .json()
        .await
        .expect("failed to decode grid-routing challenge");
    assert_eq!(public_challenge["title"], "Grid Routing");
    assert!(public_challenge["spec"]["community"].is_null());
    assert!(
        public_challenge["summary"]
            .as_str()
            .unwrap()
            .contains("二维网格")
    );
    assert!(
        public_challenge["summary"]
            .as_str()
            .unwrap()
            .contains("从 S 到 G")
    );
    assert!(
        public_challenge["statement_markdown"]
            .as_str()
            .unwrap()
            .contains("## 输入输出约定")
    );

    let sample_sum_challenge: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges/sample-sum"))
        .send()
        .await
        .expect("failed to get sample-sum challenge")
        .json()
        .await
        .expect("failed to decode sample-sum challenge");
    assert_eq!(
        sample_sum_challenge["spec"]["community"]["moltbook_submolt_name"],
        "agentics-sample-sum"
    );
    assert_eq!(
        sample_sum_challenge["spec"]["community"]["moltbook_submolt_url"],
        "https://www.moltbook.com/submolts/agentics-sample-sum"
    );
}

/// Writes private artifact challenge to the target path.
fn write_private_artifact_challenge(root: &Path, challenge_name: &str) {
    let bundle_dir = root.join(challenge_name).join("v1");
    copy_dir_all(
        &examples_challenges_root().join("sample-sum/v1"),
        &bundle_dir,
    );
    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("failed to read spec"))
            .expect("failed to parse spec");
    spec["challenge_name"] = serde_json::json!(challenge_name);
    spec["challenge_title"] = serde_json::json!(challenge_name);
    spec["solution_publication"] = serde_json::json!("private");
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write spec");
}
