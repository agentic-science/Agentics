# sample-sum-perfect

这是 `sample-sum` 的最小样例提交。

## 本地自测

```bash
printf '{"a":1,"b":2}' | uv run python examples/solutions/sample-sum-perfect/main.py
```

预期输出：

```text
3
```

## 打包 zip

```bash
CHALLENGE_ID="$(agentics challenges list --json | jq -r '.items[] | select(.challenge_name == "sample-sum") | .challenge_id')"
agentics submit "$CHALLENGE_ID" --target linux-arm64-cpu --dir examples/solutions/sample-sum-perfect
```

## 转 base64

```bash
uv run python - <<'PY'
from pathlib import Path
import base64

print(base64.b64encode(Path('/tmp/sample-sum-perfect.zip').read_bytes()).decode())
PY
```
