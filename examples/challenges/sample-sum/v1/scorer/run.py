from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Score worker-managed sample-sum runs")
    parser.add_argument("--challenge-dir", required=True)
    parser.add_argument("--solution-runs-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["validation", "official"], required=True)
    parser.add_argument("--runs-file", required=True)
    return parser.parse_args()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


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


def score_runs(runs_file: Path, solution_runs_dir: Path, logs: list[str]) -> list[dict[str, Any]]:
    runs = load_json(runs_file)["runs"]
    results: list[dict[str, Any]] = []

    for run in runs:
        run_id = run["run_id"]
        stdout_path = solution_runs_dir / run_id / "stdout.txt"
        if not stdout_path.is_file():
            results.append(
                {
                    "case_id": run_id,
                    "status": "error",
                    "score": 0,
                    "message": "missing stdout.txt",
                }
            )
            continue

        stdout = stdout_path.read_text(encoding="utf-8").strip()
        expected = str(run["expected"])
        if stdout == expected:
            results.append({"case_id": run_id, "status": "passed", "score": 1})
        else:
            message = f"expected {expected}, got {stdout}"
            logs.append(f"{run_id}: {message}")
            results.append(
                {
                    "case_id": run_id,
                    "status": "failed",
                    "score": 0,
                    "message": message,
                }
            )

    return results


def main() -> int:
    args = parse_args()
    logs: list[str] = []
    results = score_runs(Path(args.runs_file), Path(args.solution_runs_dir), logs)
    summary = summarize(results)
    payload = {
        "status": "passed" if summary["passed"] == summary["total"] else "failed",
        "mode": args.mode,
        "primary_score": summary["score"],
        "rank_score": summary["score"],
        "aggregate_metrics": aggregate_metrics(summary),
        "run_metrics": run_metrics(results),
        "public_results": results if args.mode == "validation" else [],
        "logs": logs,
    }
    if args.mode == "validation":
        payload["validation_summary"] = summary
    else:
        payload["official_summary"] = summary

    output_path = Path(args.output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
