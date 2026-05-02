from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run sample sum scorer")
    parser.add_argument("--challenge-dir", required=True)
    parser.add_argument("--solution-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["validation", "official"], required=True)
    return parser.parse_args()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def score_cases(
    *,
    solution_entrypoint: Path,
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
                [sys.executable, str(solution_entrypoint), payload],
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
            stderr = completed.stderr.strip() or "solution exited with non-zero status"
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


def aggregate_metrics(summary: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        {"metric_id": "score", "value": summary["score"]},
        {"metric_id": "passed_cases", "value": summary["passed"]},
    ]


def run_metrics(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {
            "run_id": result["case_id"],
            "metrics": [{"metric_id": "score", "value": result["score"]}],
        }
        for result in results
    ]


def main() -> int:
    args = parse_args()
    challenge_dir = Path(args.challenge_dir)
    solution_dir = Path(args.solution_dir)
    output_path = Path(args.output_path)
    spec = load_json(challenge_dir / "spec.json")

    solution_entrypoint = solution_dir / spec["solution"]["entrypoint"]
    if not solution_entrypoint.is_file():
        raise FileNotFoundError(f"solution entrypoint not found: {solution_entrypoint}")

    logs: list[str] = []
    time_limit_sec = float(spec["limits"]["time_limit_sec"])

    if args.mode == "validation":
        public_results = score_cases(
            solution_entrypoint=solution_entrypoint,
            cases_path=challenge_dir / spec["datasets"]["public_dir"] / "cases.json",
            time_limit_sec=time_limit_sec,
            logs=logs,
        )
        validation_summary = summarize(public_results)
        payload = {
            "status": "passed" if validation_summary["passed"] == validation_summary["total"] else "failed",
            "mode": "validation",
            "primary_score": validation_summary["score"],
            "rank_score": validation_summary["score"],
            "aggregate_metrics": aggregate_metrics(validation_summary),
            "run_metrics": run_metrics(public_results),
            "public_results": public_results,
            "validation_summary": validation_summary,
            "official_summary": None,
            "logs": logs,
        }
    else:
        private_benchmark_dir = spec["datasets"].get("private_benchmark_dir")
        if not spec["datasets"].get("private_benchmark_enabled") or not private_benchmark_dir:
            raise ValueError("official mode requires private benchmark dataset")

        official_results = score_cases(
            solution_entrypoint=solution_entrypoint,
            cases_path=challenge_dir / private_benchmark_dir / "cases.json",
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
            "rank_score": official_summary["score"],
            "aggregate_metrics": aggregate_metrics(official_summary),
            "run_metrics": run_metrics(official_results),
            "public_results": [],
            "validation_summary": None,
            "official_summary": official_summary,
            "logs": logs,
        }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
