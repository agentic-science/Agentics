//! Integration tests for worker-backed official solution submissions and validation runs.

mod helpers;

use std::path::Path;

use agentics_config::Config;
use agentics_domain::storage::StorageKey;
use agentics_storage::{StorageWriteIntent, build_storage, pack_directory_to_tar};
use helpers::{
    admin_service_token_header, api_url, copy_dir_all, examples_challenges_root,
    grid_routing_solution_zip_base64, published_challenge_name, run_worker_once,
    sample_sum_solution, solution_zip_base64, solution_zip_base64_with_scripts,
    spawn_app_with_config, test_config, zip_project_zip_base64,
};

fn write_challenge_manifest(
    challenge_root: &Path,
    challenge_name: &str,
    title: &str,
    summary_en: &str,
    summary_zh: &str,
    keywords: &[&str],
) {
    std::fs::write(
        challenge_root.join("agentics.challenge.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": challenge_name,
            "title": title,
            "summary": {
                "en": summary_en,
                "zh": summary_zh
            },
            "keywords": keywords,
            "readme_path": "v1/statement.md",
            "bundle_path": "v1",
            "private_assets": [],
            "ci": {
                "validate_manifest": true,
                "validate_public_bundle": true,
                "smoke_test_public_validation": true
            }
        }))
        .expect("failed to serialize challenge manifest"),
    )
    .expect("failed to write challenge manifest");
}

/// Creates validation disabled challenge after validating caller inputs.
fn create_validation_disabled_challenge(root: &Path) {
    let source = examples_challenges_root().join("sample-sum/v1");
    let challenge_root = root.join("validation-disabled");
    let bundle_dir = root.join("validation-disabled/v1");
    copy_dir_all(&source, &bundle_dir);
    write_challenge_manifest(
        &challenge_root,
        "validation-disabled",
        "Validation Disabled",
        "A sample sum variant with public validation disabled.",
        "禁用公开验证的 Sample Sum 变体。",
        &["arithmetic", "validation", "admission"],
    );

    let spec_path = bundle_dir.join("spec.json");
    let mut spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("failed to read copied spec"),
    )
    .expect("failed to parse copied spec");
    spec["challenge_name"] = serde_json::json!("validation-disabled");
    spec["challenge_title"] = serde_json::json!("Validation Disabled");
    for target in spec["targets"]
        .as_array_mut()
        .expect("targets should be an array")
    {
        target["validation_enabled"] = serde_json::json!(false);
    }
    std::fs::write(
        &spec_path,
        serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
    )
    .expect("failed to write copied spec");
}

async fn store_challenge_bundle_objects(
    config: &agentics_config::Config,
    challenge_name: &str,
    private_bundle: &Path,
    public_bundle: &Path,
) -> (StorageKey, StorageKey, StorageKey) {
    let storage = build_storage(
        config
            .storage_factory_options()
            .expect("valid storage options"),
    )
    .await
    .expect("storage backend should initialize");
    let temp = tempfile::tempdir().expect("bundle archive tempdir");
    let private_archive = temp.path().join("private.tar");
    let public_archive = temp.path().join("public.tar");
    let bundle_archive_intent = StorageWriteIntent::new(
        "challenge bundle archive",
        config.storage.max_bundle_archive_bytes,
    );
    pack_directory_to_tar(private_bundle, &private_archive, bundle_archive_intent)
        .await
        .expect("pack private challenge bundle");
    pack_directory_to_tar(public_bundle, &public_archive, bundle_archive_intent)
        .await
        .expect("pack public challenge bundle");
    let private_key = StorageKey::try_new(format!(
        "challenge-bundles/{challenge_name}/manual-private.tar"
    ))
    .expect("valid private bundle key");
    let public_key = StorageKey::try_new(format!(
        "challenge-public-bundles/{challenge_name}/manual-public.tar"
    ))
    .expect("valid public bundle key");
    let statement_key =
        StorageKey::try_new(format!("challenge-statements/{challenge_name}/manual.md"))
            .expect("valid statement key");
    storage
        .put_file(
            &private_key,
            &private_archive,
            StorageWriteIntent::new(
                "challenge bundle archive",
                config.storage.max_bundle_archive_bytes,
            ),
        )
        .await
        .expect("store private challenge bundle");
    storage
        .put_file(
            &public_key,
            &public_archive,
            StorageWriteIntent::new(
                "challenge bundle archive",
                config.storage.max_bundle_archive_bytes,
            ),
        )
        .await
        .expect("store public challenge bundle");
    let statement = tokio::fs::read(private_bundle.join("statement.md"))
        .await
        .expect("read challenge statement");
    storage
        .put(
            &statement_key,
            &statement,
            StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
        )
        .await
        .expect("store challenge statement");
    (private_key, public_key, statement_key)
}

