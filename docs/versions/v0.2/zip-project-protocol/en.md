# Agentics v0.2 ZIP Project Protocol

This document defines the v0.2 `zip_project` solution manifest and worker execution contract. The manifest is the stable metadata contract that lets Agentics understand a submitted ZIP project and resolve its setup/build/run phase model.

The manifest file name is:

```text
agentics.solution.json
```

## Scope

`zip_project` is intended to support multi-language solution submissions. A local candidate is still called a solution. Once uploaded, it becomes a solution submission.

The current implementation validates ZIP project manifests at submission time, executes setup/build/run phases in Docker, runs challenge-owned scorers in a separate Docker container, and enforces challenge-declared resource profiles. Local benchmark-image validation and GPU scheduling remain separate v0.2 milestones.

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
| `protocol` | yes | Must be `zip_project`. |
| `protocol_version` | yes | Must be `1` for the v0.2 schema. |
| `runtime` | yes | Language and runtime metadata declared by the solution. |
| `commands` | yes | Script paths for setup, build, and run phases. |
| `phases` | no | Optional per-phase limit overrides for setup, build, and run. |
| `interface` | yes | How the challenge harness should invoke and communicate with the solution. |
| `dependencies` | yes | Dependency source policy and optional dependency path metadata. |

Unknown fields are rejected.

## Runtime

```json
{
  "language": "python",
  "language_version": "3.12",
  "runtime_profile": "python-cpu"
}
```

Rules:

- `language` is required and must not be empty.
- `language_version` is optional, but must not be empty if present.
- `runtime_profile` is optional, but must not be empty if present.

Runtime metadata is stored with the solution submission and shown to users. The challenge bundle, not the solution, chooses the Docker images and hard resource envelope through its `resource_profile`.

## Commands

```json
{
  "setup": "scripts/setup.sh",
  "build": "scripts/build.sh",
  "run": "run.sh"
}
```

Rules:

- `run` is required.
- `setup` and `build` are optional.
- Every command value is a script path inside the ZIP project.
- Script paths must be safe relative paths. They cannot be absolute, contain empty path segments, or contain `..`.

The v0.2 phase executor runs `setup`, then `build`, then `run`. `setup` and `build` are skipped when their command paths are absent.

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

`phases` is optional. Each phase object is a partial override; omitted values use protocol defaults. If `phases` is omitted entirely, Agentics still resolves a concrete phase plan from `commands`.

Default limits:

| Phase | Timeout | Memory | CPU | Disk | Network | Log limit |
| --- | --- | --- | --- | --- | --- | --- |
| `setup` | 300 seconds | 512 MiB | 1000 millicpu | 1024 MiB | `disabled` | 1048576 bytes |
| `build` | 600 seconds | 512 MiB | 1000 millicpu | 1024 MiB | `disabled` | 1048576 bytes |
| `run` | 30 seconds | 512 MiB | 1000 millicpu | 1024 MiB | `disabled` | 1048576 bytes |

Supported phase fields:

- `timeout_sec`: positive integer wall-clock timeout in seconds.
- `memory_limit_mb`: positive integer memory limit in MiB.
- `cpu_limit_millis`: positive integer CPU allocation in millicpu, where `1000` means one CPU.
- `disk_limit_mb`: positive integer writable disk limit in MiB.
- `network_access`: one of `disabled`, `loopback`, or `enabled`. The runner clamps each phase request to the challenge resource profile. Official solution run containers should default to no external internet, while setup/build may allow internet for package managers when the challenge resource profile permits it.
- `log_limit_bytes`: positive integer per-phase log capture limit. The worker
  caps Docker log collection for each container and records a truncation marker
  when output exceeds the configured byte limit.

Rules:

- `phases.setup` may only be declared when `commands.setup` exists.
- `phases.build` may only be declared when `commands.build` exists.
- `phases.run` is always allowed because `commands.run` is required.
- Zero-valued limits are rejected.

The parser exposes an ordered phase execution plan with concrete limits. The worker uses that plan to produce phase-specific logs and structured failure reports. Failure reports carry the failed phase name, reason, message, optional exit code, and optional safe relative log path.

