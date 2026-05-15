from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
CHALLENGE_DIR = ROOT / "examples" / "challenges" / "sample-sum" / "v1"
SCORER_PATH = CHALLENGE_DIR / "scorer" / "run.py"


def runs_file_for_mode(mode: str) -> Path:
    return CHALLENGE_DIR / (
        "public/runs.json" if mode == "validation" else "private-benchmark/runs.json"
    )


def write_solution_runs(target_dir: Path, runs_file: Path, *, correct: bool) -> Path:
    solution_runs_dir = target_dir / "solution-runs"
    runs = json.loads(runs_file.read_text(encoding="utf-8"))["runs"]
    for run in runs:
        run_dir = solution_runs_dir / run["run_name"]
        run_dir.mkdir(parents=True, exist_ok=True)
        answer = run["expected"] if correct else "wrong"
        (run_dir / "stdout.txt").write_text(f"{answer}\n", encoding="utf-8")
    return solution_runs_dir


def run_scorer(tmp_path: Path, *, mode: str, correct: bool) -> dict:
    runs_file = runs_file_for_mode(mode)
    solution_runs_dir = write_solution_runs(tmp_path, runs_file, correct=correct)
    output_path = tmp_path / "result.json"

    subprocess.run(
        [
            sys.executable,
            str(SCORER_PATH),
            "--challenge-dir",
            str(CHALLENGE_DIR),
            "--solution-runs-dir",
            str(solution_runs_dir),
            "--output-path",
            str(output_path),
            "--mode",
            mode,
            "--runs-file",
            str(runs_file),
        ],
        check=True,
        cwd=ROOT,
    )

    return json.loads(output_path.read_text(encoding="utf-8"))


def test_validation_mode_returns_public_summary(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="validation", correct=True)

    assert result["status"] == "passed"
    assert result["mode"] == "validation"
    assert result["primary_score"] == 1
    assert result["rank_score"] == 1
    assert result["aggregate_metrics"] == [
        {"metric_name": "score", "value": 1},
        {"metric_name": "passed_cases", "value": 3},
    ]
    assert result["run_metrics"] == [
        {"run_name": "public-1", "metrics": [{"metric_name": "score", "value": 1}]},
        {"run_name": "public-2", "metrics": [{"metric_name": "score", "value": 1}]},
        {"run_name": "public-3", "metrics": [{"metric_name": "score", "value": 1}]},
    ]
    assert len(result["public_results"]) == 3
    assert result["validation_summary"] == {"score": 1, "passed": 3, "total": 3}
    assert result.get("official_summary") is None


def test_official_mode_uses_private_benchmark_cases(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="official", correct=True)

    assert result["status"] == "passed"
    assert result["mode"] == "official"
    assert result["public_results"] == []
    assert result.get("validation_summary") is None
    assert result["official_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["aggregate_metrics"] == [
        {"metric_name": "score", "value": 1},
        {"metric_name": "passed_cases", "value": 2},
    ]


def test_failed_submission_is_reported(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="validation", correct=False)

    assert result["status"] == "failed"
    assert result["primary_score"] == 0
    assert result["validation_summary"] == {"score": 0, "passed": 0, "total": 3}
    assert result["logs"]