/// Read a runner log through configured storage for assertion diagnostics.
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

/// Creates a minimal piped-stdio challenge after validating caller inputs.
fn create_piped_stdio_challenge(root: &Path) {
    let challenge_root = root.join("interactive-sum");
    let bundle_dir = root.join("interactive-sum/v1");
    std::fs::create_dir_all(bundle_dir.join("interactive-evaluator"))
        .expect("failed to create interactive-evaluator dir");
    std::fs::create_dir_all(bundle_dir.join("public")).expect("failed to create public dir");
    std::fs::create_dir_all(bundle_dir.join("private-benchmark"))
        .expect("failed to create private benchmark dir");
    std::fs::write(
        bundle_dir.join("statement.md"),
        "# Interactive Sum\n\nAdd two numbers.\n",
    )
    .expect("failed to write statement");
    write_challenge_manifest(
        &challenge_root,
        "interactive-sum",
        "Interactive Sum",
        "Add numbers through a trusted interactive evaluator.",
        "通过可信交互器完成加法。",
        &["interactive", "stdio", "arithmetic"],
    );
    std::fs::write(
        bundle_dir.join("public/session.json"),
        serde_json::json!({
            "session_name": "public-1",
            "metadata": { "a": 2, "b": 3 }
        })
        .to_string(),
    )
    .expect("failed to write validation session");
    std::fs::write(
        bundle_dir.join("private-benchmark/session.json"),
        serde_json::json!({
            "session_name": "official-1",
            "metadata": { "a": 11, "b": 31 }
        })
        .to_string(),
    )
    .expect("failed to write official session");
    std::fs::write(
        bundle_dir.join("interactive-evaluator/run.py"),
        r#"from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--challenge-dir", required=True)
parser.add_argument("--session-file", required=True)
parser.add_argument("--session-input-dir", required=True)
parser.add_argument("--output-path", required=True)
parser.add_argument("--mode", required=True)
parser.add_argument("--target", required=True)
args = parser.parse_args()

session = json.loads(Path(args.session_file).read_text())
metadata = session.get("metadata", {})
a = int(metadata.get("a", 0))
b = int(metadata.get("b", 0))
print(f"{a} {b}", flush=True)
answer = sys.stdin.readline().strip()
passed = answer == str(a + b)
score = 1.0 if passed else 0.0
summary_key = "validation_summary" if args.mode == "validation" else "official_summary"
payload = {
    "status": "passed" if passed else "failed",
    "aggregate_metrics": [
        {"metric_name": "score", "value": score},
        {"metric_name": "passed_cases", "value": score}
    ],
    summary_key: {"score": score, "passed": 1 if passed else 0, "total": 1},
}
if args.mode == "validation":
    payload["public_results"] = [
        {"case_name": session["session_name"], "status": "passed" if passed else "failed", "score": score, "message": "ok" if passed else "wrong"}
    ]
Path(args.output_path).write_text(json.dumps({
    **payload
}))
"#,
    )
    .expect("failed to write interactive-evaluator");
    std::fs::write(
        bundle_dir.join("spec.json"),
        serde_json::json!({
            "schema_version": 1,
            "challenge_name": "interactive-sum",
            "challenge_title": "Interactive Sum",
            "summary": {
                "en": "Add numbers through a trusted interactive-evaluator.",
                "zh": "通过可信交互器完成加法。"
            },
            "keywords": ["interactive"],
            "solution": {
                "protocol": "zip_project",
                "manifest_file": "agentics.solution.json"
            },
            "targets": [{
                "name": "linux-arm64-cpu",
                "docker_platform": "linux/arm64",
                "accelerator": null,
                "validation_enabled": true,
                "resource_profile": {
                    "name": "python-cpu-small",
                    "solution_image": {
                        "source": "local",
                        "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                    },
                    "evaluator_image": {
                        "source": "local",
                        "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                    },
                    "solution": {
                        "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"},
                        "build": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"},
                        "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"}
                    },
                    "evaluator": {
                        "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"},
                        "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"}
                    }
                }
            }],
            "starts_at": "2026-01-01T00:00:00Z",
            "eligibility": { "type": "open" },
            "validation_submission_limit": 20,
            "visibility": {
                "leaderboard": "public_live",
                "score_distribution": "public_live",
                "result_detail": "submitter_live_public_live"
            },
            "solution_publication": "public",
            "execution": {
                "mode": "piped_stdio",
                "acknowledge_stdio_protocol_framing": true,
                "interactive_evaluator": {
                    "command": ["python", "interactive-evaluator/run.py"],
                    "result_file": "result.json"
                },
                "validation_session": "public/session.json",
                "official_session": "private-benchmark/session.json"
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
                    },
                    {
                        "name": "passed_cases",
                        "label": "Passed Cases",
                        "direction": "maximize",
                        "visibility": "public"
                    }
                ],
                "ranking": {
                    "primary_metric_name": "score",
                    "tie_breaker_metric_names": ["passed_cases"]
                }
            }
        })
        .to_string(),
    )
    .expect("failed to write spec");
}

