//! Integration tests for evaluation job leases and stale-worker write guards.

mod helpers;

use helpers::{
    api_url, examples_challenges_root, sample_sum_solution, solution_zip_base64,
    spawn_app_with_config, test_config,
};
use shared::db::{MarkEvaluationStartedInput, PersistedEvaluationResult};
use shared::models::evaluation::{EvaluationStatus, MetricValue, ScoreSummary, ScoringMode};
use shared::models::ids::{EvaluationId, EvaluationJobId, SolutionSubmissionId};
use shared::models::names::MetricName;

/// Verifies that stale running job fails after max attempts.
#[sqlx::test(migrations = "../migrations")]
async fn stale_running_job_fails_after_max_attempts(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "stale-job-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "stale job"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let solution_submission_id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    let solution_submission_id = SolutionSubmissionId::try_new(solution_submission_id)
        .expect("API returned valid solution submission id");

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'running',
            worker_id = 'worker-1',
            claimed_at = NOW() - INTERVAL '10 minutes',
            attempt_count = max_attempts
        WHERE solution_submission_id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .execute(&pool)
    .await
    .expect("failed to mark job stale");

    let result = shared::db::reap_stuck_jobs(&pool, 1)
        .await
        .expect("failed to reap stale jobs");

    assert_eq!(result.requeued, 0);
    assert_eq!(result.failed, 1);

    let states: (String, String) = sqlx::query_as(
        r#"
        SELECT j.status, s.status
        FROM evaluation_jobs j
        JOIN solution_submissions s ON s.id = j.solution_submission_id
        WHERE s.id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to query states");
    assert_eq!(states, ("failed".to_string(), "failed".to_string()));
}

/// Verifies that refreshed job lease is not reaped.
#[sqlx::test(migrations = "../migrations")]
async fn refreshed_job_lease_is_not_reaped(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "lease-refresh-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "lease refresh"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let solution_submission_id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    let solution_submission_id = SolutionSubmissionId::try_new(solution_submission_id)
        .expect("API returned valid solution submission id");
    let job_id: String = sqlx::query_scalar(
        r#"
        UPDATE evaluation_jobs
        SET status = 'running',
            worker_id = 'worker-1',
            claimed_at = NOW() - INTERVAL '10 minutes',
            attempt_count = 1,
            max_attempts = 2
        WHERE solution_submission_id = $1::uuid
        RETURNING id::text AS id
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to mark job running");

    let job_id = EvaluationJobId::try_new(job_id).expect("stored job id is valid");
    let stale_attempt_refreshed =
        shared::db::refresh_evaluation_job_claim(&pool, &job_id, "worker-1", 2)
            .await
            .expect("failed to reject stale attempt refresh");
    assert!(!stale_attempt_refreshed);

    let wrong_worker_refreshed =
        shared::db::refresh_evaluation_job_claim(&pool, &job_id, "worker-2", 1)
            .await
            .expect("failed to reject wrong worker refresh");
    assert!(!wrong_worker_refreshed);

    let refreshed = shared::db::refresh_evaluation_job_claim(&pool, &job_id, "worker-1", 1)
        .await
        .expect("failed to refresh job lease");
    assert!(refreshed);

    let result = shared::db::reap_stuck_jobs(&pool, 1)
        .await
        .expect("failed to reap stale jobs");

    assert_eq!(result.requeued, 0);
    assert_eq!(result.failed, 0);
}

/// Verifies database constraints keep jobs and evaluations on the submission target.
#[sqlx::test(migrations = "../migrations")]
async fn evaluation_rows_cannot_cross_submission_targets(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "target-constraint-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "target consistency"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let solution_submission_id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    let job_id: String = sqlx::query_scalar(
        "SELECT id::text FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("created submission should have staged job");

    let wrong_target_job = sqlx::query(
        r#"
        INSERT INTO evaluation_jobs (
            id, solution_submission_id, challenge_name, target, eval_type, status, payload_json
        )
        VALUES (
            $1::uuid, $2::uuid, 'sample-sum', 'linux-amd64-cpu', 'official', 'queued', '{}'::jsonb
        )
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(solution_submission_id)
    .execute(&pool)
    .await;
    assert!(
        wrong_target_job.is_err(),
        "evaluation jobs must not reference a different target than their submission"
    );

    let wrong_target_evaluation = sqlx::query(
        r#"
        INSERT INTO evaluations (
            id, solution_submission_id, job_id, target, eval_type, status
        )
        VALUES (
            $1::uuid, $2::uuid, $3::uuid, 'linux-amd64-cpu', 'official', 'completed'
        )
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(solution_submission_id)
    .bind(job_id)
    .execute(&pool)
    .await;
    assert!(
        wrong_target_evaluation.is_err(),
        "evaluations must not reference a different target than their job"
    );
}

/// Verifies that stale worker completion cannot overwrite current claim.
#[sqlx::test(migrations = "../migrations")]
async fn stale_worker_completion_cannot_overwrite_current_claim(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "stale-finish-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "stale worker finish"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create response");
    let solution_submission_id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    let solution_submission_id = SolutionSubmissionId::try_new(solution_submission_id)
        .expect("API returned valid solution submission id");

    let first_claim = shared::db::claim_next_evaluation_job(&pool, "worker-a")
        .await
        .expect("failed to claim first job")
        .expect("missing first job");
    assert_eq!(first_claim.solution_submission_id, solution_submission_id);
    assert_eq!(first_claim.attempt_count, 1);
    assert!(
        shared::db::mark_evaluation_started(
            &pool,
            &MarkEvaluationStartedInput {
                evaluation_id: EvaluationId::generate(),
                solution_submission_id: solution_submission_id.clone(),
                job_id: first_claim.id.clone(),
                worker_id: "worker-a".to_string(),
                claim_attempt_count: first_claim.attempt_count,
                target: first_claim.target.clone(),
                eval_type: first_claim.eval_type,
            },
        )
        .await
        .expect("failed to mark first evaluation started")
    );

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET claimed_at = NOW() - INTERVAL '10 minutes',
            max_attempts = 2
        WHERE id = $1::uuid
        "#,
    )
    .bind(first_claim.id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age first claim");
    let reaped = shared::db::reap_stuck_jobs(&pool, 1)
        .await
        .expect("failed to reap first claim");
    assert_eq!(reaped.requeued, 1);
    assert_eq!(reaped.failed, 0);
    let requeued_submission: (String, bool) = sqlx::query_as(
        "SELECT status, visible_after_eval FROM solution_submissions WHERE id = $1::uuid",
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to query requeued submission");
    assert_eq!(requeued_submission, ("queued".to_string(), false));
    let stale_running_evaluations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM evaluations WHERE job_id = $1::uuid AND status = 'running'",
    )
    .bind(first_claim.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to count stale running evaluations");
    assert_eq!(
        stale_running_evaluations, 0,
        "requeue should clear stale running evaluations before a new worker starts"
    );

    let second_claim = shared::db::claim_next_evaluation_job(&pool, "worker-b")
        .await
        .expect("failed to claim second job")
        .expect("missing second job");
    assert_eq!(second_claim.id, first_claim.id);
    assert_eq!(second_claim.attempt_count, 2);
    assert!(
        shared::db::mark_evaluation_started(
            &pool,
            &MarkEvaluationStartedInput {
                evaluation_id: EvaluationId::generate(),
                solution_submission_id: solution_submission_id.clone(),
                job_id: second_claim.id.clone(),
                worker_id: "worker-b".to_string(),
                claim_attempt_count: second_claim.attempt_count,
                target: second_claim.target.clone(),
                eval_type: second_claim.eval_type,
            },
        )
        .await
        .expect("failed to mark second evaluation started"),
        "a requeued job should create a fresh running evaluation for the current claim"
    );

    let stale_failure = persisted_result(
        &first_claim,
        "worker-a",
        &solution_submission_id,
        EvaluationStatus::Failed,
        None,
    );
    assert!(
        !shared::db::mark_evaluation_finished(&pool, &stale_failure)
            .await
            .expect("stale finish should be ignored cleanly")
    );
    let still_running: (String, String, i32) = sqlx::query_as(
        "SELECT status, worker_id, attempt_count FROM evaluation_jobs WHERE id = $1::uuid",
    )
    .bind(first_claim.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to query running job");
    assert_eq!(
        still_running,
        ("running".to_string(), "worker-b".to_string(), 2)
    );

    let current_success = persisted_result(
        &second_claim,
        "worker-b",
        &solution_submission_id,
        EvaluationStatus::Completed,
        Some(1.0),
    );
    assert!(
        shared::db::mark_evaluation_finished(&pool, &current_success)
            .await
            .expect("current finish should persist")
    );
    assert!(
        !shared::db::mark_evaluation_finished(&pool, &stale_failure)
            .await
            .expect("late stale finish should be ignored")
    );

    let final_state: (String, String, String, bool, String, Option<f64>) = sqlx::query_as(
        r#"
        SELECT j.status, j.worker_id, s.status, s.visible_after_eval, e.status, e.rank_score
        FROM evaluation_jobs j
        JOIN solution_submissions s ON s.id = j.solution_submission_id
        JOIN evaluations e ON e.job_id = j.id
        WHERE j.id = $1::uuid
        "#,
    )
    .bind(first_claim.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("failed to query final state");
    assert_eq!(
        final_state,
        (
            "completed".to_string(),
            "worker-b".to_string(),
            "completed".to_string(),
            true,
            "completed".to_string(),
            Some(1.0),
        )
    );
}

/// Verifies that losing official submission does not overwrite leaderboard best metadata.
#[sqlx::test(migrations = "../migrations")]
async fn losing_official_submission_does_not_overwrite_leaderboard_best_metadata(
    pool: sqlx::PgPool,
) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "leaderboard-rerun-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let winning_submission_id = create_official_submission(&client, &app, token, "winner").await;
    finish_next_job_with_score(&pool, &winning_submission_id, "worker-winner", 1.0).await;

    let losing_submission_id = create_official_submission(&client, &app, token, "loser").await;
    finish_next_job_with_score(&pool, &losing_submission_id, "worker-loser", 0.25).await;

    let row: (String, f64, Option<f64>, serde_json::Value) = sqlx::query_as(
        r#"
        SELECT best_solution_submission_id::text AS best_solution_submission_id, best_rank_score, official_score, official_metrics_json
        FROM leaderboard_entries
        WHERE challenge_name = 'sample-sum'
          AND target = 'linux-arm64-cpu'
          AND agent_id = $1::uuid
        "#,
    )
    .bind(
        register_response["agent_id"]
            .as_str()
            .expect("missing agent id"),
    )
    .fetch_one(&pool)
    .await
    .expect("failed to query leaderboard entry");

    assert_eq!(row.0, winning_submission_id.as_str());
    assert_eq!(row.1, 1.0);
    assert_eq!(row.2, Some(1.0));
    assert_eq!(
        row.3,
        serde_json::json!([{ "metric_name": "score", "value": 1.0 }])
    );
}

/// Verifies concurrent official completions keep the best leaderboard result.
#[sqlx::test(migrations = "../migrations")]
async fn concurrent_official_completions_keep_best_leaderboard_result(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "leaderboard-race-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let winning_submission_id = create_official_submission(&client, &app, token, "winner").await;
    let losing_submission_id = create_official_submission(&client, &app, token, "loser").await;
    let winning_claim = claim_and_start_job(&pool, &winning_submission_id, "worker-winner").await;
    let losing_claim = claim_and_start_job(&pool, &losing_submission_id, "worker-loser").await;

    let winning_result = persisted_result(
        &winning_claim,
        "worker-winner",
        &winning_submission_id,
        EvaluationStatus::Completed,
        Some(1.0),
    );
    let losing_result = persisted_result(
        &losing_claim,
        "worker-loser",
        &losing_submission_id,
        EvaluationStatus::Completed,
        Some(0.25),
    );
    let (winning_finished, losing_finished) = tokio::join!(
        shared::db::mark_evaluation_finished(&pool, &winning_result),
        shared::db::mark_evaluation_finished(&pool, &losing_result),
    );
    assert!(winning_finished.expect("winner should finish"));
    assert!(losing_finished.expect("loser should finish"));

    let row: (String, f64) = sqlx::query_as(
        r#"
        SELECT best_solution_submission_id::text AS best_solution_submission_id, best_rank_score
        FROM leaderboard_entries
        WHERE challenge_name = 'sample-sum'
          AND target = 'linux-arm64-cpu'
          AND agent_id = $1::uuid
        "#,
    )
    .bind(
        register_response["agent_id"]
            .as_str()
            .expect("missing agent id"),
    )
    .fetch_one(&pool)
    .await
    .expect("failed to query leaderboard entry");
    assert_eq!(row.0, winning_submission_id.as_str());
    assert_eq!(row.1, 1.0);
}

/// Verifies stale visible official reruns do not erase the previous public result.
#[sqlx::test(migrations = "../migrations")]
async fn stale_visible_official_reruns_preserve_prior_public_result(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "visible-rerun-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let submission_id = create_official_submission(&client, &app, token, "visible rerun").await;
    finish_next_job_with_score(&pool, &submission_id, "worker-original", 1.0).await;

    let failed_rejudge = start_official_rejudge(&pool, &submission_id, "worker-failed-rerun").await;
    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET claimed_at = NOW() - INTERVAL '10 minutes',
            max_attempts = attempt_count
        WHERE id = $1::uuid
        "#,
    )
    .bind(failed_rejudge.id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age failed rejudge");
    let failed = shared::db::reap_stuck_jobs(&pool, 1)
        .await
        .expect("failed to reap stale failed rejudge");
    assert_eq!(failed.requeued, 0);
    assert_eq!(failed.failed, 1);
    assert_visible_submission_and_leaderboard(&pool, &submission_id, 1.0).await;

    let requeued_rejudge =
        start_official_rejudge(&pool, &submission_id, "worker-requeued-rerun").await;
    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET claimed_at = NOW() - INTERVAL '10 minutes',
            max_attempts = attempt_count + 1
        WHERE id = $1::uuid
        "#,
    )
    .bind(requeued_rejudge.id.as_str())
    .execute(&pool)
    .await
    .expect("failed to age requeued rejudge");
    let requeued = shared::db::reap_stuck_jobs(&pool, 1)
        .await
        .expect("failed to reap stale requeued rejudge");
    assert_eq!(requeued.requeued, 1);
    assert_eq!(requeued.failed, 0);
    assert_visible_submission_and_leaderboard(&pool, &submission_id, 1.0).await;
}

/// Creates official submission after validating caller inputs.
async fn create_official_submission(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    token: &str,
    explanation: &str,
) -> SolutionSubmissionId {
    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "sample-sum",
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": explanation
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let id = create_response["id"]
        .as_str()
        .expect("missing solution submission id");
    SolutionSubmissionId::try_new(id).expect("API returned valid solution submission id")
}

/// Queue, claim, and mark one official rejudge running for a visible submission.
async fn start_official_rejudge(
    pool: &sqlx::PgPool,
    solution_submission_id: &SolutionSubmissionId,
    worker_id: &str,
) -> shared::db::EvaluationJobRecord {
    let rejudge_id = EvaluationJobId::generate();
    shared::db::queue_evaluation_job(
        pool,
        &shared::db::QueueEvaluationJobInput {
            job_id: rejudge_id.clone(),
            solution_submission_id: solution_submission_id.clone(),
            eval_type: ScoringMode::Official,
            max_active_official_jobs: None,
        },
    )
    .await
    .expect("official rejudge should queue");
    let claim = shared::db::claim_next_evaluation_job(pool, worker_id)
        .await
        .expect("failed to claim rejudge")
        .expect("missing rejudge");
    assert_eq!(claim.id, rejudge_id);
    assert_eq!(claim.solution_submission_id, *solution_submission_id);
    assert!(
        shared::db::mark_evaluation_started(
            pool,
            &MarkEvaluationStartedInput {
                evaluation_id: EvaluationId::generate(),
                solution_submission_id: solution_submission_id.clone(),
                job_id: claim.id.clone(),
                worker_id: worker_id.to_string(),
                claim_attempt_count: claim.attempt_count,
                target: claim.target.clone(),
                eval_type: claim.eval_type,
            },
        )
        .await
        .expect("failed to mark rejudge started")
    );
    claim
}

/// Assert the prior public result and leaderboard row are still visible.
async fn assert_visible_submission_and_leaderboard(
    pool: &sqlx::PgPool,
    solution_submission_id: &SolutionSubmissionId,
    expected_score: f64,
) {
    let submission: (String, bool) = sqlx::query_as(
        "SELECT status, visible_after_eval FROM solution_submissions WHERE id = $1::uuid",
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(pool)
    .await
    .expect("failed to query visible submission");
    assert_eq!(submission, ("completed".to_string(), true));

    let leaderboard: (String, f64) = sqlx::query_as(
        r#"
        SELECT best_solution_submission_id::text AS best_solution_submission_id, best_rank_score
        FROM leaderboard_entries
        WHERE best_solution_submission_id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .fetch_one(pool)
    .await
    .expect("failed to query leaderboard");
    assert_eq!(leaderboard.0, solution_submission_id.as_str());
    assert_eq!(leaderboard.1, expected_score);
}

/// Handles finish next job with score for this module.
async fn finish_next_job_with_score(
    pool: &sqlx::PgPool,
    solution_submission_id: &SolutionSubmissionId,
    worker_id: &str,
    score: f64,
) {
    let claim = claim_and_start_job(pool, solution_submission_id, worker_id).await;
    let result = persisted_result(
        &claim,
        worker_id,
        solution_submission_id,
        EvaluationStatus::Completed,
        Some(score),
    );
    assert!(
        shared::db::mark_evaluation_finished(pool, &result)
            .await
            .expect("failed to finish evaluation")
    );
}

/// Claim and start the next queued evaluation for a known submission.
async fn claim_and_start_job(
    pool: &sqlx::PgPool,
    solution_submission_id: &SolutionSubmissionId,
    worker_id: &str,
) -> shared::db::EvaluationJobRecord {
    let claim = shared::db::claim_next_evaluation_job(pool, worker_id)
        .await
        .expect("failed to claim job")
        .expect("missing queued job");
    assert_eq!(claim.solution_submission_id, *solution_submission_id);
    assert!(
        shared::db::mark_evaluation_started(
            pool,
            &MarkEvaluationStartedInput {
                evaluation_id: EvaluationId::generate(),
                solution_submission_id: solution_submission_id.clone(),
                job_id: claim.id.clone(),
                worker_id: worker_id.to_string(),
                claim_attempt_count: claim.attempt_count,
                target: claim.target.clone(),
                eval_type: claim.eval_type,
            },
        )
        .await
        .expect("failed to mark evaluation started")
    );
    claim
}

/// Handles persisted result for this module.
fn persisted_result(
    job: &shared::db::EvaluationJobRecord,
    worker_id: &str,
    solution_submission_id: &SolutionSubmissionId,
    status: EvaluationStatus,
    score: Option<f64>,
) -> PersistedEvaluationResult {
    PersistedEvaluationResult {
        solution_submission_id: solution_submission_id.clone(),
        job_id: job.id.clone(),
        worker_id: worker_id.to_string(),
        claim_attempt_count: job.attempt_count,
        target: job.target.clone(),
        eval_type: ScoringMode::Official,
        status,
        primary_score: score,
        rank_score: score,
        aggregate_metrics: score
            .map(|value| {
                vec![MetricValue {
                    metric_name: MetricName::score(),
                    value,
                }]
            })
            .unwrap_or_default(),
        run_metrics: vec![],
        public_results: vec![],
        validation_summary: None,
        official_summary: score.map(|value| ScoreSummary {
            score: value,
            passed: 1,
            total: 1,
        }),
        log_key: None,
        last_error: if status == EvaluationStatus::Failed {
            Some("stale worker failure".to_string())
        } else {
            None
        },
    }
}
