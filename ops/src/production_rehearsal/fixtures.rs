//! Dedicated challenge and solution fixtures for production rehearsal.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use zip::write::SimpleFileOptions;

use super::ProductionRehearsalError;
use super::report::RehearsalChallengeEvidence;

const TARGET_CPU: &str = "linux-arm64-cpu";

/// Rehearsal fixture names and bundle root.
#[derive(Debug, Clone)]
pub(super) struct RehearsalFixtures {
    pub(super) root: PathBuf,
    pub(super) separated: RehearsalChallengeEvidence,
    pub(super) piped_stdio: RehearsalChallengeEvidence,
    pub(super) coexecuted: RehearsalChallengeEvidence,
}

impl RehearsalFixtures {
    /// All CPU fixture challenges.
    pub(super) fn cpu_challenges(&self) -> Vec<RehearsalChallengeEvidence> {
        vec![
            self.separated.clone(),
            self.piped_stdio.clone(),
            self.coexecuted.clone(),
        ]
    }
}

/// Runner image reference used by generated challenge fixtures.
#[derive(Debug, Clone)]
pub(super) struct RehearsalImageConfig {
    pub(super) source: String,
    pub(super) reference: String,
}

/// Write all dedicated rehearsal challenge bundles under `work_root`.
pub(super) fn write_rehearsal_fixtures(
    work_root: &Path,
    run_id: &str,
    image: &RehearsalImageConfig,
) -> Result<RehearsalFixtures, ProductionRehearsalError> {
    let root = work_root.join("challenges");
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;

    let separated = RehearsalChallengeEvidence {
        name: format!("rehearse-{run_id}-sep"),
        title: "Rehearsal Separated Evaluator".to_string(),
        mode: "separated_evaluator".to_string(),
        target: TARGET_CPU.to_string(),
    };
    let piped_stdio = RehearsalChallengeEvidence {
        name: format!("rehearse-{run_id}-pipe"),
        title: "Rehearsal Piped Stdio".to_string(),
        mode: "piped_stdio".to_string(),
        target: TARGET_CPU.to_string(),
    };
    let coexecuted = RehearsalChallengeEvidence {
        name: format!("rehearse-{run_id}-coexec"),
        title: "Rehearsal Coexecuted Benchmark".to_string(),
        mode: "coexecuted_benchmark".to_string(),
        target: TARGET_CPU.to_string(),
    };

    write_separated_bundle(&root, &separated, image)?;
    write_piped_stdio_bundle(&root, &piped_stdio, image)?;
    write_coexecuted_bundle(&root, &coexecuted, image)?;

    Ok(RehearsalFixtures {
        root,
        separated,
        piped_stdio,
        coexecuted,
    })
}

/// Build a normal separated-evaluator solution ZIP.
pub(super) fn separated_solution_zip_base64() -> Result<String, ProductionRehearsalError> {
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "production rehearsal separated evaluator solution",
                "commands": {
                    "setup": "scripts/setup.sh",
                    "build": "scripts/build.sh",
                    "run": "run.sh"
                }
            })
            .to_string(),
        ),
        (
            "scripts/setup.sh",
            "#!/usr/bin/env sh\nset -eu\nprintf setup > .setup-marker\n".to_string(),
        ),
        (
            "scripts/build.sh",
            "#!/usr/bin/env sh\nset -eu\nmkdir -p build\nprintf built > build/generated.txt\n"
                .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\ntest -f build/generated.txt\npython main.py\n".to_string(),
        ),
        (
            "main.py",
            sample_sum_solution("payload['a'] + payload['b']"),
        ),
    ])
}

/// Build a piped-stdio solution ZIP.
pub(super) fn piped_stdio_solution_zip_base64() -> Result<String, ProductionRehearsalError> {
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "production rehearsal piped stdio solution",
                "commands": { "run": "run.sh" }
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

/// Build a coexecuted benchmark solution ZIP.
pub(super) fn coexecuted_solution_zip_base64() -> Result<String, ProductionRehearsalError> {
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "production rehearsal coexecuted benchmark solution",
                "commands": { "run": "unused-run.sh" }
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

/// Build an oversized-note solution ZIP that should be rejected before durable storage.
pub(super) fn oversized_note_zip_base64() -> Result<String, ProductionRehearsalError> {
    let note = "x".repeat(1100);
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": note,
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        ),
        ("run.sh", "#!/usr/bin/env sh\nexit 0\n".to_string()),
    ])
}

