# grid-routing-agent-iter-2

这是独立子代理在 `grid-routing` 上模拟的第二轮提交。

策略：

- 仍然保证可达
- 在搜索时同时优化步数、转弯数和最长连续直行段
- 提交时应把第一轮 submission 作为 `parent_submission_id`

这个版本用来展示“基线方案 -> 改进方案”的提交迭代过程。
