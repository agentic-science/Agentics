from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
CHALLENGE_DIR = ROOT / "examples" / "challenges" / "grid-routing" / "v1"
SCORER_PATH = CHALLENGE_DIR / "scorer" / "run.py"


def write_submission(target_dir: Path, paths_by_instance: dict[str, str]) -> Path:
    submission_dir = target_dir / "submission"
    submission_dir.mkdir(parents=True, exist_ok=True)
    (submission_dir / "main.py").write_text(
        "\n".join(
            [
                "from __future__ import annotations",
                "",
                "import json",
                "import sys",
                "",
                "",
                "PATHS = {",
                *[
                    f"    {instance_id!r}: {path!r},"
                    for instance_id, path in sorted(paths_by_instance.items())
                ],
                "}",
                "",
                "",
                "def main() -> None:",
                "    payload = json.loads(sys.argv[1])",
                "    print(PATHS[payload['instance_id']])",
                "",
                "",
                "if __name__ == '__main__':",
                "    main()",
                "",
            ]
        ),
        encoding="utf-8",
    )
    return submission_dir


def run_scorer(tmp_path: Path, *, mode: str, paths_by_instance: dict[str, str]) -> dict:
    submission_dir = write_submission(tmp_path, paths_by_instance)
    output_path = tmp_path / "result.json"

    subprocess.run(
        [
            sys.executable,
            str(SCORER_PATH),
            "--challenge-dir",
            str(CHALLENGE_DIR),
            "--submission-dir",
            str(submission_dir),
            "--output-path",
            str(output_path),
            "--mode",
            mode,
        ],
        check=True,
        cwd=ROOT,
    )

    return json.loads(output_path.read_text(encoding="utf-8"))


def test_validation_mode_returns_public_scores(tmp_path: Path) -> None:
    result = run_scorer(
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
    assert result["primary_score"] == 1
    assert result["rank_score"] == 1
    assert result["aggregate_metrics"] == [
        {"metric_id": "score", "value": 1},
        {"metric_id": "passed_cases", "value": 3},
    ]
    assert result["run_metrics"] == [
        {"run_id": "public-1", "metrics": [{"metric_id": "score", "value": 1}]},
        {"run_id": "public-2", "metrics": [{"metric_id": "score", "value": 1}]},
        {"run_id": "public-3", "metrics": [{"metric_id": "score", "value": 1}]},
    ]
    assert len(result["public_results"]) == 3
    assert all(item["status"] == "passed" for item in result["public_results"])
    assert all(item["score"] == 1 for item in result["public_results"])
    assert result["validation_summary"] == {"score": 1, "passed": 3, "total": 3}
    assert result["official_summary"] is None


def test_validation_mode_rewards_valid_but_indirect_route(tmp_path: Path) -> None:
    result = run_scorer(
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
    result = run_scorer(
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
    result = run_scorer(
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
    assert result["validation_summary"] is None
    assert result["official_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["aggregate_metrics"] == [
        {"metric_id": "score", "value": 1},
        {"metric_id": "passed_cases", "value": 2},
    ]
