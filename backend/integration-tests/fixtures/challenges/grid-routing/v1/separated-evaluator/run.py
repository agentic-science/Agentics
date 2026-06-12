from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


DIRECTIONS = {
    "U": (-1, 0),
    "D": (1, 0),
    "L": (0, -1),
    "R": (0, 1),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Score worker-managed grid-routing runs")
    parser.add_argument("--challenge-dir", required=True)
    parser.add_argument("--solution-runs-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["validation", "official"], required=True)
    parser.add_argument("--runs-file", required=True)
    return parser.parse_args()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def find_marker(grid: list[str], marker: str) -> tuple[int, int]:
    for row_index, row in enumerate(grid):
        for col_index, cell in enumerate(row):
            if cell == marker:
                return row_index, col_index
    raise ValueError(f"marker {marker!r} not found in grid")


def analyze_path(grid: list[str], path: str) -> dict[str, Any]:
    if any(move not in DIRECTIONS for move in path):
        return {"status": "error", "message": "path must only contain U, D, L, R"}

    height = len(grid)
    width = len(grid[0]) if grid else 0
    row, col = find_marker(grid, "S")
    goal = find_marker(grid, "G")
    turns = 0
    current_run = 0
    longest_run = 0
    previous_move = ""

    for step_index, move in enumerate(path, start=1):
        if previous_move and previous_move != move:
            turns += 1
            current_run = 1
        elif previous_move == move:
            current_run += 1
        else:
            current_run = 1

        longest_run = max(longest_run, current_run)
        delta_row, delta_col = DIRECTIONS[move]
        row += delta_row
        col += delta_col
        previous_move = move

        if row < 0 or row >= height or col < 0 or col >= width:
            return {"status": "failed", "message": f"left grid at step {step_index}"}

        if grid[row][col] == "#":
            return {"status": "failed", "message": f"hit obstacle at step {step_index}"}

    if (row, col) != goal:
        return {"status": "failed", "message": "did not reach goal"}

    return {
        "status": "passed",
        "steps": len(path),
        "turns": turns,
        "longest_run": longest_run,
    }


def score_run(run: dict[str, Any], solution_runs_dir: Path, logs: list[str]) -> dict[str, Any]:
    run_name = run["run_name"]
    candidate_path_file = solution_runs_dir / run_name / "output" / "path.txt"
    case_file = solution_runs_dir / run_name / "input" / "case.json"
    if not candidate_path_file.is_file():
        return {
            "case_name": run_name,
            "status": "error",
            "score": 0,
            "message": "missing output/path.txt",
        }

    grid = load_json(case_file)["grid"]
    candidate_path = candidate_path_file.read_text(encoding="utf-8").strip()
    benchmark_metrics = analyze_path(grid, run["metadata"]["benchmark_path"])
    if benchmark_metrics["status"] != "passed":
        raise ValueError(f"invalid benchmark for {run_name}: {benchmark_metrics['message']}")

    candidate_metrics = analyze_path(grid, candidate_path)
    if candidate_metrics["status"] != "passed":
        logs.append(f"{run_name}: {candidate_metrics['message']}")
        return {
            "case_name": run_name,
            "status": candidate_metrics["status"],
            "score": 0,
            "message": candidate_metrics["message"],
        }

    efficiency = benchmark_metrics["steps"] / max(candidate_metrics["steps"], 1)
    turn_bonus = min((benchmark_metrics["turns"] + 1) / (candidate_metrics["turns"] + 1), 1)
    straight_bonus = min(
        candidate_metrics["longest_run"] / max(benchmark_metrics["longest_run"], 1),
        1,
    )
    score = round(0.60 + 0.20 * efficiency + 0.12 * turn_bonus + 0.08 * straight_bonus, 4)
    message = (
        f"steps={candidate_metrics['steps']}, turns={candidate_metrics['turns']}, "
        f"longest_run={candidate_metrics['longest_run']}, score={score}"
    )
    logs.append(f"{run_name}: {message}")
    return {"case_name": run_name, "status": "passed", "score": score, "message": message}


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    total = len(results)
    passed = sum(1 for result in results if result["status"] == "passed")
    score = 0 if total == 0 else round(sum(result["score"] for result in results) / total, 4)
    return {"score": score, "passed": passed, "total": total}


def aggregate_metrics(summary: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        {"metric_name": "score", "value": summary["score"]},
        {"metric_name": "passed_cases", "value": summary["passed"]},
    ]


def run_metrics(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {
            "run_name": result["case_name"],
            "metrics": [{"metric_name": "score", "value": result["score"]}],
        }
        for result in results
    ]


def main() -> int:
    args = parse_args()
    runs = load_json(Path(args.runs_file))["runs"]
    logs: list[str] = []
    results = [score_run(run, Path(args.solution_runs_dir), logs) for run in runs]
    summary = summarize(results)
    payload = {
        "status": "passed" if summary["passed"] == summary["total"] else "failed",
        "mode": args.mode,
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
