# Operational Scripting Policy

Agentics platform-owned operational automation should be Rust-first.
Shell is acceptable only for small wrappers or environment-specific entrypoints where a Rust executable would not improve safety, testability, cancellation, or error reporting.

## Scope

This policy applies when adding or changing platform-owned operational scripts and executables, including local MVP checks, Compose dev-data seeding, DGX Spark host/profile checks, DGX storage preparation, and DGX profile management.

Challenge payload scripts, sample solution scripts, and image-local smoke scripts have separate runtime contracts and are outside this policy unless a specific task says otherwise.

## Implementation Defaults

- Use the `agentics-ops` Cargo package for Rust operational automation.
- Use separate executables for unrelated operational tasks.
  Subcommands are appropriate only when the operations are part of one cohesive task family.
- Share common safety, logging, cancellation, filesystem, and process helpers through library modules instead of forcing unrelated tasks into one giant executable.
- Avoid shell command invocations when practical. Use Rust APIs, libraries, and typed data models first.
- Prefer idiomatic Rust and native crates such as `reqwest`, `serde_json`, `sqlx`, `bollard`, `tempfile`, and standard filesystem/process APIs when they fit the task.
- Reuse existing workspace code, configuration types, DTOs, validation helpers, and domain newtypes before adding new local abstractions.
- Do not duplicate default values, environment variable names, ports, paths, or other constants.
  Promote shared defaults to an appropriate common module and import them instead.
- Load environment variables at process boundaries through grouped raw env structs.
  Use normal deserialization for basic scalar values such as numbers, booleans, and enums; reserve custom parsing for lists, paths, URLs, secrets, and domain-specific validation.
  Convert raw strings into typed config, domain newtypes, URLs, paths, modes, and secrets before passing values inward.
  Avoid scattered `std::env::var` calls in operational logic.
- Treat external process execution as a typed boundary. Do not use `sh -c` for ordinary command execution.
- Keep command documentation close to the implementation and update matching operator/developer docs when behavior changes.

## Safety And Reliability

- Add Ctrl-C handling for long-running commands.
  Cancellation should stop waits, probes, and child processes, then clean up temporary state when possible.
- Parallelize independent checks or probes with async when it improves operator latency without making output confusing.
- Use precise domain errors instead of vague process failures.
  Bound captured stdout/stderr before putting command output into logs or user-facing errors.
- Redact secrets in command logs, diagnostics, and errors. Secrets must not be printed by default.
- Design commands to be idempotent. Re-running a command should either confirm the desired state or make a narrow, safe change toward it.
- For destructive or rootful DGX operations, provide a `--dry-run` option that reports intended mutations without applying them.
- Add rollback for risky mutations when partial failure could leave platform state worse than before.
  Rollback should affect only state owned or changed by the current invocation.
- Use explicit confirmation gates for destructive operations such as storage preparation or data purging.

## Documentation

Each operational executable should document:

- its purpose and supported workflow;
- required environment variables and flags;
- which operations are native Rust and which still invoke external commands;
- cancellation behavior;
- dry-run behavior for destructive commands;
- idempotence and rollback guarantees;
- targeted tests and any manual DGX validation required.
