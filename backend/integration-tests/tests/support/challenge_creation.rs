use std::path::Path;
use std::process::Command;

use crate::helpers::{self, TestCreatorSession, api_url, zip_project_zip_base64};
use serde_json::json;

/// Handles creator auth for this module.
pub fn creator_auth(
    request: reqwest::RequestBuilder,
    creator: &TestCreatorSession,
) -> reqwest::RequestBuilder {
    request
        .header("Cookie", &creator.cookie_header)
        .header("X-Agentics-CSRF-Token", &creator.csrf_token)
}

/// Shared request context for validating, approving, and publishing one draft.
pub struct DraftPublishFlow<'a> {
    pub client: &'a reqwest::Client,
    pub app: &'a helpers::TestApp,
    pub creator: &'a TestCreatorSession,
    pub admin_auth: &'a str,
    pub public_repo: &'a Path,
}

/// Creates validate approve publish draft after validating caller inputs.
pub async fn create_validate_approve_publish_draft(
    flow: &DraftPublishFlow<'_>,
    commit_sha: &str,
    pr_number: i32,
    manifest: serde_json::Value,
) -> serde_json::Value {
    let draft = create_draft_with_commit(
        flow.client,
        flow.app,
        flow.creator,
        pr_number,
        manifest,
        commit_sha,
    )
    .await;
    let draft_id = draft["id"].as_str().expect("draft id");
    if draft["request"] != "archive_challenge" {
        creator_auth(
            flow.client.post(api_url(
                flow.app,
                &format!("/api/creator/challenge-drafts/{draft_id}/private-assets"),
            )),
            flow.creator,
        )
        .json(&json!({
            "asset_name": "official-cases",
            "kind": "private_benchmark_data",
            "required": false,
            "asset_base64": private_benchmark_asset_zip_base64()
        }))
        .send()
        .await
        .expect("private asset request")
        .error_for_status()
        .expect("private asset should upload");
    }

    let validated: serde_json::Value = flow
        .client
        .post(api_url(
            flow.app,
            &format!("/admin/challenge-drafts/{draft_id}/validate"),
        ))
        .header("Authorization", flow.admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": flow.public_repo.to_string_lossy() }))
        .send()
        .await
        .expect("validate request")
        .error_for_status()
        .expect("draft should validate")
        .json()
        .await
        .expect("validated draft json");
    flow.client
        .post(api_url(
            flow.app,
            &format!("/admin/challenge-drafts/{draft_id}/approve"),
        ))
        .header("Authorization", flow.admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({
            "message": "approved",
            "expected_validation_bundle_sha256": validated["validation_bundle_sha256"]
        }))
        .send()
        .await
        .expect("approve request")
        .error_for_status()
        .expect("draft should approve");
    flow.client
        .post(api_url(
            flow.app,
            &format!("/admin/challenge-drafts/{draft_id}/publish"),
        ))
        .header("Authorization", flow.admin_auth)
        .header("X-Agentics-Admin-Automation", "true")
        .json(&json!({ "repository_path": flow.public_repo.to_string_lossy() }))
        .send()
        .await
        .expect("publish request")
        .error_for_status()
        .expect("draft should publish")
        .json()
        .await
        .expect("publish json")
}

/// Creates draft after validating caller inputs.
pub async fn create_draft(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    pr_number: i32,
    manifest: serde_json::Value,
) -> serde_json::Value {
    create_draft_with_author(client, app, creator, pr_number, manifest, 1001).await
}

/// Creates a draft whose reviewed commit is an actual Git checkout commit.
pub async fn create_draft_with_commit(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    pr_number: i32,
    manifest: serde_json::Value,
    commit_sha: &str,
) -> serde_json::Value {
    create_draft_with_author_and_commit(client, app, creator, pr_number, manifest, 1001, commit_sha)
        .await
}

/// Creates a draft with an explicit PR author id for ownership boundary tests.
pub async fn create_draft_with_author(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    pr_number: i32,
    manifest: serde_json::Value,
    pr_author_github_user_id: i64,
) -> serde_json::Value {
    let commit_sha = format!("0123456789abcdef0123456789abcdef{pr_number:08x}");
    create_draft_with_author_and_commit(
        client,
        app,
        creator,
        pr_number,
        manifest,
        pr_author_github_user_id,
        &commit_sha,
    )
    .await
}

/// Creates a draft with explicit PR author and commit identity.
pub async fn create_draft_with_author_and_commit(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    creator: &TestCreatorSession,
    pr_number: i32,
    manifest: serde_json::Value,
    pr_author_github_user_id: i64,
    commit_sha: &str,
) -> serde_json::Value {
    creator_auth(
        client.post(api_url(app, "/api/creator/challenge-drafts")),
        creator,
    )
    .json(&json!({
        "repo_url": "https://github.com/agentics-reifying/agentics-challenges",
        "pr_number": pr_number,
        "pr_url": format!("https://github.com/agentics-reifying/agentics-challenges/pull/{pr_number}"),
        "commit_sha": commit_sha,
        "challenge_path": "challenges/sample-sum",
        "pr_author_github_user_id": pr_author_github_user_id,
        "manifest": manifest
    }))
    .send()
    .await
    .expect("draft request")
    .error_for_status()
    .expect("draft should create")
    .json()
    .await
    .expect("draft json")
}

/// Handles register agent for this module.
pub async fn register_agent(pool: &sqlx::PgPool, name: &str) -> String {
    let token = shared::auth::create_agent_token();
    let token_hash = shared::auth::hash_agent_token(&token);
    shared::db::register_agent(
        pool,
        &shared::db::RegisterAgentInput {
            agent_id: shared::models::ids::AgentId::generate(),
            token_id: shared::models::ids::AgentTokenId::generate(),
            token_hash,
            display_name: name.to_string(),
            agent_description: String::new(),
            owner: String::new(),
            model_info: json!({}),
        },
        1_000,
    )
    .await
    .expect("agent should register");
    token
}

/// Writes public challenge to the target path and returns the committed Git HEAD.
pub fn write_public_challenge(repo: &Path) -> String {
    let challenge_root = repo.join("challenges/sample-sum");
    std::fs::create_dir_all(challenge_root.join("v1/public")).expect("public dir");
    write_file(&challenge_root.join("README.md"), "# Sample Sum\n");
    write_file(&challenge_root.join("v1/statement.md"), "# Sample Sum\n");
    write_file(
        &challenge_root.join("v1/public/runs.json"),
        &json!({
            "runs": [
                {
                    "run_name": "case-1",
                    "interface": "stdio",
                    "stdin_json": { "a": 1, "b": 2 },
                    "expected": "3",
                    "output_files": []
                }
            ]
        })
        .to_string(),
    );
    write_file(
        &challenge_root.join("v1/evaluator/run.py"),
        SAMPLE_SUM_EVALUATOR,
    );
    write_file(
        &challenge_root.join("v1/spec.json"),
        &json!({
            "schema_version": 1,
            "challenge_name": "sample-sum",
            "challenge_title": "Sample Sum",
            "summary": { "en": "Add numbers", "zh": "数字求和" },
            "keywords": ["arithmetic", "smoke"],
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "targets": [
                {
                    "name": "linux-arm64-cpu",
                    "docker_platform": "linux/arm64",
                    "accelerator": null,
                    "validation_enabled": true,
                    "resource_profile": {
                        "name": "agentics-cpu-small",
                        "solution_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "evaluator_image": {
                            "source": "local",
                            "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                        },
                        "solution": {
                            "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                            "build": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                            "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                        },
                        "evaluator": {
                            "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                            "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                        }
                    }
                }
            ],
            "starts_at": "2026-01-01T00:00:00Z",
            "eligibility": { "type": "open" },
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_live"
            },
            "solution_publication": "public",
            "execution": {
                "mode": "separated_evaluator",
                "evaluator": {
                    "command": ["python", "evaluator/run.py"],
                    "result_file": "result.json"
                },
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
                        "name": "score",
                        "label": "Score",
                        "direction": "maximize",
                        "visibility": "public"
                    }
                ],
                "ranking": {
                    "primary_metric_name": "score"
                }
            }
        })
        .to_string(),
    );
    write_file(
        &challenge_root.join("agentics.challenge.json"),
        &manifest_json().to_string(),
    );
    commit_all(repo, "create sample-sum")
}

