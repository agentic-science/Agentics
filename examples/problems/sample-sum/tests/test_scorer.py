from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
PROBLEM_DIR = ROOT / "examples" / "problems" / "sample-sum" / "v1"
SCORER_PATH = PROBLEM_DIR / "scorer" / "run.py"


def write_submission(target_dir: Path, expression: str) -> Path:
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
    return submission_dir


def run_scorer(tmp_path: Path, *, mode: str, expression: str) -> dict:
    submission_dir = write_submission(tmp_path, expression)
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


def test_public_mode_returns_shown_and_hidden_summary(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="public", expression="payload['a'] + payload['b']")

    assert result["status"] == "passed"
    assert result["mode"] == "public"
    assert result["primary_score"] == 1
    assert len(result["shown_results"]) == 2
    assert result["hidden_summary"] == {"score": 1, "passed": 2, "total": 2}
    assert result["official_summary"] is None


def test_official_mode_uses_heldout_cases(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="official", expression="payload['a'] + payload['b']")

    assert result["status"] == "passed"
    assert result["mode"] == "official"
    assert result["shown_results"] == []
    assert result["hidden_summary"] is None
    assert result["official_summary"] == {"score": 1, "passed": 2, "total": 2}


def test_failed_submission_is_reported(tmp_path: Path) -> None:
    result = run_scorer(tmp_path, mode="public", expression="payload['a'] - payload['b']")

    assert result["status"] == "failed"
    assert result["primary_score"] == 0
    assert result["hidden_summary"] == {"score": 0, "passed": 0, "total": 2}
    assert result["logs"]
