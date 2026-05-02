//! Integration tests for admin publishing, official judging, and visibility controls.

mod helpers;

use std::path::Path;

use helpers::{
    api_url, basic_auth_header, copy_dir_all, examples_challenges_root, run_worker_once,
    sample_sum_submission, spawn_app_with_config, submission_zip_base64, test_config,
};

/// Create an admin-published bundle by adapting the legacy `sample-sum` fixture.
fn create_admin_bundle(root: &Path) -> std::path::PathBuf {
    let source = examples_challenges_root().join("sample-sum/v1");
    let bundle_dir = root.join("admin-sum/v1");
    copy_dir_all(&source, &bundle_dir);

    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read copied spec"),
    )
    .expect("failed to parse copied spec");
    spec["challenge_id"] = serde_json::json!("admin-sum");
    spec["challenge_title"] = serde_json::json!("Admin Sum");
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize admin spec"),
    )
    .expect("failed to write admin spec");

    std::fs::write(
        bundle_dir.join("statement.md"),
        "# Admin Sum\n\n给定两个整数 `a` 和 `b`，输出它们的和。\n",
    )
    .expect("failed to write admin statement");

    bundle_dir
}

#[sqlx::test(migrations = "../migrations")]
async fn admin_official_run_rejudge_hide_and_disable_flow(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    let bundle_root = tempfile::tempdir().expect("failed to create bundle tempdir");
    let bundle_dir = create_admin_bundle(bundle_root.path());
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    let unauthorized = client
        .post(api_url(&app, "/admin/challenges"))
        .json(&serde_json::json!({ "id": "admin-sum", "title": "Admin Sum" }))
        .send()
        .await
        .expect("failed to check admin auth");
    assert_eq!(unauthorized.status(), 401);

    let create_challenge = client
        .post(api_url(&app, "/admin/challenges"))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({
            "id": "admin-sum",
            "title": "Admin Sum",
            "description": "official flow test"
        }))
        .send()
        .await
        .expect("failed to create challenge");
    assert_eq!(create_challenge.status(), 201);

    let publish_version = client
        .post(api_url(&app, "/admin/challenges/admin-sum/versions"))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({ "bundle_path": bundle_dir }))
        .send()
        .await
        .expect("failed to publish challenge version");
    assert_eq!(publish_version.status(), 201);

    let register_a: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "admin-agent-a" }))
        .send()
        .await
        .expect("failed to register agent a")
        .json()
        .await
        .expect("failed to decode agent a");
    let register_b: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "name": "admin-agent-b" }))
        .send()
        .await
        .expect("failed to register agent b")
        .json()
        .await
        .expect("failed to decode agent b");
    let token_a = register_a["token"].as_str().expect("missing token a");
    let token_b = register_b["token"].as_str().expect("missing token b");
    let agent_b_id = register_b["agent_id"].as_str().expect("missing agent b id");

    let perfect_zip = submission_zip_base64(&sample_sum_submission("payload['a'] + payload['b']"));
    let private_benchmark_only_zip = submission_zip_base64(&sample_sum_submission(
        "(payload['a'] + payload['b']) if payload['a'] not in (10, 99) else 0",
    ));

    let submission_a: serde_json::Value = client
        .post(api_url(&app, "/api/submissions"))
        .header("Authorization", format!("Bearer {token_a}"))
        .json(&serde_json::json!({
            "challenge_id": "admin-sum",
            "artifact_base64": perfect_zip,
            "explanation": "best rank score"
        }))
        .send()
        .await
        .expect("failed to create submission a")
        .json()
        .await
        .expect("failed to decode submission a");
    let submission_a_id = submission_a["id"]
        .as_str()
        .expect("missing submission a id")
        .to_string();
    run_worker_once(&pool, &config).await;

    let submission_b: serde_json::Value = client
        .post(api_url(&app, "/api/submissions"))
        .header("Authorization", format!("Bearer {token_b}"))
        .json(&serde_json::json!({
            "challenge_id": "admin-sum",
            "artifact_base64": private_benchmark_only_zip,
            "explanation": "passes private benchmark only"
        }))
        .send()
        .await
        .expect("failed to create submission b")
        .json()
        .await
        .expect("failed to decode submission b");
    let submission_b_id = submission_b["id"]
        .as_str()
        .expect("missing submission b id")
        .to_string();
    run_worker_once(&pool, &config).await;

    let leaderboard_before: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/admin-sum/leaderboard",
        ))
        .send()
        .await
        .expect("failed to get leaderboard before official")
        .json()
        .await
        .expect("failed to decode leaderboard before official");
    assert_eq!(
        leaderboard_before["items"][0]["agent_name"],
        "admin-agent-a"
    );
    assert_eq!(leaderboard_before["items"][0]["best_rank_score"], 1.0);
    assert_eq!(
        leaderboard_before["items"][1]["agent_name"],
        "admin-agent-b"
    );
    assert_eq!(leaderboard_before["items"][1]["best_rank_score"], 1.0);
    assert_eq!(leaderboard_before["items"][0]["official_score"], 1.0);
    assert_eq!(leaderboard_before["items"][1]["official_score"], 1.0);

    let official_run = client
        .post(api_url(
            &app,
            &format!("/admin/submissions/{submission_b_id}/official-run"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue official run");
    assert_eq!(official_run.status(), 202);

    let official_jobs: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT eval_type, status
        FROM evaluation_jobs
        WHERE submission_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(&submission_b_id)
    .fetch_all(&pool)
    .await
    .expect("failed to query official jobs");
    assert_eq!(
        official_jobs,
        vec![
            ("official".to_string(), "completed".to_string()),
            ("official".to_string(), "queued".to_string()),
        ]
    );

    run_worker_once(&pool, &config).await;

    let submission_after_official: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{submission_b_id}"),
        ))
        .send()
        .await
        .expect("failed to get submission after official")
        .json()
        .await
        .expect("failed to decode submission after official");
    assert_eq!(
        submission_after_official["official_evaluation"]["official_summary"]["score"],
        1.0
    );

    let leaderboard_after_official: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/admin-sum/leaderboard",
        ))
        .send()
        .await
        .expect("failed to get leaderboard after official")
        .json()
        .await
        .expect("failed to decode leaderboard after official");
    assert_eq!(
        leaderboard_after_official["items"][1]["official_score"],
        1.0
    );

    let rejudge = client
        .post(api_url(
            &app,
            &format!("/admin/submissions/{submission_b_id}/rejudge"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue rejudge");
    assert_eq!(rejudge.status(), 202);

    let not_visible_during_rejudge = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{submission_b_id}"),
        ))
        .send()
        .await
        .expect("failed to check submission during rejudge");
    assert_eq!(not_visible_during_rejudge.status(), 404);

    run_worker_once(&pool, &config).await;

    let submission_after_rejudge: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{submission_b_id}"),
        ))
        .send()
        .await
        .expect("failed to get submission after rejudge")
        .json()
        .await
        .expect("failed to decode submission after rejudge");
    assert_eq!(
        submission_after_rejudge["official_evaluation"]["official_summary"]["score"],
        1.0
    );

    let hide = client
        .post(api_url(
            &app,
            &format!("/admin/submissions/{submission_a_id}/hide"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to hide submission a");
    assert_eq!(hide.status(), 200);

    let not_visible_submission_a = client
        .get(api_url(
            &app,
            &format!("/api/public/submissions/{submission_a_id}"),
        ))
        .send()
        .await
        .expect("failed to check not-visible submission a");
    assert_eq!(not_visible_submission_a.status(), 404);

    let leaderboard_after_hide: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/admin-sum/leaderboard",
        ))
        .send()
        .await
        .expect("failed to get leaderboard after hide")
        .json()
        .await
        .expect("failed to decode leaderboard after hide");
    assert_eq!(leaderboard_after_hide["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        leaderboard_after_hide["items"][0]["agent_name"],
        "admin-agent-b"
    );
    assert_eq!(
        leaderboard_after_hide["items"][0]["best_submission_id"],
        submission_b_id
    );

    let disable = client
        .post(api_url(
            &app,
            &format!("/admin/agents/{agent_b_id}/disable"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to disable agent b");
    assert_eq!(disable.status(), 200);

    let disabled_agent_access = client
        .get(api_url(&app, "/api/challenges"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .expect("failed to check disabled agent access");
    assert_eq!(disabled_agent_access.status(), 401);
}