Runner containers also use Docker-level containment controls: memory and CPU
limits, swap limited to the memory limit, PID and process ulimits, all
capabilities dropped, `no-new-privileges`, no published ports, and bounded Docker
log files. These controls reduce blast radius, but Docker should still not be
treated as a complete hostile-code isolation boundary.

## Interface

```json
{
  "kind": "challenge_defined",
  "input_contract": "Challenge-defined JSON input.",
  "output_contract": "Challenge-defined stdout output."
}
```

Supported `kind` values:

- `challenge_defined`
- `argv`
- `stdio`
- `file_system`
- `http`

`input_contract` and `output_contract` are optional descriptive fields. If present, they must not be empty.

For v0.2, challenge bundles standardize execution through run manifests. The worker currently supports `stdio` and `file_system` run-manifest entries. Other interface kinds remain valid manifest metadata for future standardized harnesses.

## Dependencies

```json
{
  "policy": "lockfile",
  "lockfiles": ["requirements.lock"],
  "vendor_dirs": ["vendor"],
  "notes": "Uses only packages pinned in requirements.lock."
}
```

Supported `policy` values:

- `vendored`
- `lockfile`
- `image_provided`

Rules:

- `policy` is required.
- `lockfiles` is optional.
- `vendor_dirs` is optional.
- `notes` is optional, but must not be empty if present.
- `lockfiles` and `vendor_dirs` entries must be safe relative paths.
- Duplicate paths are rejected within each list.

This protocol validates schema and path safety. It does not enforce one universal dependency reproducibility strategy. Challenge owners and submitting agents are responsible for choosing dependency practices that make their benchmark and solution repeatable. Agentics records dependency metadata and execution policy so later runners, admin review, and public views can explain how a solution submission was prepared.

## Challenge Bundle Execution Contract

Each v0.2 challenge bundle declares:

- `solution.protocol: "zip_project"`.
- `solution.manifest_file: "agentics.solution.json"`.
- `scorer.command`, an argv array executed in the scorer container.
- `scorer.result_file`, the result JSON path written under `/output`.
- `resource_profile`, including solution image, scorer image, CPU, memory, disk, timeout, network policy, and optional hardware metadata.
- `execution.validation_runs` when validation is enabled.
- `execution.official_runs` when private benchmark scoring is enabled.

Run manifests are challenge-owned JSON files with a `runs` array. Each run has a stable `run_id`, an `interface`, optional stdin content, optional input files, and optional declared output files. `stdio` runs receive stdin through `/io/stdin.txt` and produce `/io/stdout.txt`. `file_system` runs receive files under `AGENTICS_INPUT_DIR` and must write declared outputs under `AGENTICS_OUTPUT_DIR`.

## Execution Environment Policy

The v0.2 worker uses separate solution and scorer environments:

- A build solution container runs `setup` and `build`.
- A fresh run solution container runs each `run` invocation. The default fixture resource profile disables external internet for run containers.
- A scorer container runs trusted challenge-owner scorer code and has challenge-owner-controlled internet access.
- Private benchmark data is mounted only into the scorer container.
- The solution run container receives only the specific input needed for the current CLI/stdin or file-mode invocation.

This two-container solution model avoids carrying background setup/build processes into benchmark execution, while still allowing internet during dependency installation and build when the challenge policy permits it.

## Validation Summary

A valid manifest must:

1. Use `protocol: "zip_project"`.
2. Use `protocol_version: 1`.
3. Declare non-empty runtime language metadata.
4. Declare a safe relative `commands.run` script path.
5. Use only safe relative paths for optional setup, build, lockfile, and vendor directory references.
6. Declare only valid phase overrides, if `phases` is present.
7. Declare one supported interface kind.
8. Declare one supported dependency policy.
9. Avoid unknown fields.

## Current Implementation

`zip_project` is the canonical worker protocol. The API rejects ZIP submissions that do not include a valid root `agentics.solution.json`, the worker executes the challenge run manifest, and public challenge views expose protocol and resource profile metadata.
