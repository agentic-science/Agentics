# 运维脚本策略

Agentics 平台拥有的 operational automation 应默认优先使用 Rust。Shell 只适合很小的
wrappers，或那些使用 Rust executable 并不能改善安全性、可测试性、取消处理或错误报告的环境特定 entrypoints。

## 范围

本策略适用于新增或修改平台拥有的 operational scripts 和 executables，包括 local MVP
checks、Compose demo seeding、DGX Spark host/profile checks、DGX storage
preparation，以及 DGX profile management。

Challenge payload scripts、sample solution scripts 和 image-local smoke scripts
有各自独立的 runtime contracts；除非具体任务另有说明，否则不属于本策略范围。

## 实现默认规则

- Rust operational automation 使用 `agentics-ops` Cargo package。
- 对互不相关的 operational tasks 使用 separate executables。只有当多个 operation
  属于同一个 cohesive task family 时，才使用 subcommands。
- 通过 library modules 共享 common safety、logging、cancellation、filesystem 和
  process helpers，不要把互不相关的 tasks 强行塞进一个巨大 executable。
- 能避免 shell command invocation 时就避免。优先使用 Rust APIs、libraries 和
  typed data models。
- 任务适配时，优先使用 idiomatic Rust 和 native crates，例如 `reqwest`、
  `serde_json`、`sqlx`、`bollard`、`tempfile`，以及标准 filesystem/process APIs。
- 新增本地 abstraction 之前，先复用 workspace 中已有的 code、configuration
  types、DTOs、validation helpers 和 domain newtypes。
- 不要重复定义 default values、environment variable names、ports、paths 或其他
  constants。应将 shared defaults 提升到合适的 common module，然后 import 使用。
- 在 process boundary 通过 grouped raw env structs 加载 environment variables，
  对 numbers、booleans 和 enums 等 basic scalar values 使用普通反序列化；只有
  lists、paths、URLs、secrets 和 domain-specific validation 才使用 custom parsing。
  然后尽早把 raw strings 转成 typed config、domain newtypes、URLs、paths、modes
  和 secrets，再向内传递。Operational logic 中避免分散的 `std::env::var` 调用。
- 将 external process execution 视为 typed boundary。普通 command execution 不使用
  `sh -c`。
- Command documentation 应靠近 implementation；行为变化时同步更新对应的 operator
  docs 和 developer docs。

## 安全性与可靠性

- 为 long-running commands 添加 Ctrl-C handling。Cancellation 应停止 waits、probes
  和 child processes，并在可行时清理 temporary state。
- 独立 checks 或 probes 可以用 async parallelization 来降低 operator latency，但不应让输出变得难以理解。
- 使用精确的 domain errors，而不是模糊的 process failures。将 command output 写入
  logs 或 user-facing errors 前，应限制 captured stdout/stderr 的大小。
- 在 command logs、diagnostics 和 errors 中 redact secrets。Secrets 默认不得打印。
- Commands 应设计为 idempotent。重复运行 command 时，应确认目标状态已满足，或只执行狭窄且安全的修正。
- 对 destructive 或 rootful DGX operations，提供 `--dry-run` option，用于报告即将执行的 mutations，而不实际应用。
- 对 partial failure 可能让 platform state 变差的 risky mutations，应提供 rollback。Rollback
  只应影响本次 invocation 拥有或修改过的 state。
- 对 storage preparation 或 data purging 等 destructive operations，使用显式 confirmation gates。

## 文档要求

每个 operational executable 都应记录：

- purpose 和 supported workflow；
- 必需的 environment variables 和 flags；
- 哪些 operations 使用 native Rust，哪些仍调用 external commands；
- cancellation behavior；
- destructive commands 的 dry-run behavior；
- idempotence 和 rollback guarantees；
- targeted tests，以及需要的 manual DGX validation。
