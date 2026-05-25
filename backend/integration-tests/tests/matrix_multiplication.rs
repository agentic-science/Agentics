//! End-to-end coverage for GitHub-backed matrix multiplication challenge creation.

mod helpers;

use std::path::{Path, PathBuf};
use std::process::Command;

use agentics_config::Config;
use agentics_storage::{StorageKey, StorageWriteIntent, build_storage};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use helpers::{
    TestCreatorSession, api_url, basic_auth_header, challenge_repo_root, copy_dir_all,
    create_creator_session, matrix_multiplication_solution_zip_base64, published_challenge_name,
    run_worker_once, spawn_app_with_config, test_config,
};

/// Handles creator auth for this module.
fn creator_auth(
    request: reqwest::RequestBuilder,
    creator: &TestCreatorSession,
) -> reqwest::RequestBuilder {
    request
        .header("Cookie", &creator.cookie_header)
        .header("X-Agentics-CSRF-Token", &creator.csrf_token)
}

/// Verifies that matrix challenge can be published and solved.
#[sqlx::test(migrations = "../migrations")]
async fn matrix_challenge_can_be_published_and_solved(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let empty_challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    let repo = tempfile::tempdir().expect("failed to create challenge repo tempdir");
    copy_dir_all(&challenge_repo_root(), repo.path());
    remove_copied_submodule_git_pointer(repo.path());
    let challenge_root = repo.path().join("challenges/matrix-multiplication");
    normalize_matrix_targets_for_mvp(&challenge_root);
    let private_asset_zip = generate_smoke_private_asset(&challenge_root);
    std::fs::remove_dir_all(challenge_root.join("v1/private-benchmark"))
        .expect("failed to remove generated private benchmark dir from public repo");
    let commit_sha = commit_all(repo.path(), "prepare matrix challenge");

    let config = test_config(storage.path(), empty_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let admin_auth = basic_auth_header(
        &config.admin_username,
        config.expose_admin_password_for_http_basic(),
    );
    let target = native_cpu_target();
    let creator = create_creator_session(&pool, 42, "matrix-creator").await;

    let manifest_path = challenge_root.join("agentics.challenge.json");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("failed to read matrix manifest"),
    )
    .expect("failed to parse matrix manifest");

    let draft: serde_json::Value = creator_auth(
        client.post(api_url(&app, "/api/creator/challenge-drafts")),
        &creator,
    )
    .json(&serde_json::json!({
        "repo_url": "git@github.com:agentics-reifying/agentics-challenges.git",
        "pr_number": 1,
        "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/1",
        "commit_sha": commit_sha,
        "challenge_path": "challenges/matrix-multiplication",
        "pr_author_github_user_id": 42,
        "manifest": manifest
    }))
    .send()
    .await
    .expect("failed to create matrix draft")
    .error_for_status()
    .expect("matrix draft should create")
    .json()
    .await
    .expect("failed to decode matrix draft");
    let draft_id = draft["id"].as_str().expect("missing draft id");

    let asset_bytes = std::fs::read(&private_asset_zip).expect("failed to read private asset zip");
    creator_auth(
        client.post(api_url(
            &app,
            &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
        )),
        &creator,
    )
    .json(&serde_json::json!({
        "asset_name": "official-seed-config",
        "kind": "private_seeds",
        "required": false,
        "asset_base64": STANDARD.encode(asset_bytes)
    }))
    .send()
    .await
    .expect("failed to upload private matrix asset")
    .error_for_status()
    .expect("private asset upload should succeed");

    let validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({ "repository_path": repo.path() }))
        .send()
        .await
        .expect("failed to validate draft")
        .error_for_status()
        .expect("draft validation should pass")
        .json()
        .await
        .expect("failed to decode validation response");

    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "message": "approved for matrix e2e",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
        .send()
        .await
        .expect("failed to approve draft")
        .error_for_status()
        .expect("draft approval should pass");

    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({ "repository_path": repo.path() }))
        .send()
        .await
        .expect("failed to publish draft")
        .error_for_status()
        .expect("draft publish should pass");
    let matrix_challenge_name = published_challenge_name(&pool, "matrix-multiplication").await;

    let participant_register: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "matrix-participant" }))
        .send()
        .await
        .expect("failed to register participant")
        .json()
        .await
        .expect("failed to decode participant registration");
    let participant_token = participant_register["token"]
        .as_str()
        .expect("missing participant token");

    let submission: serde_json::Value = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {participant_token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": matrix_challenge_name,
            "target": target,
            "artifact_base64": matrix_multiplication_solution_zip_base64(),
            "explanation": "C baseline for matrix multiplication"
        }))
        .send()
        .await
        .expect("failed to submit matrix solution")
        .error_for_status()
        .expect("matrix solution submission should queue")
        .json()
        .await
        .expect("failed to decode matrix submission");
    let submission_id = submission["id"].as_str().expect("missing submission id");

    run_worker_once(&pool, &config).await;

    let completed: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{submission_id}"),
        ))
        .header("Authorization", format!("Bearer {participant_token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to get completed matrix submission")
        .error_for_status()
        .expect("completed submission should be visible to owner")
        .json()
        .await
        .expect("failed to decode completed submission");

    let runner_log = if completed["status"] == "completed" {
        String::new()
    } else {
        latest_runner_log(&pool, &config, submission_id).await
    };
    assert_eq!(
        completed["status"],
        "completed",
        "submission response: {}\nrunner log:\n{}",
        serde_json::to_string_pretty(&completed).expect("submission response should serialize"),
        runner_log
    );
    assert_eq!(completed["evaluation"]["eval_type"], "official");
    assert!(
        completed["evaluation"]["aggregate_metrics"]
            .as_array()
            .expect("aggregate metrics should be an array")
            .iter()
            .any(|metric| metric["metric_name"] == "total_wall_time_ms")
    );
    assert_eq!(completed["evaluation"]["official_summary"]["score"], 1.0);
    assert_eq!(
        completed["evaluation"]["run_metrics"],
        serde_json::json!([])
    );
    assert!(
        completed["evaluation"]["rank_score"]
            .as_f64()
            .expect("rank score should be numeric")
            < 0.0
    );

    let run_metrics_json: serde_json::Value = sqlx::query_scalar(
        "SELECT run_metrics_json FROM evaluations WHERE solution_submission_id = $1::uuid AND eval_type = 'official'",
    )
    .bind(submission_id)
    .fetch_one(&pool)
    .await
    .expect("failed to query persisted run metrics");
    let run_metrics = run_metrics_json
        .as_array()
        .expect("run metrics should be an array");
    assert_eq!(run_metrics.len(), 2);
    assert!(run_metrics.iter().all(|run| {
        run["metrics"]
            .as_array()
            .expect("metrics should be an array")
            .iter()
            .any(|metric| metric["metric_name"] == "wall_time_ms")
    }));
}

