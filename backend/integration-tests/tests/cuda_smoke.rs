//! DGX-only CUDA target smoke coverage.

mod helpers;

use helpers::{
    api_url, run_worker_once, spawn_app_with_config, test_config, zip_project_zip_base64,
};
use shared::config::WorkerAccelerators;
use shared::models::paths::{ManagedBundlePath, ManagedStatementPath};

const CUDA_TARGET: &str = "linux-arm64-cuda";
const CUDA_IMAGE: &str = "ghcr.io/agentic-science/agentics-linux-arm64-cuda:cu130-ubuntu24.04-v0.2.5@sha256:8e3da4a65e297e3b1e9800da001fa2bbac9ed48453a6972117a0c3ad1d1eef13";

/// Verifies that a DGX GPU worker can validate, officially evaluate, persist,
/// and rank a minimal CUDA solution.
#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires DGX GPU access and published Agentics CUDA images"]
async fn dgx_cuda_smoke_completes_official_result_and_leaderboard(pool: sqlx::PgPool) {
    let storage = tempfile::tempdir().expect("failed to create storage tempdir");
    let challenges = tempfile::tempdir().expect("failed to create challenges tempdir");
    let bundles = tempfile::tempdir().expect("failed to create bundle tempdir");
    let (private_bundle, public_bundle) = write_cuda_smoke_bundles(bundles.path());

    let mut config = test_config(storage.path(), challenges.path());
    config.worker_accelerators = WorkerAccelerators::Gpu;
    config.worker_gpu_probe_image = Some(CUDA_IMAGE.to_string());

    let app = spawn_app_with_config(pool.clone(), config.clone()).await;
    publish_cuda_smoke_challenge(&pool, &private_bundle, &public_bundle).await;
    let client = reqwest::Client::new();

    let registration: serde_json::Value = client
        .post(api_url(&app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": "dgx-cuda-smoke-agent" }))
        .send()
        .await
        .expect("failed to register agent")
        .error_for_status()
        .expect("registration should succeed")
        .json()
        .await
        .expect("failed to decode registration");
    let token = registration["token"].as_str().expect("missing token");

    let validation_response = client
        .post(api_url(&app, "/api/agent/validation-runs"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "cuda-smoke",
            "target": CUDA_TARGET,
            "artifact_base64": cuda_solution_zip_base64(),
            "explanation": "DGX CUDA validation smoke"
        }))
        .send()
        .await
        .expect("failed to queue validation");
    let validation_status = validation_response.status();
    let validation_body = validation_response
        .text()
        .await
        .expect("failed to read validation response");
    assert!(
        validation_status.is_success(),
        "validation should queue: status={validation_status}, body={validation_body}"
    );
    let validation: serde_json::Value =
        serde_json::from_str(&validation_body).expect("failed to decode validation");
    let validation_id = validation["id"].as_str().expect("missing validation id");

    run_worker_once(&pool, &config).await;

    let validation_status: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/validation-runs/{validation_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to fetch validation")
        .error_for_status()
        .expect("validation status should be visible")
        .json()
        .await
        .expect("failed to decode validation status");
    assert_eq!(validation_status["status"], "completed");
    assert_eq!(validation_status["evaluation"]["rank_score"], 1.0);

    let official_response = client
        .post(api_url(&app, "/api/agent/solution-submissions"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .json(&serde_json::json!({
            "challenge_name": "cuda-smoke",
            "target": CUDA_TARGET,
            "artifact_base64": cuda_solution_zip_base64(),
            "explanation": "DGX CUDA official smoke"
        }))
        .send()
        .await
        .expect("failed to queue official submission");
    let official_status = official_response.status();
    let official_body = official_response
        .text()
        .await
        .expect("failed to read official response");
    assert!(
        official_status.is_success(),
        "official submission should queue: status={official_status}, body={official_body}"
    );
    let official: serde_json::Value =
        serde_json::from_str(&official_body).expect("failed to decode official submission");
    let submission_id = official["id"].as_str().expect("missing submission id");

    run_worker_once(&pool, &config).await;

    let completed: serde_json::Value = client
        .get(api_url(
            &app,
            &format!("/api/agent/solution-submissions/{submission_id}"),
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Agentics-Admin-Automation", "true")
        .send()
        .await
        .expect("failed to fetch official submission")
        .error_for_status()
        .expect("official submission should be visible")
        .json()
        .await
        .expect("failed to decode official submission");
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["evaluation"]["eval_type"], "official");
    assert_eq!(completed["evaluation"]["rank_score"], 1.0);

    let leaderboard: serde_json::Value = client
        .get(api_url(
            &app,
            "/api/public/challenges/cuda-smoke/leaderboard?target=linux-arm64-cuda",
        ))
        .send()
        .await
        .expect("failed to fetch leaderboard")
        .error_for_status()
        .expect("leaderboard should be public")
        .json()
        .await
        .expect("failed to decode leaderboard");
    let items = leaderboard["items"]
        .as_array()
        .expect("items should be array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["best_solution_submission_id"], submission_id);
    assert_eq!(items[0]["rank_score"], 1.0);
}

async fn publish_cuda_smoke_challenge(
    pool: &sqlx::PgPool,
    private_bundle: &std::path::Path,
    public_bundle: &std::path::Path,
) {
    shared::challenge_bundle::validate_challenge_bundle(private_bundle)
        .await
        .expect("private CUDA smoke bundle should validate");
    let spec = shared::challenge_bundle::read_challenge_bundle_spec(private_bundle)
        .await
        .expect("failed to read CUDA smoke spec");
    let managed_private =
        ManagedBundlePath::from_existing_dir(private_bundle).expect("valid private bundle path");
    let managed_public =
        ManagedBundlePath::from_existing_dir(public_bundle).expect("valid public bundle path");
    let statement_path =
        ManagedStatementPath::from_existing_file(private_bundle.join("statement.md"))
            .expect("valid statement path");
    shared::db::publish_challenge(
        pool,
        &shared::db::PublishChallengeInput {
            challenge_name: &spec.challenge_name,
            bundle_path: &managed_private,
            public_bundle_path: &managed_public,
            statement_path: &statement_path,
            spec: &spec,
            title: &spec.challenge_title,
            summary: &spec.summary,
        },
    )
    .await
    .expect("failed to publish CUDA smoke challenge");
}

fn write_cuda_smoke_bundles(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let private_bundle = root.join("cuda-smoke-private/v1");
    let public_bundle = root.join("cuda-smoke-public/v1");

    for bundle in [&private_bundle, &public_bundle] {
        std::fs::create_dir_all(bundle.join("evaluator")).expect("failed to create evaluator dir");
        std::fs::create_dir_all(bundle.join("public")).expect("failed to create public dir");
        std::fs::write(
            bundle.join("statement.md"),
            "# CUDA Smoke\n\nCompile and run a tiny CUDA kernel.\n",
        )
        .expect("failed to write statement");
        std::fs::write(
            bundle.join("public/runs.json"),
            r#"{"runs":[{"run_name":"public-gpu","interface":"stdio"}]}"#,
        )
        .expect("failed to write public runs");
        std::fs::write(
            bundle.join("evaluator/run.py"),
        r#"from __future__ import annotations

import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--challenge-dir", required=True)
parser.add_argument("--solution-runs-dir", required=True)
parser.add_argument("--output-path", required=True)
parser.add_argument("--mode", required=True)
parser.add_argument("--runs-file", required=True)
args = parser.parse_args()

runs = json.loads(Path(args.runs_file).read_text())["runs"]
results = []
for run in runs:
    stdout = (Path(args.solution_runs_dir) / run["run_name"] / "stdout.txt").read_text().strip()
    passed = stdout == "42"
    results.append({
        "case_name": run["run_name"],
        "status": "passed" if passed else "failed",
        "score": 1.0 if passed else 0.0,
        "message": "CUDA kernel returned 42" if passed else f"unexpected stdout: {stdout}",
    })

passed_count = sum(1 for result in results if result["status"] == "passed")
score = passed_count / len(results)
summary_key = "validation_summary" if args.mode == "validation" else "official_summary"
payload = {
    "status": "passed" if passed_count == len(results) else "failed",
    "rank_score": score,
    "aggregate_metrics": [
        {"metric_name": "score", "value": score},
        {"metric_name": "passed_cases", "value": passed_count},
    ],
    "run_metrics": [
        {"run_name": result["case_name"], "metrics": [{"metric_name": "score", "value": result["score"]}]}
        for result in results
    ],
    summary_key: {"score": score, "passed": passed_count, "total": len(results)},
}
if args.mode == "validation":
    payload["public_results"] = results
Path(args.output_path).write_text(json.dumps(payload))
"#,
        )
        .expect("failed to write evaluator");
    }

    std::fs::create_dir_all(private_bundle.join("private-benchmark"))
        .expect("failed to create private benchmark dir");
    std::fs::write(
        private_bundle.join("private-benchmark/runs.json"),
        r#"{"runs":[{"run_name":"official-gpu","interface":"stdio"}]}"#,
    )
    .expect("failed to write official runs");

    let spec = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "challenge_name": "cuda-smoke",
            "challenge_title": "CUDA Smoke",
            "summary": {
                "en": "Compile and run a tiny CUDA kernel on the DGX GPU target.",
                "zh": "在 DGX GPU target 上编译并运行一个极小的 CUDA kernel。"
            },
            "keywords": ["cuda", "gpu", "smoke"],
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "targets": [{
                "name": CUDA_TARGET,
                "docker_platform": "linux/arm64",
                "accelerator": "gpu",
                "validation_enabled": true,
                "resource_profile": {
                    "name": "agentics-cuda-smoke",
                    "solution_image": {"source": "registry", "reference": CUDA_IMAGE},
                    "evaluator_image": {"source": "registry", "reference": CUDA_IMAGE},
                    "solution": {
                        "setup": {"timeout_sec": 30, "memory_limit_mb": 1024, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                        "build": {"timeout_sec": 30, "memory_limit_mb": 1024, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                        "run": {"timeout_sec": 60, "memory_limit_mb": 1024, "cpu_limit_millis": 2000, "disk_limit_mb": 1024, "network_access": "disabled"}
                    },
                    "evaluator": {
                        "setup": {"timeout_sec": 30, "memory_limit_mb": 1024, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                        "run": {"timeout_sec": 30, "memory_limit_mb": 1024, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                    },
                    "hardware_metadata": {
                        "kind": "cuda",
                        "gpu_model": "NVIDIA GB10",
                        "gpu_count": 1,
                        "gpu_memory_gb": 128,
                        "cuda_variant": "cu130",
                        "cuda_version": "13.0"
                    }
                }
            }],
            "starts_at": "2026-01-01T00:00:00Z",
            "eligibility": {"type": "open"},
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
                    {"name": "score", "label": "Score", "direction": "maximize", "visibility": "public"},
                    {"name": "passed_cases", "label": "Passed Cases", "direction": "maximize", "visibility": "public"}
                ],
                "ranking": {
                    "primary_metric_name": "score",
                    "tie_breaker_metric_names": ["passed_cases"]
                }
            }
        }))
    .expect("failed to serialize spec");
    for bundle in [&private_bundle, &public_bundle] {
        std::fs::write(bundle.join("spec.json"), &spec).expect("failed to write spec");
    }

    (private_bundle, public_bundle)
}

fn cuda_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "DGX CUDA smoke solution",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\nnvcc -std=c++17 kernel.cu -o /io/tmp/kernel\n/io/tmp/kernel\n".to_string(),
        ),
        (
            "kernel.cu",
            r#"#include <cuda_runtime.h>
#include <iostream>

__global__ void write_answer(int* value) {
    *value = 42;
}

int main() {
    int* device_value = nullptr;
    cudaError_t status = cudaMalloc(&device_value, sizeof(int));
    if (status != cudaSuccess) {
        std::cerr << "cudaMalloc failed: " << cudaGetErrorString(status) << "\n";
        return 1;
    }
    write_answer<<<1, 1>>>(device_value);
    status = cudaDeviceSynchronize();
    if (status != cudaSuccess) {
        std::cerr << "kernel failed: " << cudaGetErrorString(status) << "\n";
        cudaFree(device_value);
        return 1;
    }
    int host_value = 0;
    status = cudaMemcpy(&host_value, device_value, sizeof(int), cudaMemcpyDeviceToHost);
    cudaFree(device_value);
    if (status != cudaSuccess) {
        std::cerr << "cudaMemcpy failed: " << cudaGetErrorString(status) << "\n";
        return 1;
    }
    std::cout << host_value << "\n";
    return host_value == 42 ? 0 : 1;
}
"#
            .to_string(),
        ),
    ])
}
