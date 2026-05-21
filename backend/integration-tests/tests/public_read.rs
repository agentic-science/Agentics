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
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token_a}"))
        .header("X-Agentics-Admin-Automation", "true")
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
    let first_job_id = pending_solution_submission["evaluation_job_id"]
        .as_str()
        .expect("missing first evaluation job id")
        .to_string();

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
    assert_runner_persisted_only_intended_artifacts(storage.path(), &first_job_id);
    set_official_primary_metric_for_submission(&pool, pending_id, 42.0, 1.0, 1).await;

    let second_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token_b}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": bad_artifact,
            "explanation": "bad score"
        }))
        .send()
        .await
        .expect("failed to create second solution_submission")
        .json()
        .await
        .expect("failed to decode second solution_submission");
    let second_id = second_response["id"]
        .as_str()
        .expect("missing second solution submission id");
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
    assert_eq!(
        public_solution_submission["note"],
        "sample-sum smoke solution"
    );
    assert_eq!(
        public_solution_submission["evaluation"]["eval_type"],
        "official"
    );
    assert_eq!(public_solution_submission["evaluation"]["rank_score"], 1.0);

    insert_validation_evaluation_for_submission(&pool, pending_id, 0.25).await;
    insert_running_official_evaluation_for_submission(&pool, pending_id, 999.0).await;

    let public_solution_submission_after_rejudge: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{pending_id}"),
        ))
        .send()
        .await
        .expect("failed to get public solution submission during rejudge")
        .json()
        .await
        .expect("failed to decode public solution submission during rejudge");
    assert_eq!(
        public_solution_submission_after_rejudge["evaluation"]["rank_score"], 1.0,
        "public detail must keep the latest completed official result during a running rejudge"
    );

    let public_result_report: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/public/solution-submissions/{pending_id}/result-report"),
        ))
        .send()
        .await
        .expect("failed to get public result report")
        .json()
        .await
        .expect("failed to decode public result report");
    assert_eq!(
        public_result_report["solution_submission"]["evaluation"]["eval_type"], "official",
        "result reports must prefer official evaluations over validation evaluations"
    );
    assert_eq!(
        public_result_report["solution_submission"]["evaluation"]["rank_score"],
        1.0
    );
    assert_eq!(
        public_result_report["solution_submission"]["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 42.0 }),
        "official primary metric should preserve the primary metric value separately from rank_score"
    );
    assert_eq!(
        public_result_report["solution_submission"]["note"],
        "sample-sum smoke solution"
    );

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
    assert!(
        listed_first.get("validation_score").is_none(),
        "public lists must not expose validation scores"
    );
    assert_eq!(
        listed_first["official_primary_metric"],
        serde_json::json!({ "metric_name": "score", "value": 42.0 })
    );
    assert_eq!(listed_first["note"], "sample-sum smoke solution");
    assert_eq!(listed_first["rank_score"], 1.0);
    assert!(listed_first.get("aggregate_metrics").is_none());
    assert!(listed_first.get("official_metrics").is_none());

    let public_stats: serde_json::Value = client
        .get(api_url(&app, "/api/public/stats"))
        .send()
        .await
        .expect("failed to load public stats")
        .json()
        .await
        .expect("failed to decode public stats");
    assert_eq!(public_stats["challenge_count"], 2);
    assert_eq!(public_stats["agent_count"], 2);
    assert_eq!(public_stats["solution_submission_count"], 2);

    let missing_target_response = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/solution-submissions?limit=1",
        ))
        .send()
        .await
        .expect("failed to list public solution submissions without target");
    assert_eq!(missing_target_response.status(), 400);

    let limited_solution_submissions: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/solution-submissions?target=linux-arm64-cpu&limit=1",
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
            "/api/public/challenges/sample-sum/solution-submissions?target=linux-arm64-cpu&limit=101",
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
    assert!(
        leaderboard_items[0].get("aggregate_metrics").is_none(),
        "public leaderboard rows must not carry raw aggregate metric arrays"
    );
    assert!(
        leaderboard_items[0].get("official_metrics").is_none(),
        "public leaderboard rows must not carry raw official metric arrays"
    );
    assert_eq!(leaderboard_items[1]["agent_display_name"], "leader-b");
    assert_eq!(leaderboard_items[1]["best_rank_score"], 0.0);

    set_official_primary_metric_for_submission(&pool, second_id, 7.0, 1.0, 3).await;
    let tie_break_leaderboard: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/leaderboard?target=linux-arm64-cpu&limit=1",
        ))
        .send()
        .await
        .expect("failed to get tie-break leaderboard")
        .json()
        .await
        .expect("failed to decode tie-break leaderboard");
    assert_eq!(
        tie_break_leaderboard["items"][0]["agent_display_name"], "leader-b",
        "bounded SQL ordering must apply declared numeric tie-breakers before limit"
    );

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
    assert_eq!(distribution["min"], 7.0);
    assert_eq!(distribution["max"], 42.0);

    let hidden_distribution = client
        .get(api_url(
            &app,
            "/api/public/challenges/sample-sum/score-distributions?target=linux-arm64-cpu&metric=private_metric",
        ))
        .send()
        .await
        .expect("failed to request hidden score distribution");
    assert_eq!(hidden_distribution.status(), 403);
}

