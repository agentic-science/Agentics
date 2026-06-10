from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
CHALLENGE_DIR = ROOT / "examples" / "challenges" / "grid-routing" / "v1"
EVALUATOR_PATH = CHALLENGE_DIR / "evaluator" / "run.py"


def runs_file_for_mode(mode: str) -> Path:
    return CHALLENGE_DIR / (
        "public/runs.json" if mode == "validation" else "private-benchmark/runs.json"
    )


def run_evaluator(tmp_path: Path, *, mode: str, paths_by_instance: dict[str, str]) -> dict:
    runs_file = runs_file_for_mode(mode)
    solution_runs_dir = tmp_path / "solution-runs"
    runs = json.loads(runs_file.read_text(encoding="utf-8"))["runs"]
    for run in runs:
        run_dir = solution_runs_dir / run["run_name"]
        input_dir = run_dir / "input"
        output_dir = run_dir / "output"
        input_dir.mkdir(parents=True, exist_ok=True)
        output_dir.mkdir(parents=True, exist_ok=True)
        case = run["input_files"][0]["content_json"]
        (input_dir / "case.json").write_text(json.dumps(case), encoding="utf-8")
        path = paths_by_instance[run["run_name"]]
        (output_dir / "path.txt").write_text(f"{path}\n", encoding="utf-8")

    output_path = tmp_path / "result.json"

    subprocess.run(
        [
            sys.executable,
            str(EVALUATOR_PATH),
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


def test_validation_mode_returns_public_scores(tmp_path: Path) -> None:
    result = run_evaluator(
        tmp_path,
        mode="validation",
        paths_by_instance={
            "public-1": "RRRRDDDD",
            "public-2": "DDDDRRUUUURRDDDD",
            "public-3": "RRDDRDRDDR",
        },
    )

    assert result["status"] == "passed"
    assert result["mode"] == "validation"
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
    assert all(item["status"] == "passed" for item in result["public_results"])
    assert all(item["score"] == 1 for item in result["public_results"])
    assert result["validation_summary"] == {"score": 1, "passed": 3, "total": 3}
    assert result.get("official_summary") is None


def test_validation_mode_rewards_valid_but_indirect_route(tmp_path: Path) -> None:
    result = run_evaluator(
        tmp_path,
        mode="validation",
        paths_by_instance={
            "public-1": "RRLLRRRRDDDD",
            "public-2": "DDDDRRUUUURRDDDD",
            "public-3": "RRDDRDRDDR",
        },
    )

    public_case = result["public_results"][0]

    assert result["status"] == "passed"
    assert public_case["status"] == "passed"
    assert 0 < public_case["score"] < 1
    assert "turns=" in public_case["message"]


def test_failed_path_is_reported(tmp_path: Path) -> None:
    result = run_evaluator(
        tmp_path,
        mode="validation",
        paths_by_instance={
            "public-1": "RDDDD",
            "public-2": "DDDDRRUUUURRDDDD",
            "public-3": "RRDDRDRDDR",
        },
    )

    assert result["status"] == "failed"
    assert result["public_results"][0]["status"] == "failed"
    assert result["public_results"][0]["score"] == 0
    assert "hit obstacle" in result["public_results"][0]["message"]
    assert result["logs"]


def test_official_mode_uses_private_benchmark_cases(tmp_path: Path) -> None:
    result = run_evaluator(
        tmp_path,
        mode="official",
        paths_by_instance={
            "private-benchmark-1": "RRDDRRDDDR",
            "private-benchmark-2": "DDDDRRRRDDRR",
        },
    )

    assert result["status"] == "passed"
    assert result["mode"] == "official"
    assert result["public_results"] == []
    assert result.get("validation_summary") is None
    assert result["official_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["aggregate_metrics"] == [
        {"metric_name": "score", "value": 1},
        {"metric_name": "passed_cases", "value": 2},
    ]
