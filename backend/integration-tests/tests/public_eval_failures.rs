//! Integration tests for worker failure propagation during evaluation.

mod helpers;

use std::path::Path;

use agentics_config::{Config, OfficialLogRedactionMode};
use agentics_domain::storage::StorageKey;
use agentics_storage::{StorageWriteIntent, build_storage};
use helpers::{
    api_url, copy_dir_all, examples_challenges_root, published_challenge_name, run_worker_once,
    sample_sum_solution, solution_zip_base64, solution_zip_base64_with_scripts,
    spawn_app_with_config, test_config,
};

/// Verifies that worker keeps sanitized logs for failed validation runs.
#[sqlx::test(migrations = "../migrations")]
async fn worker_keeps_failed_validation_run_logs_when_artifact_is_missing(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();

    let register_response: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "worker-failure-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    let token = register_response["token"].as_str().expect("missing token");

    let artifact_base64 = solution_zip_base64(&sample_sum_solution("payload['a'] + payload['b']"));
    let create_response: serde_json::Value = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(&pool, "sample-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "validation eval failure test"
        }))
        .send()
        .await
        .expect("failed to create solution submission")
        .json()
        .await
        .expect("failed to decode create solution submission response");
    let validation_run_id = create_response["id"]
        .as_str()
        .expect("missing validation run id");

    sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET payload_json = jsonb_set(payload_json, '{artifact_key}', to_jsonb($2::text))
        WHERE solution_submission_id = $1::uuid
        "#,
    )
    .bind(validation_run_id)
    .bind("missing/agentics-missing-solution-submission.zip")
    .execute(&pool)
    .await
    .expect("failed to corrupt artifact path");

    run_worker_once(&pool, &config).await;

    let solution_submission_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_run_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get solution submission");
    assert_eq!(solution_submission_response.status(), 200);

    let solution_submission: serde_json::Value = solution_submission_response
        .json()
        .await
        .expect("failed to decode solution submission response");
    assert_eq!(solution_submission["status"], "failed");
    assert_eq!(solution_submission["visible_after_eval"], false);
    assert_eq!(solution_submission["evaluation"]["status"], "failed");
    assert!(
        solution_submission["evaluation"]["runner_log_storage_key"].is_string(),
        "failed validation evaluations should keep a runner log key"
    );
    assert!(
        solution_submission["evaluation"]
            .get("rank_score")
            .is_none()
    );
    assert_eq!(solution_submission["evaluation_job"]["status"], "failed");

    let logs_response = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{validation_run_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to get validation failure logs");
    assert_eq!(logs_response.status(), 200);
    let logs: serde_json::Value = logs_response
        .json()
        .await
        .expect("failed to decode validation logs response");
    assert_eq!(logs["availability"], "available");
    assert!(
        logs["runner_log_storage_key"].is_string(),
        "failed validation logs should return their persisted runner log storage key"
    );
    assert!(
        logs["content"].is_string(),
        "failed validation logs should return submitter-visible log content"
    );
}

