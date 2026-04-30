from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run sample sum scorer")
    parser.add_argument("--problem-dir", required=True)
    parser.add_argument("--submission-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["public", "official"], required=True)
    return parser.parse_args()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def score_cases(
    *,
    submission_entrypoint: Path,
    cases_path: Path,
    time_limit_sec: float,
    logs: list[str],
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    cases = load_json(cases_path)

    for case in cases:
        case_id = case["case_id"]
        payload = json.dumps(case["input"], separators=(",", ":"))

        try:
            completed = subprocess.run(
                [sys.executable, str(submission_entrypoint), payload],
                capture_output=True,
                text=True,
                timeout=time_limit_sec,
                check=False,
            )
        except subprocess.TimeoutExpired:
            logs.append(f"{case_id}: timeout")
            results.append(
                {
                    "case_id": case_id,
                    "status": "error",
                    "score": 0,
                    "message": "timeout",
                }
            )
            continue

        stdout = completed.stdout.strip()
        expected = str(case["expected"])

        if completed.returncode != 0:
            stderr = completed.stderr.strip() or "submission exited with non-zero status"
            logs.append(f"{case_id}: runtime error: {stderr}")
            results.append(
                {
                    "case_id": case_id,
                    "status": "error",
                    "score": 0,
                    "message": stderr,
                }
            )
            continue

        if stdout == expected:
            results.append(
                {
                    "case_id": case_id,
                    "status": "passed",
                    "score": 1,
                }
            )
            continue

        logs.append(f"{case_id}: expected {expected}, got {stdout}")
        results.append(
            {
                "case_id": case_id,
                "status": "failed",
                "score": 0,
                "message": f"expected {expected}, got {stdout}",
            }
        )

    return results


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    total = len(results)
    passed = sum(1 for result in results if result["status"] == "passed")
    score = 0 if total == 0 else passed / total
    return {"score": score, "passed": passed, "total": total}


def main() -> int:
    args = parse_args()
    problem_dir = Path(args.problem_dir)
    submission_dir = Path(args.submission_dir)
    output_path = Path(args.output_path)
    spec = load_json(problem_dir / "spec.json")

    submission_entrypoint = submission_dir / spec["submission"]["entrypoint"]
    if not submission_entrypoint.is_file():
        raise FileNotFoundError(f"submission entrypoint not found: {submission_entrypoint}")

    logs: list[str] = []
    time_limit_sec = float(spec["limits"]["time_limit_sec"])

    if args.mode == "public":
        shown_results = score_cases(
            submission_entrypoint=submission_entrypoint,
            cases_path=problem_dir / spec["datasets"]["shown_dir"] / "cases.json",
            time_limit_sec=time_limit_sec,
            logs=logs,
        )
        hidden_results = score_cases(
            submission_entrypoint=submission_entrypoint,
            cases_path=problem_dir / spec["datasets"]["hidden_dir"] / "cases.json",
            time_limit_sec=time_limit_sec,
            logs=logs,
        )
        hidden_summary = summarize(hidden_results)
        payload = {
            "status": "passed" if hidden_summary["passed"] == hidden_summary["total"] else "failed",
            "mode": "public",
            "primary_score": hidden_summary["score"],
            "shown_results": shown_results,
            "hidden_summary": hidden_summary,
            "official_summary": None,
            "logs": logs,
        }
    else:
        heldout_dir = spec["datasets"].get("heldout_dir")
        if not spec["datasets"].get("heldout_enabled") or not heldout_dir:
            raise ValueError("official mode requires heldout dataset")

        official_results = score_cases(
            submission_entrypoint=submission_entrypoint,
            cases_path=problem_dir / heldout_dir / "cases.json",
            time_limit_sec=time_limit_sec,
            logs=logs,
        )
        official_summary = summarize(official_results)
        payload = {
            "status": "passed"
            if official_summary["passed"] == official_summary["total"]
            else "failed",
            "mode": "official",
            "primary_score": official_summary["score"],
            "shown_results": [],
            "hidden_summary": None,
            "official_summary": official_summary,
            "logs": logs,
        }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