/// Confirms runner scratch work is cleaned instead of persisting private I/O trees.
fn assert_runner_persisted_only_intended_artifacts(storage_root: &Path, job_id: &str) {
    let durable_job_dir = storage_root.join("eval-artifacts").join(job_id);
    let log_paths = std::fs::read_dir(&durable_job_dir)
        .expect("durable job artifact directory should exist")
        .map(|entry| {
            entry
                .expect("durable artifact entry")
                .path()
                .join("runner.log")
        })
        .collect::<Vec<_>>();
    assert!(
        log_paths.iter().any(|path| path.exists()),
        "runner log should remain as the intended durable artifact"
    );
    for private_scratch in [
        "source",
        "build-workspace",
        "prepared",
        "solution-runs",
        "evaluator-output",
    ] {
        assert!(
            !durable_job_dir.join(private_scratch).exists(),
            "runner scratch directory `{private_scratch}` must not be durable storage"
        );
    }
    assert!(
        !runner_temp_workspace_exists(job_id),
        "runner temporary workspace should be removed after evaluation"
    );
}

/// Returns whether any attempt-scoped temporary workspace remains for a job.
fn runner_temp_workspace_exists(job_id: &str) -> bool {
    let root = std::env::temp_dir().join("agentics-eval-artifacts");
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{job_id}-"))
    })
}