/// Verifies public-only official failures preserve runner diagnostics under contract-based redaction.
#[sqlx::test(migrations = "../migrations")]
async fn public_only_official_failure_keeps_diagnostic_logs(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    create_public_only_sample_sum_root(challenges.path());
    let config = test_config(storage.path(), challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let token = register_agent(&client, &app, "public-official-failure-agent").await;
    let sentinel = "PUBLIC_ONLY_OFFICIAL_DIAGNOSTIC";

    let solution_submission_id =
        submit_failing_official_sample_sum(&client, &app, &pool, &token, sentinel).await;

    run_worker_once(&pool, &config).await;

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(&solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query public-only official last_error");
    assert!(
        last_error.contains(sentinel),
        "public-only official last_error should include actionable excerpt: {last_error}"
    );

    let runner_log_storage_key: Option<String> = sqlx::query_scalar(
        "SELECT runner_log_storage_key FROM evaluations WHERE solution_submission_id = $1::uuid",
    )
    .bind(&solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query public-only official log key");
    let runner_log = runner_log_text(&config, runner_log_storage_key.as_deref())
        .await
        .expect("public-only official failure should persist logs");
    assert!(
        runner_log.contains(sentinel),
        "public-only official runner log should retain diagnostics: {runner_log}"
    );

    let logs = submitter_logs(&client, &app, &token, &solution_submission_id).await;
    assert_eq!(logs["availability"], "available");
    assert!(
        logs["runner_log_storage_key"].is_string(),
        "public-only official logs should expose a runner log storage key to the submitter"
    );
    assert!(
        logs["content"]
            .as_str()
            .is_some_and(|content| content.contains(sentinel)),
        "public-only official logs should expose actionable diagnostics: {logs:#}"
    );
}

/// Verifies the operator override redacts official logs even for public-only contracts.
#[sqlx::test(migrations = "../migrations")]
async fn public_only_official_failure_redacts_logs_when_configured_always(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenge tempdir");
    create_public_only_sample_sum_root(challenges.path());
    let mut config = test_config(storage.path(), challenges.path());
    config.runner.official_log_redaction = OfficialLogRedactionMode::Always;
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let token = register_agent(&client, &app, "always-redacted-official-failure-agent").await;

    let solution_submission_id = submit_failing_official_sample_sum(
        &client,
        &app,
        &pool,
        &token,
        "PUBLIC_ONLY_OFFICIAL_ALWAYS_REDACTED",
    )
    .await;

    run_worker_once(&pool, &config).await;

    let logs = submitter_logs(&client, &app, &token, &solution_submission_id).await;
    assert_eq!(logs["availability"], "redacted_by_config");
    assert!(logs["runner_log_storage_key"].is_null());
    assert!(logs["content"].is_null());
}

/// Verifies private official failures redact runner diagnostics under contract-based redaction.
#[sqlx::test(migrations = "../migrations")]
async fn private_official_failure_redacts_diagnostic_logs(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let config = test_config(storage.path(), &examples_challenges_root());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let token = register_agent(&client, &app, "private-official-failure-agent").await;
    let sentinel = "PRIVATE_OFFICIAL_DIAGNOSTIC";

    let solution_submission_id =
        submit_failing_official_sample_sum(&client, &app, &pool, &token, sentinel).await;

    run_worker_once(&pool, &config).await;

    let last_error: String = sqlx::query_scalar(
        "SELECT last_error FROM evaluation_jobs WHERE solution_submission_id = $1::uuid",
    )
    .bind(&solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query private official last_error");
    assert!(
        !last_error.contains(sentinel),
        "private official last_error must not expose sentinel: {last_error}"
    );
    assert!(
        last_error.contains("redacted"),
        "private official last_error should explain redaction: {last_error}"
    );

    let runner_log_storage_key: Option<String> = sqlx::query_scalar(
        "SELECT runner_log_storage_key FROM evaluations WHERE solution_submission_id = $1::uuid",
    )
    .bind(&solution_submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query private official log key");
    let runner_log = runner_log_text(&config, runner_log_storage_key.as_deref())
        .await
        .expect("private official failure should persist redacted logs");
    assert!(
        !runner_log.contains(sentinel),
        "private official runner log must not expose sentinel: {runner_log}"
    );
    assert!(
        runner_log.contains("redacted"),
        "private official runner log should contain redaction notice: {runner_log}"
    );

    let logs = submitter_logs(&client, &app, &token, &solution_submission_id).await;
    assert_eq!(logs["availability"], "redacted_private_official");
    assert!(logs["runner_log_storage_key"].is_null());
    assert!(logs["content"].is_null());
}

/// Copy the sample-sum fixture and make official evaluation use only public manifests.
fn create_public_only_sample_sum_root(root: &Path) {
    copy_dir_all(&examples_challenges_root(), root);
    let spec_path = root.join("sample-sum/v1/spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read sample-sum spec"),
    )
    .expect("failed to parse sample-sum spec");
    spec["execution"]["official_runs"] = serde_json::json!("public/runs.json");
    spec["datasets"]["private_benchmark_enabled"] = serde_json::json!(false);
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize public-only spec"),
    )
    .expect("failed to write public-only spec");
}

/// Register one test agent and return its bearer token.
async fn register_agent(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    display_name: &str,
) -> String {
    let response: serde_json::Value = client
        .post(api_url(app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": display_name }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");

    response["token"]
        .as_str()
        .expect("missing registration token")
        .to_string()
}

/// Submit a sample-sum official solution whose run phase writes a sentinel and exits non-zero.
async fn submit_failing_official_sample_sum(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    pool: &sqlx::PgPool,
    token: &str,
    sentinel: &str,
) -> String {
    let artifact_base64 = solution_zip_base64_with_scripts(
        &sample_sum_solution("payload['a'] + payload['b']"),
        "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n",
        &format!("#!/usr/bin/env sh\nset -eu\necho {sentinel} >&2\nexit 7\n"),
        "#!/usr/bin/env sh\nset -eu\ntest -f build/generated.txt\npython main.py\n",
    );
    let create_response: serde_json::Value = client
        .post(api_url(app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "challenge_name": published_challenge_name(pool, "sample-sum").await,
            "target": "linux-arm64-cpu",
            "artifact_base64": artifact_base64,
            "explanation": "official eval failure test"
        }))
        .send()
        .await
        .expect("failed to create official submission")
        .json()
        .await
        .expect("failed to decode official submission response");

    create_response["id"]
        .as_str()
        .expect("missing solution submission id")
        .to_string()
}

/// Fetch submitter-visible logs for one solution submission.
async fn submitter_logs(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    token: &str,
    solution_submission_id: &str,
) -> serde_json::Value {
    let response = client
        .get(api_url(
            app,
            &format!("/api/agent/solution-submissions/{solution_submission_id}/logs"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("failed to fetch submitter logs");
    assert_eq!(response.status(), 200);
    response
        .json()
        .await
        .expect("failed to decode submitter logs response")
}

/// Read one persisted runner log from the configured storage backend.
async fn runner_log_text(config: &Config, runner_log_storage_key: Option<&str>) -> Option<String> {
    let runner_log_storage_key = runner_log_storage_key?;
    let storage = build_storage(config.storage_factory_options().ok()?)
        .await
        .ok()?;
    let key = StorageKey::try_new(runner_log_storage_key).ok()?;
    storage
        .get(
            &key,
            StorageWriteIntent::new("runner log", config.runner.max_result_log_bytes),
        )
        .await
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}
