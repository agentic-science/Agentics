# Agentics v0.2 ZIP Project Protocol

本文档定义 v0.2 的 `zip_project` solution manifest。Manifest 是稳定的 metadata contract，让 Agentics 可以理解提交的 ZIP project，并在后续 milestone 加入 worker execution 之前解析 setup/build/run phase model。

Manifest 文件名为：

```text
agentics.solution.json
```

## Scope

`zip_project` 用于支持多语言 solution submissions。本地候选项目仍称为 solution。上传之后称为 solution submission。

当前 protocol code 定义 schema validation 和 phase model。它尚不改变 worker execution。Worker orchestration、resource enforcement、local benchmark-image validation 和 dependency layout enforcement 都是独立的 v0.2 milestones。

## Manifest Example

```json
{
  "protocol": "zip_project",
  "protocol_version": 1,
  "runtime": {
    "language": "python",
    "language_version": "3.12",
    "runtime_profile": "python-cpu"
  },
  "commands": {
    "setup": "scripts/setup.sh",
    "build": "scripts/build.sh",
    "run": "run.sh"
  },
  "phases": {
    "setup": {
      "timeout_sec": 120,
      "memory_limit_mb": 1024,
      "cpu_limit_millis": 1500,
      "disk_limit_mb": 2048,
      "network_access": "disabled",
      "log_limit_bytes": 2097152
    },
    "build": {
      "timeout_sec": 300,
      "network_access": "disabled"
    },
    "run": {
      "timeout_sec": 45,
      "network_access": "loopback"
    }
  },
  "interface": {
    "kind": "challenge_defined",
    "input_contract": "Challenge-defined JSON input.",
    "output_contract": "Challenge-defined stdout output."
  },
  "dependencies": {
    "policy": "lockfile",
    "lockfiles": ["requirements.lock"]
  }
}
```

## Top-Level Fields

| Field | Required | Meaning |
| --- | --- | --- |
| `protocol` | yes | 必须为 `zip_project`。 |
| `protocol_version` | yes | v0.2 schema 必须为 `1`。 |
| `runtime` | yes | Solution 声明的 language 和 runtime metadata。 |
| `commands` | yes | Setup、build 和 run phases 的 script paths。 |
| `phases` | no | Setup、build 和 run 的可选 per-phase limit overrides。 |
| `interface` | yes | Challenge harness 应如何调用 solution 并与其通信。 |
| `dependencies` | yes | Dependency source policy 和可选 dependency path metadata。 |

Unknown fields 会被拒绝。

## Runtime

```json
{
  "language": "python",
  "language_version": "3.12",
  "runtime_profile": "python-cpu"
}
```

规则：

- `language` 必填，且不能为空。
- `language_version` 可选，但如果存在则不能为空。
- `runtime_profile` 可选，但如果存在则不能为空。

Runtime metadata 在本 milestone 中只用于描述。后续 resource profile 和 worker milestones 会决定 runtime metadata 如何映射到 benchmark images 和 execution environments。

## Commands

```json
{
  "setup": "scripts/setup.sh",
  "build": "scripts/build.sh",
  "run": "run.sh"
}
```

规则：

- `run` 必填。
- `setup` 和 `build` 可选。
- 每个 command value 都是 ZIP project 内部的 script path。
- Script paths 必须是 safe relative paths。它们不能是 absolute paths，不能包含空路径片段，也不能包含 `..`。

v0.2 phase executor 会按顺序运行 `setup`、`build`、`run`。如果没有对应 command path，则跳过 `setup` 或 `build`。

## Phases

```json
{
  "setup": {
    "timeout_sec": 120,
    "memory_limit_mb": 1024,
    "cpu_limit_millis": 1500,
    "disk_limit_mb": 2048,
    "network_access": "enabled",
    "log_limit_bytes": 2097152
  },
  "build": {
    "timeout_sec": 300,
    "network_access": "disabled"
  },
  "run": {
    "timeout_sec": 45,
    "network_access": "loopback"
  }
}
```

`phases` 是可选字段。每个 phase object 都是 partial override；未声明的值使用 protocol defaults。如果整个 `phases` 被省略，Agentics 仍会根据 `commands` 解析出一个 concrete phase plan。