/// Build an unsafe traversal ZIP that should be rejected.
pub(super) fn traversal_zip_base64() -> Result<String, ProductionRehearsalError> {
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "unsafe traversal rehearsal",
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        ),
        ("../evil.txt", "nope".to_string()),
        ("run.sh", "#!/usr/bin/env sh\nexit 0\n".to_string()),
    ])
}

/// Build a run-stage network probe solution. It should fail under disabled network policy.
pub(super) fn network_probe_zip_base64() -> Result<String, ProductionRehearsalError> {
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "production rehearsal network probe",
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\npython - <<'PY'\nimport json\nimport socket\nimport sys\nsocket.create_connection(('1.1.1.1', 53), timeout=3)\npayload = json.loads(sys.stdin.read())\nprint(payload['a'] + payload['b'])\nPY\n"
                .to_string(),
        ),
    ])
}

/// Build a private-data probe that succeeds only if private paths are absent from participant runs.
pub(super) fn private_data_probe_zip_base64() -> Result<String, ProductionRehearsalError> {
    zip_base64(&[
        (
            "agentics.solution.json",
            json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "production rehearsal private data probe",
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        ),
        (
            "run.sh",
            "#!/usr/bin/env sh\nset -eu\nif [ -e /challenge/private-benchmark/runs.json ] || [ -e /private-benchmark/runs.json ]; then echo private-data-visible >&2; exit 7; fi\npython main.py\n"
                .to_string(),
        ),
        ("main.py", sample_sum_solution("payload['a'] + payload['b']")),
    ])
}

fn write_separated_bundle(
    root: &Path,
    challenge: &RehearsalChallengeEvidence,
    image: &RehearsalImageConfig,
) -> Result<(), ProductionRehearsalError> {
    let bundle = root.join(&challenge.name).join("v1");
    fs::create_dir_all(bundle.join("separated-evaluator"))?;
    fs::create_dir_all(bundle.join("public"))?;
    fs::create_dir_all(bundle.join("private-benchmark"))?;
    fs::write(
        bundle.join("statement.md"),
        "# Rehearsal Separated Evaluator\n\nAdd evaluator-controlled pairs.\n",
    )?;
    fs::write(
        bundle.join("public/runs.json"),
        json!({
            "runs": [
                {"run_name": "public-1", "interface": "stdio", "stdin_json": {"a": 1, "b": 2}, "expected": "3"},
                {"run_name": "public-2", "interface": "stdio", "stdin_json": {"a": -5, "b": 12}, "expected": "7"}
            ]
        })
        .to_string(),
    )?;
    fs::write(
        bundle.join("private-benchmark/runs.json"),
        json!({
            "runs": [
                {"run_name": "official-1", "interface": "stdio", "stdin_json": {"a": 20, "b": 22}, "expected": "42"},
                {"run_name": "official-2", "interface": "stdio", "stdin_json": {"a": -100, "b": 58}, "expected": "-42"}
            ]
        })
        .to_string(),
    )?;
    fs::write(
        bundle.join("separated-evaluator/run.py"),
        separated_evaluator_py(),
    )?;
    fs::write(
        bundle.join("spec.json"),
        serde_json::to_string_pretty(&base_spec_json(
            challenge,
            image,
            json!({
                "mode": "separated_evaluator",
                "separated_evaluator": {
                    "command": ["python", "separated-evaluator/run.py"],
                    "result_file": "result.json"
                },
                "validation_runs": "public/runs.json",
                "official_runs": "private-benchmark/runs.json"
            }),
            true,
            true,
        ))?,
    )?;
    Ok(())
}

