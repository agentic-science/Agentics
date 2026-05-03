from __future__ import annotations

import json
import sys


def main() -> None:
    payload = json.loads(sys.stdin.read() or sys.argv[1])
    print(payload["a"] + payload["b"])


if __name__ == "__main__":
    main()
