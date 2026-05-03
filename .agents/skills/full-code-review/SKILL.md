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
- Challenge bundle schemas, CLI packaging rules, web schemas, README examples,
  PRDs, milestones, and skills must stay aligned when behavior changes.

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

Keep commits logical. Do not combine unrelated security, architecture, docs, and
frontend changes in one commit unless they are part of the same behavioral fix.
