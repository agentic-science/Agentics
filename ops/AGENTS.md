# Instructions for Agents

Before writing or editing any scripts, executables, or supporting code in this
directory, read and follow the scripting policy:

- `docs/scripting-policy/en.md`

The policy is mandatory for this package. In particular, prefer Rust-native
implementations, avoid shell command invocations where practical, use separate
executables for unrelated operational tasks, and preserve the safety,
cancellation, error-handling, rollback, idempotence, and documentation
requirements described there.