/// Adjusts official metric fields so public surfaces must distinguish primary metric and rank score.
async fn set_official_primary_metric_for_submission(
    pool: &sqlx::PgPool,
    solution_submission_id: &str,
    primary_metric_value: f64,
    rank_score: f64,
    passed_cases: i64,
) {
    sqlx::query(
        r#"
        UPDATE evaluations
        SET rank_score = $2,
            aggregate_metrics_json = jsonb_build_array(
                jsonb_build_object('metric_name', 'score', 'value', $3),
                jsonb_build_object('metric_name', 'passed_cases', 'value', $4),
                jsonb_build_object('metric_name', 'private_metric', 'value', 999)
            )
        WHERE solution_submission_id = $1::uuid
          AND eval_type = 'official'
        "#,
    )
    .bind(solution_submission_id)
    .bind(rank_score)
    .bind(primary_metric_value)
    .bind(passed_cases)
    .execute(pool)
    .await
    .expect("official evaluation should update");
    sqlx::query(
        r#"
        UPDATE leaderboard_entries
        SET best_rank_score = $2,
            aggregate_metrics_json = jsonb_build_array(
                jsonb_build_object('metric_name', 'score', 'value', $3),
                jsonb_build_object('metric_name', 'passed_cases', 'value', $4),
                jsonb_build_object('metric_name', 'private_metric', 'value', 999)
            ),
            official_metrics_json = jsonb_build_array(
                jsonb_build_object('metric_name', 'score', 'value', $3),
                jsonb_build_object('metric_name', 'private_metric', 'value', 999)
            )
        WHERE best_solution_submission_id = $1::uuid
        "#,
    )
    .bind(solution_submission_id)
    .bind(rank_score)
    .bind(primary_metric_value)
    .bind(passed_cases)
    .execute(pool)
    .await
    .expect("leaderboard entry should update");
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
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
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

/// Verifies that seeded challenge summaries are public.
#[sqlx::test(migrations = "../migrations")]
async fn seeded_challenge_summaries_are_public(pool: sqlx::PgPool) {
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
    assert!(
        public_challenge["summary"]["zh"]
            .as_str()
            .unwrap()
            .contains("二维网格")
    );
    assert!(
        public_challenge["summary"]["zh"]
            .as_str()
            .unwrap()
            .contains("从 S 到 G")
    );
    assert!(
        public_challenge["summary"]["en"]
            .as_str()
            .unwrap()
            .contains("route")
    );
    assert_eq!(
        public_challenge["summary"],
        public_challenge["spec"]["summary"]
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
    assert_eq!(sample_sum_challenge["title"], "Sample Sum");
}

/// Adds a running official rejudge after a completed official result.
async fn insert_running_official_evaluation_for_submission(
    pool: &sqlx::PgPool,
    solution_submission_id: &str,
    rank_score: f64,
) {
    let job_id = uuid::Uuid::new_v4().to_string();
    let evaluation_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (
            id,
            solution_submission_id,
            challenge_name,
            target,
            eval_type,
            status,
            worker_id,
            attempt_count,
            payload_json,
            claimed_at
        )
        VALUES (
            $1::uuid,
            $2::uuid,
            'sample-sum',
            'linux-arm64-cpu',
            'official',
            'running',
            'public-read-rejudge-worker',
            1,
            '{}'::jsonb,
            NOW()
        )
        "#,
    )
    .bind(&job_id)
    .bind(solution_submission_id)
    .execute(pool)
    .await
    .expect("running official job should insert");
    sqlx::query(
        r#"
        INSERT INTO evaluations (
            id,
            solution_submission_id,
            job_id,
            target,
            eval_type,
            status,
            rank_score,
            aggregate_metrics_json,
            official_summary_json,
            started_at
        )
        VALUES (
            $1::uuid,
            $2::uuid,
            $3::uuid,
            'linux-arm64-cpu',
            'official',
            'running',
            $4,
            '[{"metric_name":"score","value":999.0}]'::jsonb,
            '{"score":999.0,"passed":999,"total":999}'::jsonb,
            NOW()
        )
        "#,
    )
    .bind(&evaluation_id)
    .bind(solution_submission_id)
    .bind(&job_id)
    .bind(rank_score)
    .execute(pool)
    .await
    .expect("running official evaluation should insert");
}

/// Adds a validation evaluation to an already evaluated official submission for precedence tests.
async fn insert_validation_evaluation_for_submission(
    pool: &sqlx::PgPool,
    solution_submission_id: &str,
    rank_score: f64,
) {
    let job_id = uuid::Uuid::new_v4().to_string();
    let evaluation_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (
            id,
            solution_submission_id,
            challenge_name,
            target,
            eval_type,
            status,
            payload_json,
            finished_at
        )
        VALUES (
            $1::uuid,
            $2::uuid,
            'sample-sum',
            'linux-arm64-cpu',
            'validation',
            'completed',
            '{}'::jsonb,
            NOW()
        )
        "#,
    )
    .bind(&job_id)
    .bind(solution_submission_id)
    .execute(pool)
    .await
    .expect("validation job should insert");
    sqlx::query(
        r#"
        INSERT INTO evaluations (
            id,
            solution_submission_id,
            job_id,
            target,
            eval_type,
            status,
            rank_score,
            aggregate_metrics_json,
            validation_summary_json,
            finished_at
        )
        VALUES (
            $1::uuid,
            $2::uuid,
            $3::uuid,
            'linux-arm64-cpu',
            'validation',
            'completed',
            $4,
            '[{"metric_name":"score","value":0.25}]'::jsonb,
            '{"score":0.25,"passed":1,"total":4}'::jsonb,
            NOW()
        )
        "#,
    )
    .bind(&evaluation_id)
    .bind(solution_submission_id)
    .bind(&job_id)
    .bind(rank_score)
    .execute(pool)
    .await
    .expect("validation evaluation should insert");
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