/// Creates public/private bundle paths for a minimal coexecuted-evaluator challenge.
fn create_coexecuted_benchmark_bundles(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let public_bundle = root.join("coexecuted-public/v1");
    let private_bundle = root.join("coexecuted-private/v1");
    for bundle in [&public_bundle, &private_bundle] {
        std::fs::create_dir_all(bundle.join("coexecuted-evaluator"))
            .expect("failed to create coexecuted-evaluator dir");
        std::fs::create_dir_all(bundle.join("public")).expect("failed to create public dir");
        std::fs::write(
            bundle.join("statement.md"),
            "# Coexecuted Sum\n\nImport participant code from the built workspace.\n",
        )
        .expect("failed to write statement");
        std::fs::write(bundle.join("public/case.json"), r#"{"a":2,"b":3}"#)
            .expect("failed to write public case");
        std::fs::write(
            bundle.join("coexecuted-evaluator/run.py"),
            r#"from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--challenge-dir", required=True)
parser.add_argument("--workspace-dir", required=True)
parser.add_argument("--output-path", required=True)
parser.add_argument("--mode", required=True)
parser.add_argument("--target", required=True)
args = parser.parse_args()

challenge_dir = Path(args.challenge_dir)
private_marker = challenge_dir / "private-benchmark" / "secret.json"
if args.mode == "validation":
    if private_marker.exists():
        raise SystemExit("validation benchmark unexpectedly received private data")
    case = json.loads((challenge_dir / "public" / "case.json").read_text())
else:
    case = json.loads(private_marker.read_text())

module_path = Path(args.workspace_dir) / "solution.py"
spec = importlib.util.spec_from_file_location("agentics_solution", module_path)
module = importlib.util.module_from_spec(spec)
assert spec and spec.loader
spec.loader.exec_module(module)
answer = int(module.solve(case["a"], case["b"]))
passed = answer == int(case["a"]) + int(case["b"])
score = 1.0 if passed else 0.0
summary_key = "validation_summary" if args.mode == "validation" else "official_summary"
payload = {
    "status": "passed" if passed else "failed",
    "aggregate_metrics": [
        {"metric_name": "score", "value": score},
        {"metric_name": "passed_cases", "value": score},
    ],
    summary_key: {"score": score, "passed": 1 if passed else 0, "total": 1},
}
if args.mode == "validation":
    payload["public_results"] = [
        {"case_name": "public-1", "status": "passed" if passed else "failed", "score": score}
    ]
Path(args.output_path).write_text(json.dumps(payload))
"#,
        )
        .expect("failed to write coexecuted-evaluator");
    }
    std::fs::create_dir_all(private_bundle.join("private-benchmark"))
        .expect("failed to create private benchmark dir");
    std::fs::write(
        private_bundle.join("private-benchmark/secret.json"),
        r#"{"a":11,"b":31}"#,
    )
    .expect("failed to write private case");

    let spec = serde_json::json!({
        "schema_version": 1,
        "challenge_name": "coexecuted-sum",
        "challenge_title": "Coexecuted Sum",
        "summary": {
            "en": "Import participant code in a trusted coexecuted-evaluator.",
            "zh": "在可信基准程序中导入参赛代码。"
        },
        "keywords": ["benchmark"],
        "solution": {
            "protocol": "zip_project",
            "manifest_file": "agentics.solution.json"
        },
        "targets": [{
            "name": "linux-arm64-cpu",
            "docker_platform": "linux/arm64",
            "accelerator": null,
            "validation_enabled": true,
            "resource_profile": {
                "name": "python-cpu-small",
                "solution_image": {
                    "source": "local",
                    "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                },
                "evaluator_image": {
                    "source": "local",
                    "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                },
                "solution": {
                    "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"},
                    "build": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"}
                },
                "evaluator": {
                    "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"},
                    "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 256, "network_access": "disabled"}
                }
            }
        }],
        "starts_at": "2026-01-01T00:00:00Z",
        "eligibility": { "type": "open" },
        "validation_submission_limit": 20,
        "visibility": {
            "leaderboard": "public_live",
            "score_distribution": "public_live",
            "result_detail": "submitter_live_public_live"
        },
        "solution_publication": "public",
        "execution": {
            "mode": "coexecuted_benchmark",
            "coexecuted_evaluator": {
                "command": ["python", "coexecuted-evaluator/run.py"],
                "result_file": "result.json"
            },
            "acknowledge_danger": true
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
                },
                {
                    "name": "passed_cases",
                    "label": "Passed Cases",
                    "direction": "maximize",
                    "visibility": "public"
                }
            ],
            "ranking": {
                "primary_metric_name": "score",
                "tie_breaker_metric_names": ["passed_cases"]
            }
        }
    });
    for bundle in [&public_bundle, &private_bundle] {
        std::fs::write(
            bundle.join("spec.json"),
            serde_json::to_string_pretty(&spec).expect("failed to serialize spec"),
        )
        .expect("failed to write spec");
    }

    (public_bundle, private_bundle)
}

