from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
PROBLEM_DIR = ROOT / "examples" / "problems" / "grid-routing" / "v1"
SCORER_PATH = PROBLEM_DIR / "scorer" / "run.py"


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
            "--problem-dir",
            str(PROBLEM_DIR),
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


def test_public_mode_returns_partial_scores(tmp_path: Path) -> None:
    result = run_scorer(
        tmp_path,
        mode="public",
        paths_by_instance={
            "shown-1": "RRRRDDDD",
            "shown-2": "DDDDRRUUUURRDDDD",
            "shown-3": "RRDDRDRDDR",
            "hidden-1": "DDDDDRRRRR",
            "hidden-2": "RRDRDDRDDR",
        },
    )

    assert result["status"] == "passed"
    assert result["mode"] == "public"
    assert result["primary_score"] == 1
    assert len(result["shown_results"]) == 3
    assert all(item["status"] == "passed" for item in result["shown_results"])
    assert all(item["score"] == 1 for item in result["shown_results"])
    assert result["hidden_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["official_summary"] is None


def test_public_mode_rewards_valid_but_indirect_route(tmp_path: Path) -> None:
    result = run_scorer(
        tmp_path,
        mode="public",
        paths_by_instance={
            "shown-1": "RRLLRRRRDDDD",
            "shown-2": "DDDDRRUUUURRDDDD",
            "shown-3": "RRDDRDRDDR",
            "hidden-1": "DDDDDRRRRR",
            "hidden-2": "RRDRDDRDDR",
        },
    )

    shown_case = result["shown_results"][0]

    assert result["status"] == "passed"
    assert shown_case["status"] == "passed"
    assert 0 < shown_case["score"] < 1
    assert "turns=" in shown_case["message"]


def test_failed_path_is_reported(tmp_path: Path) -> None:
    result = run_scorer(
        tmp_path,
        mode="public",
        paths_by_instance={
            "shown-1": "RDDDD",
            "shown-2": "DDDDRRUUUURRDDDD",
            "shown-3": "RRDDRDRDDR",
            "hidden-1": "DDDDDRRRRR",
            "hidden-2": "RRDRDDRDDR",
        },
    )

    assert result["status"] == "passed"
    assert result["shown_results"][0]["status"] == "failed"
    assert result["shown_results"][0]["score"] == 0
    assert "hit obstacle" in result["shown_results"][0]["message"]
    assert result["logs"]


def test_official_mode_uses_heldout_cases(tmp_path: Path) -> None:
    result = run_scorer(
        tmp_path,
        mode="official",
        paths_by_instance={
            "heldout-1": "RRDDRRDDDR",
            "heldout-2": "DDDDRRRRDDRR",
        },
    )

    assert result["status"] == "passed"
    assert result["mode"] == "official"
    assert result["shown_results"] == []
    assert result["hidden_summary"] is None
    assert result["official_summary"] == {"score": 1, "passed": 2, "total": 2}
