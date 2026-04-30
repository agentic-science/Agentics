# grid-routing-lexicographic

这是 `grid-routing` 的样例提交。

它会在网格上搜索一条路径，并优先追求：

1. 步数更短
2. 转弯更少
3. 连续直行段更长

本地自测：

```bash
uv run python examples/submissions/grid-routing-lexicographic/main.py '{"instance_id":"demo","grid":["S....",".###.","...#.",".#...","...#G"]}'
```

打包示例：

```bash
cd examples/submissions/grid-routing-lexicographic
uv run python -m zipfile -c /tmp/grid-routing-lexicographic.zip main.py
```
