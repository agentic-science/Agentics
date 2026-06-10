//! Integration tests for admin publishing, official judging, and visibility controls.

mod helpers;

use std::path::Path;

use helpers::{
    admin_service_token_header, api_url, copy_dir_all, examples_challenges_root,
    published_challenge_name, run_worker_once, sample_sum_solution, solution_zip_base64,
    spawn_app_with_config, test_config,
};

/// Create an admin-published bundle by adapting the legacy `sample-sum` fixture.
fn create_admin_bundle(root: &Path) -> std::path::PathBuf {
    let source = examples_challenges_root().join("sample-sum/v1");
    let challenge_root = root.join("admin-sum");
    let bundle_dir = root.join("admin-sum/v1");
    copy_dir_all(&source, &bundle_dir);

    std::fs::write(
        challenge_root.join("agentics.challenge.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": "admin-sum",
            "title": "Admin Sum",
            "summary": {
                "en": "Official flow test",
                "zh": "官方流程测试"
            },
            "keywords": ["arithmetic", "admin", "official"],
            "readme_path": "v1/statement.md",
            "bundle_path": "v1",
            "private_assets": [],
            "ci": {
                "validate_manifest": true,
                "validate_public_bundle": true,
                "smoke_test_public_validation": true
            }
        }))
        .expect("failed to serialize admin challenge manifest"),
    )
    .expect("failed to write admin challenge manifest");

    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read copied spec"),
    )
    .expect("failed to parse copied spec");
    spec["challenge_name"] = serde_json::json!("admin-sum");
    spec["challenge_title"] = serde_json::json!("Admin Sum");
    spec["summary"] = serde_json::json!({
        "en": "Official flow test",
        "zh": "官方流程测试"
    });
    spec["visibility"]["result_detail"] = serde_json::json!("submitter_live_public_live");
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