/// Remove the copied submodule `.git` pointer before creating an isolated test repo.
fn remove_copied_submodule_git_pointer(repo: &Path) {
    let git_path = repo.join(".git");
    if !git_path.exists() {
        return;
    }
    if git_path.is_dir() {
        std::fs::remove_dir_all(&git_path).expect("failed to remove copied .git directory");
    } else {
        std::fs::remove_file(&git_path).expect("failed to remove copied .git pointer");
    }
}

/// Commit every change in the copied challenge repository and return the new HEAD SHA.
fn commit_all(repo: &Path, message: &str) -> String {
    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.email", "tests@example.invalid"]);
    run_git(repo, &["config", "user.name", "Agentics Tests"]);
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "--allow-empty", "-m", message]);
    run_git(repo, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Run a Git command in a test repository and panic with stderr if it fails.
fn run_git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git command should start");
    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).expect("git output should be UTF-8")
}

/// Handles native cpu target for this module.
fn native_cpu_target() -> &'static str {
    "linux-arm64-cpu"
}

/// Handles normalize matrix targets for mvp for this module.
fn normalize_matrix_targets_for_mvp(challenge_root: &Path) {
    let spec_path = challenge_root.join("v1/spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read matrix spec"),
    )
    .expect("failed to parse matrix spec");
    let targets = spec["targets"]
        .as_array_mut()
        .expect("matrix spec targets should be an array");
    targets.retain(|target| target["docker_platform"] == "linux/arm64");
    let target = targets
        .first_mut()
        .expect("matrix spec should retain one arm64 target");
    target["name"] = serde_json::Value::String("linux-arm64-cpu".to_string());
    for stage_group in ["solution", "evaluator"] {
        for stage in ["setup", "build", "run"] {
            if let Some(network_access) = target
                .get_mut("resource_profile")
                .and_then(|profile| profile.get_mut(stage_group))
                .and_then(|stages| stages.get_mut(stage))
                .and_then(|stage| stage.get_mut("network_access"))
            {
                *network_access = serde_json::Value::String("disabled".to_string());
            }
        }
    }
    std::fs::write(
        &spec_path,
        serde_json::to_vec_pretty(&spec).expect("matrix spec should serialize"),
    )
    .expect("failed to write normalized matrix spec");
}

/// Handles generate smoke private asset for this module.
fn generate_smoke_private_asset(challenge_root: &Path) -> PathBuf {
    let output_zip = challenge_root
        .parent()
        .expect("challenge root should have parent")
        .join("matrix-smoke.private-assets.zip");
    let status = std::process::Command::new("python3")
        .arg(challenge_root.join("tools/generate_assets.py"))
        .arg("--root")
        .arg(challenge_root)
        .arg("--preset")
        .arg("official-config")
        .arg("--square-cases")
        .arg("1")
        .arg("--rect-cases")
        .arg("1")
        .arg("--zip")
        .arg(&output_zip)
        .status()
        .expect("failed to run matrix asset generator");
    assert!(status.success(), "matrix asset generator failed");
    output_zip
}

/// Read latest runner log for a failed matrix evaluation.
async fn latest_runner_log(pool: &sqlx::PgPool, config: &Config, submission_id: &str) -> String {
    let log_key: Option<String> = sqlx::query_scalar(
        "SELECT log_key FROM evaluations WHERE solution_submission_id = $1::uuid ORDER BY finished_at DESC NULLS LAST LIMIT 1",
    )
    .bind(submission_id)
    .fetch_optional(pool)
    .await
    .expect("failed to fetch matrix evaluation log key");
    let Some(log_key) = log_key else {
        return "<missing log key>".to_string();
    };
    let storage = match build_storage(config).await {
        Ok(storage) => storage,
        Err(error) => return format!("<failed to initialize storage: {error}>"),
    };
    let key = match StorageKey::try_new(log_key) {
        Ok(key) => key,
        Err(error) => return format!("<invalid runner log key: {error}>"),
    };
    let intent = StorageWriteIntent::new("runner log", config.runner_max_result_log_bytes);
    storage
        .get(&key, intent)
        .await
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_else(|error| format!("<failed to read runner log: {error}>"))
}
