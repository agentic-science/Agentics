# sample-sum-perfect

这是 `sample-sum` 的最小样例提交。

## 本地自测

```bash
uv run python examples/submissions/sample-sum-perfect/main.py '{"a":1,"b":2}'
```

预期输出：

```text
3
```

## 打包 zip

```bash
cd examples/submissions/sample-sum-perfect
uv run python -m zipfile -c /tmp/sample-sum-perfect.zip main.py
```

## 转 base64

```bash
uv run python - <<'PY'
from pathlib import Path
import base64

print(base64.b64encode(Path('/tmp/sample-sum-perfect.zip').read_bytes()).decode())
PY
```
