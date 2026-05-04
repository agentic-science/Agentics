---
name: full-code-review
description: Use when performing a complete Agentics code review across backend, frontend, CLI, worker, protocol, security, architecture, tests, and documentation alignment.
---

# Full Code Review

This skill defines the expected review bar for Agentics. Use it before broad
reviews, release-readiness reviews, security reviews, or refactor planning.

## Review Stance

Act like a senior engineer and architect with a high quality bar. Prioritize
confirmed correctness, security, scalability, architecture, and maintainability
issues over stylistic preferences. Do not soften a real release blocker, and do
not inflate taste-only concerns into bugs.

Findings must be evidence-backed:

- Lead with findings, ordered by severity.
- Include exact file paths and tight line references.
- Explain the failure mode or architectural cost.
- State whether the issue is confirmed or a residual risk.
- Suggest a concrete remediation.
- Avoid vague comments such as "clean this up" without a target design.

## Required Review Lanes

Cover these lanes when the user asks for a complete review:

1. Backend Rust code quality
   - Non-idiomatic Rust, weak error handling, avoidable `unwrap` or `expect`,
     duplicated logic, excessive coupling, missing regression tests, and
     reinvented functionality that a mature crate should handle.
   - Check whether code can be simplified with current Rust language features
     and standard-library APIs documented in `docs/new-rust-features-apis/en.md`.
     Prefer these updates when they remove real nesting, repeated allocation,
     lossy error handling, platform-specific duplication, or manual time/path
     logic.
2. Frontend and CLI code quality
   - TypeScript and React correctness, schema drift, weak typing, state
     handling, i18n drift, CLI command structure, package misuse, and missing
     focused tests.
3. Security
   - Auth and authorization, hostile-code execution, Docker isolation, private
     benchmark leakage, path traversal, symlink handling, CORS, request limits,
     resource exhaustion, token storage, SSRF, XSS, and insecure defaults.
4. Backend architecture
   - Domain boundaries, worker and API lifecycle, evaluation state machines,
     protocol ownership, database constraints, migrations, scaling limits, and
     terminology consistency.
5. Frontend architecture
   - API contract ownership, route data loading, UI state boundaries, component
     size, visual-system consistency, admin workflow separation, tests, and CLI
     extensibility.

## Agentics-Specific Checks

Always inspect these platform-specific risks:

- Private benchmark data must not leak through public DTOs, logs, run IDs,
  per-case metrics, scorer messages, artifacts, or frontend render paths.
- Official evaluations must have quota, rate, queue, and storage controls before
  public deployment.
- Validation and official modes must be distinct in both product behavior and
  API exposure.
- Docker is not a sufficient hostile-code boundary by itself. Check container
  capabilities, users, PID limits, ulimits, read-only filesystems, network mode,
  bind mounts, log limits, and cleanup behavior.
- ZIP and workspace handling must reject path traversal, symlinks that escape
  roots, oversized artifacts, and excessive file counts or disk usage.
- Worker jobs need clear leases, retries, heartbeats, terminal states, and
  idempotent result handling.
- A refreshed lease is not enough to prove result ownership. For every worker
  job completion path, verify that the final database write is guarded by the
  current claim identity, such as worker ID plus attempt number or another
  monotonic claim token. Stale workers must not be able to overwrite newer
  attempts, terminal results, submission status, leaderboard rows, artifacts, or
  logs.
- Challenge bundle schemas, CLI packaging rules, web schemas, README examples,
  PRDs, milestones, and skills must stay aligned when behavior changes.
- Rust review passes should include a modernization check against
  `docs/new-rust-features-apis/en.md`, especially for `LazyLock`, let chains,
  `std::fs::exists`, `cfg_select!`, collection helpers, duration constructors,
  and newer path/string APIs.

## Subagent Instructions

When spawning a subagent for Rust backend, worker, or CLI review, explicitly ask
that subagent to read `docs/new-rust-features-apis/en.md` before reviewing code.
The subagent should report places where newer Rust features or APIs simplify
Agentics code without causing churn for its own sake.

## Severity Guidance

- P0: Release blocker, likely security compromise, private data leak, destructive
  data corruption, or uncontrolled public resource exhaustion.
- P1: Serious correctness, security, lifecycle, or scaling issue that should be
  fixed before MVP or before enabling the affected feature publicly.
- P2: Important maintainability, reliability, compatibility, or architecture
  concern that can be scheduled but should not be ignored.
- P3: Low-risk cleanup, test gap, or polish issue with limited blast radius.

## Validation Expectations

For implementation follow-up after review, require focused regression tests
around each fixed behavior. Before committing fixes, run the relevant checks:

- Rust: `cargo fmt --all`, `cargo check`, targeted tests, and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- Web: from `frontends/web`, run `bun run lint`, `bun run test`, and
  `bun run build` when frontend contracts or UI behavior changed.
- CLI: run targeted Rust tests for `frontends/agentics-cli` when CLI behavior
  changed.

For worker and queue fixes, include regression tests that simulate stale actors,
not only healthy-path timing. A good test should claim a job, requeue or advance
the claim, then make the old actor attempt to persist success or failure after a
newer claim exists. Assert that the stale write is a clean no-op and that the
newer result remains authoritative.

Keep commits logical. Do not combine unrelated security, architecture, docs, and
frontend changes in one commit unless they are part of the same behavioral fix.
