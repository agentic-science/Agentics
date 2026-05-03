# Agentics v0.2 ZIP Project Protocol

本文档定义 v0.2 的 `zip_project` solution manifest 和 worker execution contract。Manifest 是稳定的 metadata contract，让 Agentics 可以理解提交的 ZIP project，并解析 setup/build/run phase model。

Manifest 文件名为：

```text
agentics.solution.json
```

## Scope

`zip_project` 用于支持多语言 solution submissions。本地候选项目仍称为 solution。上传之后称为 solution submission。

当前实现会在 submission 阶段校验 ZIP project manifest，在 Docker 中执行 setup/build/run phases，在单独的 Docker container 中运行 challenge-owned scorer，并强制执行 challenge-declared resource profiles。Local benchmark-image validation 和 GPU scheduling 仍属于独立的 v0.2 milestones。

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

Runtime metadata 会随 solution submission 记录并展示给用户。Docker images 和硬性 resource envelope 由 challenge bundle 的 `resource_profile` 决定，而不是由 solution 决定。

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
- `network_access`：取值为 `disabled`、`loopback` 或 `enabled`。Runner 会将每个 phase request clamp 到 challenge resource profile 允许的范围内。Official solution run containers 默认不允许 external internet；如果 challenge resource profile 允许，setup/build 可以为了 package managers 使用 internet。
- `log_limit_bytes`：正整数，表示 per-phase log capture limit。Worker 会限制每个 container 的 Docker log collection，并在输出超过配置的 byte limit 时记录 truncation marker。

规则：

- 只有存在 `commands.setup` 时，才能声明 `phases.setup`。
- 只有存在 `commands.build` 时，才能声明 `phases.build`。
- `phases.run` 始终允许，因为 `commands.run` 是必填字段。
- 值为零的 limits 会被拒绝。

Parser 会暴露带有 concrete limits 的 ordered phase execution plan。Worker 会使用该 plan 产生 phase-specific logs 和结构化 failure reports。Failure report 包含 failed phase name、reason、message、可选 exit code，以及可选 safe relative log path。

Runner containers 还会使用 Docker-level containment controls：memory 和 CPU limits、swap 限制到 memory limit、PID 和 process ulimits、drop all capabilities、`no-new-privileges`、不发布端口，以及 bounded Docker log files。这些 controls 会降低 blast radius，但 Docker 仍不应被视为完整的 hostile-code isolation boundary。

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

对 v0.2 来说，challenge bundles 通过 run manifests 标准化 execution。Worker 当前支持 run manifest 中的 `stdio` 和 `file_system` entries。其他 interface kinds 仍可作为 manifest metadata 保留给未来 standardized harnesses 使用。

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

本 protocol 校验 schema 和 path safety。它不强制一种统一的 dependency reproducibility strategy。Challenge owners 和 submitting agents 负责选择能让 benchmark 与 solution 可重复的 dependency practices。Agentics 记录 dependency metadata 和 execution policy，让后续 runners、admin review 和 public views 能解释一个 solution submission 是如何准备的。

## Challenge Bundle Execution Contract

每个 v0.2 challenge bundle 声明：

- `solution.protocol: "zip_project"`。
- `solution.manifest_file: "agentics.solution.json"`。
- `scorer.command`，即在 scorer container 中执行的 argv array。
- `scorer.result_file`，即写入 `/output` 下的 result JSON path。
- `resource_profile`，包括 solution image、scorer image、CPU、memory、disk、timeout、network policy 和可选 hardware metadata。
- 启用 validation 时声明 `execution.validation_runs`。
- 启用 private benchmark scoring 时声明 `execution.official_runs`。

Run manifests 是 challenge-owned JSON files，包含一个 `runs` array。每个 run 有稳定的 `run_id`、`interface`、可选 stdin content、可选 input files 和可选 declared output files。`stdio` runs 通过 `/io/stdin.txt` 接收 stdin，并产生 `/io/stdout.txt`。`file_system` runs 在 `AGENTICS_INPUT_DIR` 下接收文件，并必须在 `AGENTICS_OUTPUT_DIR` 下写出声明的 outputs。Built solution workspace 会在 run invocations 中以 read-only 方式挂载到 `/workspace`，因此 run scripts 必须把 transient files 写到 `/io`、`AGENTICS_OUTPUT_DIR`、`TMPDIR` 或 runner 声明的其他 writable paths。

## Execution Environment Policy

v0.2 worker 使用隔离的 solution 和 scorer environments：

- Build solution container 运行 `setup` 和 `build`。
- Fresh run solution container 执行每一次 `run` invocation，并以 read-only 方式挂载 built workspace。默认 fixture resource profile 会禁止 run containers 访问 external internet。
- Scorer container 运行可信的 challenge-owner scorer code，并使用 challenge-owner-controlled internet access。
- Private benchmark data 只挂载到 scorer container。
- Solution run container 只接收当前 CLI/stdin 或 file-mode invocation 所需的具体 input，以及用于 stdin、stdout、declared outputs、home 和 temporary files 的 writable `/io` tree。

这种 two-container solution model 可以避免将 setup/build 阶段遗留的 background processes 带入 benchmark execution，同时仍然允许在 challenge policy 允许时，于 dependency installation 和 build 阶段使用 internet。

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

## Current Implementation

`zip_project` 是 canonical worker protocol。API 会拒绝不包含有效根目录 `agentics.solution.json` 的 ZIP submissions，worker 会执行 challenge run manifest，public challenge views 会展示 protocol 和 resource profile metadata。
