# Agentics v0.2 ZIP Project Protocol

This document defines the v0.2 `zip_project` solution manifest. The manifest is the stable metadata contract that lets Agentics understand a submitted ZIP project before later milestones add multi-phase execution.

The manifest file name is:

```text
agentics.solution.json
```

## Scope

`zip_project` is intended to support multi-language solution submissions. A local candidate is still called a solution. Once uploaded, it becomes a solution submission.

This milestone defines schema and validation only. It does not yet change worker execution. Setup, build, run phase orchestration, resource enforcement, local benchmark-image validation, and dependency layout enforcement are separate v0.2 milestones.

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

The runtime metadata is descriptive in this milestone. Later resource profile and worker milestones decide how runtime metadata maps to benchmark images and execution environments.

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

The v0.2 phase executor will later run `setup`, then `build`, then `run` when those phases are supported.

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

For v0.2, `challenge_defined` is the safest default because existing challenges own their exact invocation contract. More specific interface kinds are available for future standardized harnesses.

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

This milestone validates the schema and path safety. Strong dependency policy enforcement, such as requiring vendored directories for `vendored` or lockfiles for `lockfile`, belongs to `M0.2-PROTO-3`.

## Validation Summary

A valid manifest must:

1. Use `protocol: "zip_project"`.
2. Use `protocol_version: 1`.
3. Declare non-empty runtime language metadata.
4. Declare a safe relative `commands.run` script path.
5. Use only safe relative paths for optional setup, build, lockfile, and vendor directory references.
6. Declare one supported interface kind.
7. Declare one supported dependency policy.
8. Avoid unknown fields.

## Current Compatibility

The current v0.1 worker still executes the legacy Python ZIP project contract. `zip_project` manifests are parsed and documented in v0.2 protocol code, but worker execution support arrives in later v0.2 milestones.