fn write_piped_stdio_bundle(
    root: &Path,
    challenge: &RehearsalChallengeEvidence,
    image: &RehearsalImageConfig,
) -> Result<(), ProductionRehearsalError> {
    let bundle = root.join(&challenge.name).join("v1");
    fs::create_dir_all(bundle.join("interactive-evaluator"))?;
    fs::create_dir_all(bundle.join("public"))?;
    fs::create_dir_all(bundle.join("private-benchmark"))?;
    fs::write(
        bundle.join("statement.md"),
        "# Rehearsal Piped Stdio\n\nAdd numbers through a trusted interactor.\n",
    )?;
    fs::write(
        bundle.join("public/session.json"),
        json!({"session_name": "public-1", "metadata": {"a": 2, "b": 3}}).to_string(),
    )?;
    fs::write(
        bundle.join("private-benchmark/session.json"),
        json!({"session_name": "official-1", "metadata": {"a": 11, "b": 31}}).to_string(),
    )?;
    fs::write(
        bundle.join("interactive-evaluator/run.py"),
        piped_stdio_evaluator_py(),
    )?;
    fs::write(
        bundle.join("spec.json"),
        serde_json::to_string_pretty(&base_spec_json(
            challenge,
            image,
            json!({
                "mode": "piped_stdio",
                "interactive_evaluator": {
                    "command": ["python", "interactive-evaluator/run.py"],
                    "result_file": "result.json"
                },
                "acknowledge_stdio_protocol_framing": true,
                "validation_session": "public/session.json",
                "official_session": "private-benchmark/session.json"
            }),
            true,
            true,
        ))?,
    )?;
    Ok(())
}

fn write_coexecuted_bundle(
    root: &Path,
    challenge: &RehearsalChallengeEvidence,
    image: &RehearsalImageConfig,
) -> Result<(), ProductionRehearsalError> {
    let bundle = root.join(&challenge.name).join("v1");
    fs::create_dir_all(bundle.join("coexecuted-evaluator"))?;
    fs::create_dir_all(bundle.join("public"))?;
    fs::create_dir_all(bundle.join("private-benchmark"))?;
    fs::write(
        bundle.join("statement.md"),
        "# Rehearsal Coexecuted Benchmark\n\nImport participant code from `/workspace`.\n",
    )?;
    fs::write(bundle.join("public/case.json"), r#"{"a":2,"b":3}"#)?;
    fs::write(
        bundle.join("private-benchmark/secret.json"),
        r#"{"a":11,"b":31}"#,
    )?;
    fs::write(
        bundle.join("coexecuted-evaluator/run.py"),
        coexecuted_evaluator_py(),
    )?;
    fs::write(
        bundle.join("spec.json"),
        serde_json::to_string_pretty(&base_spec_json(
            challenge,
            image,
            json!({
                "mode": "coexecuted_benchmark",
                "coexecuted_evaluator": {
                    "command": ["python", "coexecuted-evaluator/run.py"],
                    "result_file": "result.json"
                },
                "acknowledge_danger": true
            }),
            true,
            false,
        ))?,
    )?;
    Ok(())
}

fn base_spec_json(
    challenge: &RehearsalChallengeEvidence,
    image: &RehearsalImageConfig,
    execution: serde_json::Value,
    private_benchmark_enabled: bool,
    include_solution_run: bool,
) -> serde_json::Value {
    let solution_profile = if include_solution_run {
        json!({
            "setup": stage_profile(),
            "build": stage_profile(),
            "run": stage_profile()
        })
    } else {
        json!({
            "setup": stage_profile(),
            "build": stage_profile()
        })
    };
    json!({
        "schema_version": 1,
        "challenge_name": challenge.name.as_str(),
        "challenge_title": challenge.title.as_str(),
        "summary": {
            "en": "Production rehearsal fixture for Agentics.",
            "zh": "Agentics 生产演练夹具。"
        },
        "keywords": ["rehearsal"],
        "solution": {
            "protocol": "zip_project",
            "manifest_file": "agentics.solution.json"
        },
        "targets": [{
            "name": TARGET_CPU,
            "docker_platform": "linux/arm64",
            "accelerator": null,
            "validation_enabled": true,
            "resource_profile": {
                "name": "agentics-rehearsal-cpu",
                "solution_image": {"source": image.source.as_str(), "reference": image.reference.as_str()},
                "evaluator_image": {"source": image.source.as_str(), "reference": image.reference.as_str()},
                "solution": solution_profile,
                "evaluator": {
                    "setup": stage_profile(),
                    "run": stage_profile()
                },
                "resource_description": "Small CPU profile for production rehearsal fixtures.",
                "hardware_metadata": {"kind": "cpu"}
            }
        }],
        "starts_at": "2026-01-01T00:00:00Z",
        "eligibility": { "type": "open" },
        "visibility": {
            "leaderboard": "public_live",
            "score_distribution": "public_live",
            "result_detail": "submitter_live_public_live"
        },
        "solution_publication": "public",
        "execution": execution,
        "datasets": {
            "public_dir": "public",
            "private_benchmark_dir": "private-benchmark",
            "public_policy": "full",
            "private_benchmark_policy": "score_only",
            "private_benchmark_enabled": private_benchmark_enabled
        },
        "metric_schema": {
            "metrics": [
                {"name": "score", "label": "Score", "direction": "maximize", "visibility": "public"},
                {"name": "passed_cases", "label": "Passed Cases", "unit": "cases", "direction": "maximize", "visibility": "public"}
            ],
            "ranking": {
                "primary_metric_name": "score",
                "tie_breaker_metric_names": ["passed_cases"]
            }
        }
    })
}

fn stage_profile() -> serde_json::Value {
    json!({
        "timeout_sec": 30,
        "memory_limit_mb": 512,
        "cpu_limit_millis": 1000,
        "disk_limit_mb": 256,
        "network_access": "disabled"
    })
}

fn separated_evaluator_py() -> &'static str {
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
logs = []
for run in runs:
    stdout_path = Path(args.solution_runs_dir) / run["run_name"] / "stdout.txt"
    if not stdout_path.is_file():
        results.append({"case_name": run["run_name"], "status": "error", "score": 0, "message": "missing stdout.txt"})
        continue
    actual = stdout_path.read_text().strip()
    expected = str(run["expected"])
    if actual == expected:
        results.append({"case_name": run["run_name"], "status": "passed", "score": 1})
    else:
        logs.append(f"{run['run_name']}: expected {expected}, got {actual}")
        results.append({"case_name": run["run_name"], "status": "failed", "score": 0, "message": "wrong answer"})

total = len(results)
passed = sum(1 for result in results if result["status"] == "passed")
score = 0 if total == 0 else passed / total
summary_key = "validation_summary" if args.mode == "validation" else "official_summary"
payload = {
    "status": "passed" if passed == total else "failed",
    "rank_score": score,
    "aggregate_metrics": [{"metric_name": "score", "value": score}, {"metric_name": "passed_cases", "value": passed}],
    "run_metrics": [{"run_name": result["case_name"], "metrics": [{"metric_name": "score", "value": result["score"]}]} for result in results],
    "logs": logs,
    summary_key: {"score": score, "passed": passed, "total": total},
}
if args.mode == "validation":
    payload["public_results"] = results
else:
    payload["public_results"] = []
Path(args.output_path).write_text(json.dumps(payload))
"#
}