/// Build a base64 ZIP containing a coexecuted-evaluator sum solution.
fn coexecuted_sum_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "coexecuted sum solution",
                "commands": {
                    "run": "unused-run.sh"
                }
            })
            .to_string(),
        ),
        (
            "unused-run.sh",
            "#!/usr/bin/env sh\nset -eu\npython solution.py\n".to_string(),
        ),
        (
            "solution.py",
            "def solve(a, b):\n    return a + b\n".to_string(),
        ),
    ])
}

/// Build a base64 ZIP containing an interactive sum solution.
fn piped_stdio_sum_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "interactive sum solution",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\npython main.py\n".to_string(),
        ),
        (
            "main.py",
            "import sys\nline = sys.stdin.readline().strip()\na, b = map(int, line.split())\nprint(a + b, flush=True)\n".to_string(),
        ),
    ])
}

/// Handles register agent token for this module.
async fn register_agent_token(
    client: &reqwest::Client,
    app: &helpers::TestApp,
    name: &str,
) -> String {
    let register_response: serde_json::Value = client
        .post(api_url(app, "/api/agents/register"))
        .json(&serde_json::json!({ "display_name": name }))
        .send()
        .await
        .expect("failed to register agent")
        .json()
        .await
        .expect("failed to decode register response");
    register_response["token"]
        .as_str()
        .expect("missing token")
        .to_string()
}

/// Handles grid routing symlink solution zip base64 for this module.
fn grid_routing_symlink_solution_zip_base64() -> String {
    zip_project_zip_base64(vec![
        (
            "agentics.solution.json",
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "symlink output probe",
                "commands": {
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\nln -sf /etc/passwd \"$AGENTICS_OUTPUT_DIR/path.txt\"\n"
                .to_string(),
        ),
    ])
}

#[path = "public_eval/admission.rs"]
mod admission;
#[path = "public_eval/runner.rs"]
mod runner;
#[path = "public_eval/security.rs"]
mod security;