默认 limits：

| Phase | Timeout | Memory | CPU | Disk | Network | Log limit |
| --- | --- | --- | --- | --- | --- | --- |
| `setup` | 300 seconds | 512 MiB | 1000 millicpu | 1024 MiB | `disabled` | 1048576 bytes |
| `build` | 600 seconds | 512 MiB | 1000 millicpu | 1024 MiB | `disabled` | 1048576 bytes |
| `run` | 30 seconds | 512 MiB | 1000 millicpu | 1024 MiB | `disabled` | 1048576 bytes |

支持的 phase fields：

- `timeout_sec`：正整数，表示 wall-clock timeout，单位为秒。
- `memory_limit_mb`：正整数，表示 memory limit，单位为 MiB。
- `cpu_limit_millis`：正整数，表示 CPU allocation，单位为 millicpu，其中 `1000` 表示 one CPU。
- `disk_limit_mb`：正整数，表示 writable disk limit，单位为 MiB。
- `network_access`：取值为 `disabled`、`loopback` 或 `enabled`。后续 worker/resource milestones 应对 ranked official runs clamp 或拒绝 external network access。
- `log_limit_bytes`：正整数，表示 per-phase log capture limit。

规则：

- 只有存在 `commands.setup` 时，才能声明 `phases.setup`。
- 只有存在 `commands.build` 时，才能声明 `phases.build`。
- `phases.run` 始终允许，因为 `commands.run` 是必填字段。
- 值为零的 limits 会被拒绝。

Parser 会暴露带有 concrete limits 的 ordered phase execution plan。后续 worker milestones 会使用该 plan 产生 phase-specific status、logs 和 failure reports。Failure report 包含 failed phase name、reason、message、可选 exit code，以及可选 safe relative log path。

## Interface

```json
{
  "kind": "challenge_defined",
  "input_contract": "Challenge-defined JSON input.",
  "output_contract": "Challenge-defined stdout output."
}
```

支持的 `kind` values：

- `challenge_defined`
- `argv`
- `stdio`
- `file_system`
- `http`

`input_contract` 和 `output_contract` 是可选描述字段。如果存在，不能为空。

对 v0.2 来说，`challenge_defined` 是最安全的默认值，因为现有 challenges 拥有自己的精确 invocation contract。更具体的 interface kinds 可以留给未来 standardized harnesses 使用。

## Dependencies

```json
{
  "policy": "lockfile",
  "lockfiles": ["requirements.lock"],
  "vendor_dirs": ["vendor"],
  "notes": "Uses only packages pinned in requirements.lock."
}
```

支持的 `policy` values：

- `vendored`
- `lockfile`
- `image_provided`

规则：

- `policy` 必填。
- `lockfiles` 可选。
- `vendor_dirs` 可选。
- `notes` 可选，但如果存在则不能为空。
- `lockfiles` 和 `vendor_dirs` 中的条目必须是 safe relative paths。
- 每个列表内部不能有重复 paths。

本 milestone 校验 schema 和 path safety。更强的 dependency policy enforcement，例如 `vendored` 必须包含 vendored directories 或 `lockfile` 必须包含 lockfiles，属于 `M0.2-PROTO-3`。

## Validation Summary

有效 manifest 必须：

1. 使用 `protocol: "zip_project"`。
2. 使用 `protocol_version: 1`。
3. 声明非空的 runtime language metadata。
4. 声明 safe relative `commands.run` script path。
5. 对可选 setup、build、lockfile 和 vendor directory references 只使用 safe relative paths。
6. 如果存在 `phases`，只声明合法的 phase overrides。
7. 声明一个受支持的 interface kind。
8. 声明一个受支持的 dependency policy。
9. 不包含 unknown fields。

## Current Compatibility

当前 v0.1 worker 仍执行 legacy Python ZIP project contract。`zip_project` manifests 已经可以在 shared protocol code 中被解析，并且 setup/build/run phase plan 也可以被解析，但 worker execution support 会在后续 v0.2 milestones 中实现。