/// Verifies that admin official run, rejudge, archive, and disable flow.
#[sqlx::test(migrations = "../migrations")]
async fn admin_official_run_rejudge_archive_and_disable_flow(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    create_admin_bundle(challenges.path());
    let mut config = test_config(storage.path(), challenges.path());
    config.quotas.max_active_official_jobs = 1;
    config.quotas.official_runs_per_agent_challenge_day = 2;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let admin_sum_id = published_challenge_name(&pool, "admin-sum").await;
    let admin_auth = admin_service_token_header(&app);

    let register_a: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "admin-agent-a" }))
        .send()
        .await
        .expect("failed to register agent a")
        .json()
        .await
        .expect("failed to decode agent a");
    let register_b: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "admin-agent-b" }))
        .send()
        .await
        .expect("failed to register agent b")
        .json()
        .await
        .expect("failed to decode agent b");
    let token_a = register_a["token"].as_str().expect("missing token a");
    let token_b = register_b["token"].as_str().expect("missing token b");
    let agent_b_id = register_b["agent_id"].as_str().expect("missing agent b id");

    let perfect_zip = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let private_benchmark_only_zip = solution_zip_base64(&sample_sum_solution(
        "(payload['a'] + payload['b']) if payload['a'] not in (10, 99) else 0",
    ));

    let solution_submission_a: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token_a}"))
        .json(&serde_json::json!({
            "challenge_name": &admin_sum_id,
            "target": "linux-arm64-cpu",
            "artifact_base64": perfect_zip,
            "explanation": "best rank score"
        }))
        .send()
        .await
        .expect("failed to create solution submission a")
        .json()
        .await
        .expect("failed to decode solution submission a");
    let solution_submission_a_id = solution_submission_a["id"]
        .as_str()
        .expect("missing solution submission a id")
        .to_string();
    run_worker_once(&pool, &config).await;

    let solution_submission_b: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token_b}"))
        .json(&serde_json::json!({
            "challenge_name": &admin_sum_id,
            "target": "linux-arm64-cpu",
            "artifact_base64": private_benchmark_only_zip,
            "explanation": "passes private benchmark only"
        }))
        .send()
        .await
        .expect("failed to create solution submission b")
        .json()
        .await
        .expect("failed to decode solution submission b");
    let solution_submission_b_id = solution_submission_b["id"]
        .as_str()
        .expect("missing solution submission b id")
        .to_string();
    run_worker_once(&pool, &config).await;

    let leaderboard_before: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{admin_sum_id}/leaderboard?target=linux-arm64-cpu"),
        ))
        .send()
        .await
        .expect("failed to get leaderboard before official")
        .json()
        .await
        .expect("failed to decode leaderboard before official");
    assert_eq!(
        leaderboard_before["items"][0]["agent_display_name"],
        "admin-agent-a"
    );
    assert_eq!(
        leaderboard_before["items"][1]["agent_display_name"],
        "admin-agent-b"
    );
    assert_eq!(
        leaderboard_before["items"][0]["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );
    assert_eq!(
        leaderboard_before["items"][1]["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );

    let official_run = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{solution_submission_b_id}/official-run"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue official run");
    assert_eq!(official_run.status(), 202);

    let duplicate_official_run = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{solution_submission_b_id}/official-run"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue duplicate official run");
    assert_eq!(duplicate_official_run.status(), 409);

    let capacity_limited_official_run = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{solution_submission_a_id}/official-run"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue capacity-limited official run");
    assert_eq!(capacity_limited_official_run.status(), 429);

    let official_jobs: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT eval_type, status
        FROM evaluation_jobs
        WHERE solution_submission_id = $1::uuid
        ORDER BY created_at ASC
        "#,
    )
    .bind(&solution_submission_b_id)
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

    let solution_submission_after_official: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{solution_submission_b_id}"),
        ))
        .send()
        .await
        .expect("failed to get solution submission after official")
        .json()
        .await
        .expect("failed to decode solution submission after official");
    assert_eq!(
        solution_submission_after_official["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );
    assert!(
        solution_submission_after_official["official_evaluation"]["official_summary"].is_null()
    );

    let leaderboard_after_official: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{admin_sum_id}/leaderboard?target=linux-arm64-cpu"),
        ))
        .send()
        .await
        .expect("failed to get leaderboard after official")
        .json()
        .await
        .expect("failed to decode leaderboard after official");
    assert_eq!(
        leaderboard_after_official["items"][1]["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );

    let rejudge = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{solution_submission_b_id}/rejudge"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to queue rejudge");
    assert_eq!(rejudge.status(), 202);

    let visible_during_rejudge = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{solution_submission_b_id}"),
        ))
        .send()
        .await
        .expect("failed to check solution submission during rejudge")
        .error_for_status()
        .expect("previous official result should remain visible during rejudge")
        .json::<serde_json::Value>()
        .await
        .expect("failed to decode visible submission during rejudge");
    assert_eq!(
        visible_during_rejudge["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );

    let leaderboard_during_rejudge: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/challenges/{admin_sum_id}/leaderboard?target=linux-arm64-cpu"),
        ))
        .send()
        .await
        .expect("failed to get leaderboard during rejudge")
        .json()
        .await
        .expect("failed to decode leaderboard during rejudge");
    assert_eq!(
        leaderboard_during_rejudge["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        leaderboard_during_rejudge["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["best_solution_submission_id"] == solution_submission_b_id)
    );

    run_worker_once(&pool, &config).await;

    let solution_submission_after_rejudge: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{solution_submission_b_id}"),
        ))
        .send()
        .await
        .expect("failed to get solution submission after rejudge")
        .json()
        .await
        .expect("failed to decode solution submission after rejudge");
    assert_eq!(
        solution_submission_after_rejudge["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 1.0 })
    );
    assert!(
        solution_submission_after_rejudge["official_evaluation"]["run_metrics"]
            .as_array()
            .expect("run_metrics should be an array")
            .is_empty()
    );

    let second_participant_submission = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token_b}"))
        .json(&serde_json::json!({
            "challenge_name": &admin_sum_id,
            "target": "linux-arm64-cpu",
            "artifact_base64": perfect_zip,
            "explanation": "second participant-created official submission should still fit quota"
        }))
        .send()
        .await
        .expect("failed to create second participant submission");
    assert_eq!(
        second_participant_submission.status(),
        201,
        "admin official reruns must not consume participant submission quota"
    );
    run_worker_once(&pool, &config).await;

    let admin_sum_id_typed =
        agentics_domain::models::names::ChallengeName::try_new(admin_sum_id.clone())
            .expect("test challenge name is valid");
    agentics_persistence::Repositories::new(&pool)
        .challenges()
        .archive(&admin_sum_id_typed)
        .await
        .expect("challenge should archive");
    let archived_rejudge = client
        .post(api_url(
            &app,
            &format!("/admin/solution-submissions/{solution_submission_b_id}/official-run"),
        ))
        .header("Authorization", &admin_auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("failed to try official run for archived challenge");
    assert_eq!(
        archived_rejudge.status(),
        404,
        "archived challenges must reject admin queued official work"
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
        .get(api_url(&app, "/api/agent/challenges"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .expect("failed to check disabled agent access");
    assert_eq!(disabled_agent_access.status(), 401);
}