/// Writes archive manifest to the target path.
pub fn write_archive_manifest(repo: &Path) {
    let challenge_root = repo.join("challenges/sample-sum");
    write_file(
        &challenge_root.join("agentics.challenge.json"),
        &archive_manifest_json().to_string(),
    );
}

/// Commit every change in the test repository and return the new HEAD SHA.
pub fn commit_all(repo: &Path, message: &str) -> String {
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

/// Handles manifest json for this module.
pub fn manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "new_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
        "keywords": ["arithmetic", "smoke"],
        "readme_path": "README.md",
        "bundle_path": "v1",
        "private_assets": [
            {
                "asset_name": "official-cases",
                "kind": "private_benchmark_data",
                "required": true,
                "required_paths": ["private-benchmark/runs.json"]
            }
        ]
    })
}

/// Handles archive manifest json for this module.
pub fn archive_manifest_json() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request": "archive_challenge",
        "challenge_name": "sample-sum",
        "title": "Sample Sum",
        "summary": { "en": "Add numbers", "zh": "数字求和" },
        "keywords": ["arithmetic", "smoke"],
        "readme_path": "README.md",
        "archive": {
            "reason": "Retired for MVP lifecycle testing"
        }
    })
}

/// Writes file to the target path.
pub fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent dir");
    }
    std::fs::write(path, content).expect("write file");
}

