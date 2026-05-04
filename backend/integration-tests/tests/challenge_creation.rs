//! Challenge creation draft lifecycle integration tests.

mod helpers;

use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use helpers::{api_url, basic_auth_header, spawn_app_with_config, test_config};
use serde_json::json;

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_can_be_validated_approved_and_published(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let public_repo = tempfile::tempdir().expect("public repo tempdir");
    write_public_challenge(public_repo.path(), "v1", None);

    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    let client = reqwest::Client::new();
    let token = register_agent(&pool, "creator-agent").await;
    let bearer = format!("Bearer {token}");
    let admin_auth = basic_auth_header(&config.admin_username, &config.admin_password);

    client
        .post(api_url(&app, "/api/challenge-creator/github-identity"))
        .header("Authorization", &bearer)
        .json(&json!({
            "github_user_id": 1001,
            "github_login": "creator"
        }))
        .send()
        .await
        .expect("identity request")
        .error_for_status()
        .expect("identity should link");

    let draft: serde_json::Value = client
        .post(api_url(&app, "/api/challenge-drafts"))
        .header("Authorization", &bearer)
        .json(&json!({
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": 7,
            "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/7",
            "commit_sha": "0123456789abcdef",
            "challenge_path": "challenges/sample-sum",
            "pr_author_github_user_id": 1001,
            "manifest": manifest_json("new_challenge", "v1", None)
        }))
        .send()
        .await
        .expect("draft request")
        .error_for_status()
        .expect("draft should create")
        .json()
        .await
        .expect("draft json");
    assert_eq!(draft["status"], "draft");
    let draft_id = draft["id"].as_str().expect("draft id");

    let asset: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/api/challenge-drafts/{draft_id}/private-assets"),
        ))
        .header("Authorization", &bearer)
        .json(&json!({
            "asset_id": "official-cases",
            "kind": "private_benchmark_data",
            "required": false,
            "asset_base64": STANDARD.encode(b"[]")
        }))
        .send()
        .await
        .expect("asset request")
        .error_for_status()
        .expect("asset should upload")
        .json()
        .await
        .expect("asset json");
    assert_eq!(asset["required"], true);
    assert_eq!(asset["size_bytes"], 2);

    let validated: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate")
        .json()
        .await
        .expect("validated json");
    assert_eq!(validated["status"], "validated");
    assert_eq!(
        validated["validation_records"][0]["status"], "passed",
        "validation record should be persisted"
    );

    client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "message": "looks good" }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve");

    let published: serde_json::Value = client
        .post(api_url(
            &app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", &admin_auth)
        .json(&json!({ "repository_path": public_repo.path().to_string_lossy() }))
        .send()
        .await
        .expect("publish request")
        .error_for_status()
        .expect("draft should publish")
        .json()
        .await
        .expect("published json");
    assert_eq!(published["status"], "published");
    assert_eq!(published["published_challenge_version_id"], "sample-sum:v1");

    let public_challenge: serde_json::Value = client
        .get(api_url(&app, "/api/public/challenges/sample-sum"))
        .send()
        .await
        .expect("public challenge request")
        .error_for_status()
        .expect("published challenge should be visible")
        .json()
        .await
        .expect("public challenge json");
    assert_eq!(public_challenge["current_version"]["version"], "v1");
}

#[sqlx::test(migrations = "../migrations")]
async fn challenge_draft_rejects_mismatched_pr_author(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let token = register_agent(&pool, "creator-agent").await;
    let bearer = format!("Bearer {token}");

    client
        .post(api_url(&app, "/api/challenge-creator/github-identity"))
        .header("Authorization", &bearer)
        .json(&json!({
            "github_user_id": 1001,
            "github_login": "creator"
        }))
        .send()
        .await
        .expect("identity request")
        .error_for_status()
        .expect("identity should link");

    let response = client
        .post(api_url(&app, "/api/challenge-drafts"))
        .header("Authorization", &bearer)
        .json(&json!({
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": 8,
            "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/8",
            "commit_sha": "0123456789abcdef",
            "challenge_path": "challenges/sample-sum",
            "pr_author_github_user_id": 2002,
            "manifest": manifest_json("new_challenge", "v1", None)
        }))
        .send()
        .await
        .expect("draft request");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../migrations")]
async fn private_asset_upload_rejects_duplicate_asset_id(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("storage tempdir");
    let seeded_challenges = tempfile::tempdir().expect("seed tempdir");
    let config = test_config(storage.path(), seeded_challenges.path());
    let app = spawn_app_with_config(pool.clone(), config).await;
    let client = reqwest::Client::new();
    let token = register_agent(&pool, "creator-agent").await;
    let bearer = format!("Bearer {token}");

    client
        .post(api_url(&app, "/api/challenge-creator/github-identity"))
        .header("Authorization", &bearer)
        .json(&json!({
            "github_user_id": 1001,
            "github_login": "creator"
        }))
        .send()
        .await
        .expect("identity request")
        .error_for_status()
        .expect("identity should link");
    let draft: serde_json::Value = client
        .post(api_url(&app, "/api/challenge-drafts"))
        .header("Authorization", &bearer)
        .json(&json!({
            "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
            "pr_number": 9,
            "pr_url": "https://github.com/agentics-reifying/agentics-challenges/pull/9",
            "commit_sha": "0123456789abcdef",
            "challenge_path": "challenges/sample-sum",
            "pr_author_github_user_id": 1001,
            "manifest": manifest_json("new_challenge", "v1", None)
        }))
        .send()
        .await
        .expect("draft request")
        .error_for_status()
        .expect("draft should create")
        .json()
        .await
        .expect("draft json");
    let draft_id = draft["id"].as_str().expect("draft id");

    for expected_status in [reqwest::StatusCode::CREATED, reqwest::StatusCode::CONFLICT] {
        let response = client
            .post(api_url(
                &app,
                &format!("/api/challenge-drafts/{draft_id}/private-assets"),
            ))
            .header("Authorization", &bearer)
            .json(&json!({
                "asset_id": "official-cases",
                "kind": "private_benchmark_data",
                "asset_base64": STANDARD.encode(b"[]")
            }))
            .send()
            .await
            .expect("asset request");
        assert_eq!(response.status(), expected_status);
    }
}

async fn register_agent(pool: &sqlx::PgPool, name: &str) -> String {
    let token = shared::auth::create_agent_token();
    let token_hash = shared::auth::hash_agent_token(&token);
    shared::db::register_agent(
        pool,
        &shared::db::RegisterAgentInput {
            agent_id: uuid::Uuid::new_v4().to_string(),
            token_id: uuid::Uuid::new_v4().to_string(),
            token_hash,
            name: name.to_string(),
            agent_description: String::new(),
            owner: String::new(),
            model_info: json!({}),
        },
    )
    .await
    .expect("agent should register");
    token
}

fn write_public_challenge(repo: &Path, version: &str, supersedes_version: Option<&str>) {
    let challenge_root = repo.join("challenges/sample-sum");
    std::fs::create_dir_all(challenge_root.join(format!("versions/{version}/public")))
        .expect("public dir");
    write_file(&challenge_root.join("README.md"), "# Sample Sum\n");
    write_file(
        &challenge_root.join(format!("versions/{version}/statement.md")),
        "# Sample Sum\n",
    );
    write_file(
        &challenge_root.join(format!("versions/{version}/public/runs.json")),
        &json!({
            "runs": [
                {
                    "run_id": "case-1",
                    "interface": "stdio",
                    "stdin_json": { "a": 1, "b": 2 },
                    "output_files": []
                }
            ]
        })
        .to_string(),
    );
    write_file(
        &challenge_root.join(format!("versions/{version}/spec.json")),
        &json!({
            "schema_version": 1,
            "challenge_id": "sample-sum",
            "challenge_title": "Sample Sum",
            "challenge_summary": "Add numbers",
            "challenge_version": version,
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "scorer": {
                "command": ["python", "scorer/run.py"],
                "result_file": "result.json"
            },
            "benchmark_targets": [
                {
                    "id": "cpu-linux-arm64",
                    "docker_platform": "linux/arm64",
                    "accelerator": "cpu",
                    "validation_enabled": true,
                    "resource_profile": {
                        "id": "python-cpu-small",
                        "solution_image": "python:3.12-slim-bookworm",
                        "scorer_image": "python:3.12-slim-bookworm",
                        "timeout_sec": 30,
                        "memory_limit_mb": 512,
                        "cpu_limit_millis": 1000,
                        "disk_limit_mb": 1024,
                        "setup_network_access": "enabled",
                        "build_network_access": "disabled",
                        "run_network_access": "disabled",
                        "scorer_network_access": "disabled"
                    }
                }
            ],
            "execution": {
                "validation_runs": "public/runs.json",
                "official_runs": "private-benchmark/runs.json"
            },
            "datasets": {
                "public_dir": "public",
                "private_benchmark_dir": "private-benchmark",
                "public_policy": "full",
                "private_benchmark_policy": "score_only",
                "private_benchmark_enabled": true
            },
            "metric_schema": {
                "metrics": [
                    {
                        "id": "score",
                        "label": "Score",
                        "direction": "maximize",
                        "visibility": "public"
                    }
                ],
                "ranking": {
                    "primary_metric_id": "score"
                }
            }
        })
        .to_string(),
    );
    write_file(
        &challenge_root.join("agentics.challenge.json"),
        &manifest_json("new_challenge", version, supersedes_version).to_string(),
    );
}

fn manifest_json(
    request: &str,
    version: &str,
    supersedes_version: Option<&str>,
) -> serde_json::Value {
    let mut version_json = json!({
        "version": version,
        "bundle_path": format!("versions/{version}")
    });
    if let Some(supersedes_version) = supersedes_version {
        version_json["supersedes_version"] = json!(supersedes_version);
    }

    json!({
        "schema_version": 1,
        "request": request,
        "challenge_id": "sample-sum",
        "title": "Sample Sum",
        "summary": "Add numbers",
        "readme_path": "README.md",
        "version": version_json,
        "private_assets": [
            {
                "asset_id": "official-cases",
                "kind": "private_benchmark_data",
                "required": true
            }
        ]
    })
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, content).expect("write file");
}
