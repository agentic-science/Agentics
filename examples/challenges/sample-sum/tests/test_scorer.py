from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
CHALLENGE_DIR = ROOT / "examples" / "challenges" / "sample-sum" / "v1"
SCORER_PATH = CHALLENGE_DIR / "scorer" / "run.py"


def write_solution(target_dir: Path, expression: str) -> Path:
    solution_dir = target_dir / "solution"
    solution_dir.mkdir(parents=True, exist_ok=True)
    (solution_dir / "main.py").write_text(
        "\n".join(
            [
                "from __future__ import annotations",
                "",
                "import json",
                "import sys",
                "",
                "",
                "def main() -> None:",
                "    payload = json.loads(sys.argv[1])",
                f"    print({expression})",
                "",
                "",
                "if __name__ == '__main__':",
                "    main()",
                "",
            ]
        ),
        encoding="utf-8",
    )
    return solution_dir


def run_scorer(tmp_path: Path, *, mode: str, expression: str) -> dict:
    solution_dir = write_solution(tmp_path, expression)
    output_path = tmp_path / "result.json"

    subprocess.run(
        [
            sys.executable,
            str(SCORER_PATH),
            "--challenge-dir",
            str(CHALLENGE_DIR),
            "--solution-dir",
            str(solution_dir),
            "--output-path",
            str(output_path),
            "--mode",
            mode,
        ],
        check=True,
        cwd=ROOT,
    )

    return json.loads(output_path.read_text(encoding="utf-8"))


def test_validation_mode_returns_public_summary(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="validation", expression="payload['a'] + payload['b']")

    assert result["status"] == "passed"
    assert result["mode"] == "validation"
    assert result["primary_score"] == 1
    assert result["rank_score"] == 1
    assert result["aggregate_metrics"] == [
        {"metric_id": "score", "value": 1},
        {"metric_id": "passed_cases", "value": 2},
    ]
    assert result["run_metrics"] == [
        {"run_id": "public-1", "metrics": [{"metric_id": "score", "value": 1}]},
        {"run_id": "public-2", "metrics": [{"metric_id": "score", "value": 1}]},
    ]
    assert len(result["public_results"]) == 2
    assert result["validation_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["official_summary"] is None


def test_official_mode_uses_private_benchmark_cases(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="official", expression="payload['a'] + payload['b']")

    assert result["status"] == "passed"
    assert result["mode"] == "official"
    assert result["public_results"] == []
    assert result["validation_summary"] is None
    assert result["official_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["aggregate_metrics"] == [
        {"metric_id": "score", "value": 1},
        {"metric_id": "passed_cases", "value": 2},
    ]


def test_failed_submission_is_reported(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="validation", expression="payload['a'] - payload['b']")

    assert result["status"] == "failed"
    assert result["primary_score"] == 0
    assert result["validation_summary"] == {"score": 0, "passed": 0, "total": 2}
    assert result["logs"]
