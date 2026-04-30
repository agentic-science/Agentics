from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


DIRECTIONS = {
    "U": (-1, 0),
    "D": (1, 0),
    "L": (0, -1),
    "R": (0, 1),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run grid routing scorer")
    parser.add_argument("--problem-dir", required=True)
    parser.add_argument("--submission-dir", required=True)
    parser.add_argument("--output-path", required=True)
    parser.add_argument("--mode", choices=["public", "official"], required=True)
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
        return {
            "status": "error",
            "message": "path must only contain U, D, L, R",
        }

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
            return {
                "status": "failed",
                "message": f"left grid at step {step_index}",
            }

        if grid[row][col] == "#":
            return {
                "status": "failed",
                "message": f"hit obstacle at step {step_index}",
            }

    if (row, col) != goal:
        return {
            "status": "failed",
            "message": "did not reach goal",
        }

    return {
        "status": "passed",
        "steps": len(path),
        "turns": turns,
        "longest_run": longest_run,
    }


def score_case(
    *,
    case: dict[str, Any],
    submission_entrypoint: Path,
    time_limit_sec: float,
    logs: list[str],
) -> dict[str, Any]:
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
        return {
            "case_id": case_id,
            "status": "error",
            "score": 0,
            "message": "timeout",
        }

    if completed.returncode != 0:
        stderr = completed.stderr.strip() or "submission exited with non-zero status"
        logs.append(f"{case_id}: runtime error: {stderr}")
        return {
            "case_id": case_id,
            "status": "error",
            "score": 0,
            "message": stderr,
        }

    candidate_path = completed.stdout.strip()
    if not candidate_path:
        logs.append(f"{case_id}: empty output")
        return {
            "case_id": case_id,
            "status": "error",
            "score": 0,
            "message": "empty output",
        }

    grid = case["input"]["grid"]
    benchmark_metrics = analyze_path(grid, case["benchmark_path"])
    if benchmark_metrics["status"] != "passed":
        raise ValueError(f"invalid benchmark for {case_id}: {benchmark_metrics['message']}")

    candidate_metrics = analyze_path(grid, candidate_path)
    if candidate_metrics["status"] != "passed":
        logs.append(f"{case_id}: {candidate_metrics['message']}")
        return {
            "case_id": case_id,
            "status": candidate_metrics["status"],
            "score": 0,
            "message": candidate_metrics["message"],
        }

    efficiency = benchmark_metrics["steps"] / max(candidate_metrics["steps"], 1)
    turn_bonus = min(
        (benchmark_metrics["turns"] + 1) / (candidate_metrics["turns"] + 1),
        1,
    )
    straight_bonus = min(
        candidate_metrics["longest_run"] / max(benchmark_metrics["longest_run"], 1),
        1,
    )
    score = round(
        0.60 + 0.20 * efficiency + 0.12 * turn_bonus + 0.08 * straight_bonus,
        4,
    )
    message = (
        f"steps={candidate_metrics['steps']}, turns={candidate_metrics['turns']}, "
        f"longest_run={candidate_metrics['longest_run']}, score={score}"
    )
    logs.append(f"{case_id}: {message}")

    return {
        "case_id": case_id,
        "status": "passed",
        "score": score,
        "message": message,
    }


def score_dataset(
    *,
    submission_entrypoint: Path,
    cases_path: Path,
    time_limit_sec: float,
    logs: list[str],
) -> list[dict[str, Any]]:
    cases = load_json(cases_path)
    return [
        score_case(
            case=case,
            submission_entrypoint=submission_entrypoint,
            time_limit_sec=time_limit_sec,
            logs=logs,
        )
        for case in cases
    ]


def summarize(results: list[dict[str, Any]]) -> dict[str, Any]:
    total = len(results)
    passed = sum(1 for result in results if result["status"] == "passed")
    average_score = 0 if total == 0 else round(sum(result["score"] for result in results) / total, 4)
    return {"score": average_score, "passed": passed, "total": total}


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
        shown_results = score_dataset(
            submission_entrypoint=submission_entrypoint,
            cases_path=problem_dir / spec["datasets"]["shown_dir"] / "cases.json",
            time_limit_sec=time_limit_sec,
            logs=logs,
        )
        hidden_results = score_dataset(
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

        official_results = score_dataset(
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
