from __future__ import annotations

import heapq
import json
import sys


DIRECTIONS = {
    "D": (1, 0),
    "L": (0, -1),
    "R": (0, 1),
    "U": (-1, 0),
}


def find_marker(grid: list[str], marker: str) -> tuple[int, int]:
    for row_index, row in enumerate(grid):
        for col_index, cell in enumerate(row):
            if cell == marker:
                return row_index, col_index
    raise ValueError(f"marker {marker!r} not found")


def solve(grid: list[str]) -> str:
    height = len(grid)
    width = len(grid[0]) if grid else 0
    start = find_marker(grid, "S")
    goal = find_marker(grid, "G")
    queue: list[tuple[int, int, int, str, int, int, str]] = []
    best_cost: dict[tuple[int, int, str, int, int], tuple[int, int, int]] = {}
    heapq.heappush(queue, (0, 0, 0, "", start[0], start[1], ""))

    while queue:
        steps, turns, neg_longest_run, path, row, col, previous_move = heapq.heappop(queue)
        current_run = 0
        if path:
            last_move = path[-1]
            for move in reversed(path):
                if move != last_move:
                    break
                current_run += 1
        state = (row, col, previous_move, current_run, len(path))
        state_cost = (steps, turns, neg_longest_run)

        if state in best_cost and best_cost[state] <= state_cost:
            continue
        best_cost[state] = state_cost

        if (row, col) == goal:
            return path

        for move, (delta_row, delta_col) in DIRECTIONS.items():
            next_row = row + delta_row
            next_col = col + delta_col
            if next_row < 0 or next_row >= height or next_col < 0 or next_col >= width:
                continue
            if grid[next_row][next_col] == "#":
                continue

            next_turns = turns + (1 if previous_move and previous_move != move else 0)
            next_path = path + move
            next_run = current_run + 1 if previous_move == move else 1
            next_longest_run = max(-neg_longest_run, next_run)
            heapq.heappush(
                queue,
                (
                    steps + 1,
                    next_turns,
                    -next_longest_run,
                    next_path,
                    next_row,
                    next_col,
                    move,
                ),
            )

    raise RuntimeError("goal is unreachable")


def main() -> None:
    payload = json.loads(sys.argv[1])
    print(solve(payload["grid"]))


if __name__ == "__main__":
    main()