fn piped_stdio_evaluator_py() -> &'static str {
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
    "rank_score": score,
    "aggregate_metrics": [{"metric_name": "score", "value": score}, {"metric_name": "passed_cases", "value": score}],
    summary_key: {"score": score, "passed": 1 if passed else 0, "total": 1},
}
if args.mode == "validation":
    payload["public_results"] = [{"case_name": session["session_name"], "status": "passed" if passed else "failed", "score": score}]
Path(args.output_path).write_text(json.dumps(payload))
"#
}

fn coexecuted_evaluator_py() -> &'static str {
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
    "rank_score": score,
    "aggregate_metrics": [{"metric_name": "score", "value": score}, {"metric_name": "passed_cases", "value": score}],
    summary_key: {"score": score, "passed": 1 if passed else 0, "total": 1},
}
if args.mode == "validation":
    payload["public_results"] = [{"case_name": "public-1", "status": "passed" if passed else "failed", "score": score}]
Path(args.output_path).write_text(json.dumps(payload))
"#
}

fn sample_sum_solution(expression: &str) -> String {
    [
        "from __future__ import annotations",
        "",
        "import json",
        "import sys",
        "",
        "payload = json.loads(sys.stdin.read())",
        &format!("print({expression})"),
        "",
    ]
    .join("\n")
}

fn zip_base64(entries: &[(&str, String)]) -> Result<String, ProductionRehearsalError> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for (name, content) in entries {
        archive.start_file(name, options)?;
        archive.write_all(content.as_bytes())?;
    }

    let cursor = archive.finish()?;
    Ok(STANDARD.encode(cursor.into_inner()))
}