/// Handles private benchmark asset zip base64 for this module.
pub fn private_benchmark_asset_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "private-benchmark/runs.json",
            json!({
                "runs": [
                    {
                        "run_name": "private-benchmark-1",
                        "interface": "stdio",
                        "stdin_json": { "a": 20, "b": 22 },
                        "expected": "42",
                        "output_files": []
                    }
                ]
            })
            .to_string(),
        ),
        (
            "private-benchmark/cases.json",
            json!({ "cases": [{ "case_name": "private-benchmark-1" }] }).to_string(),
        ),
    ])
}

const SAMPLE_SUM_EVALUATOR: &str = r#"from __future__ import annotations

import argparse
import json
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--solution-runs-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["validation", "official"], required=True)
    parser.add_argument("--runs-file", required=True)
    parser.add_argument("--challenge-dir", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    runs = json.loads(Path(args.runs_file).read_text(encoding="utf-8"))["runs"]
    results = []
    for run in runs:
        stdout = (Path(args.solution_runs_dir) / run["run_name"] / "stdout.txt").read_text(encoding="utf-8").strip()
        passed = stdout == str(run["expected"])
        results.append({"case_name": run["run_name"], "status": "passed" if passed else "failed", "score": 1 if passed else 0})
    passed_count = sum(1 for result in results if result["status"] == "passed")
    total = len(results)
    score = 0 if total == 0 else passed_count / total
    payload = {
        "status": "passed" if passed_count == total else "failed",
        "mode": args.mode,
        "rank_score": score,
        "aggregate_metrics": [{"metric_name": "score", "value": score}],
        "run_metrics": [{"run_name": result["case_name"], "metrics": [{"metric_name": "score", "value": result["score"]}]} for result in results],
        "public_results": results if args.mode == "validation" else [],
    }
    if args.mode == "validation":
        payload["validation_summary"] = {"score": score, "passed": passed_count, "total": total}
    else:
        payload["official_summary"] = {"score": score, "passed": passed_count, "total": total}
    Path(args.output_path).write_text(json.dumps(payload), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#;
