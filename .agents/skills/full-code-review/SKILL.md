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
   - Stringly typed domain identifiers. Flag stable IDs with validation,
     authorization, ranking, storage, routing, or security meaning when they are
     represented as raw `String` or `&str` beyond the external parsing
     boundary. Prefer explicit validated newtypes for challenge IDs, benchmark
     target IDs, solution submission IDs, agent IDs, asset IDs, metric IDs, and
     worker claim IDs.
   - Check whether code can be simplified with current Rust language features
     and standard-library APIs documented in `docs/new-rust-features-apis/en.md`.
     Prefer these updates when they remove real nesting, repeated allocation,
     lossy error handling, platform-specific duplication, or manual time/path
     logic.
2. Frontend and CLI code quality
   - TypeScript and React correctness, schema drift, weak typing, state
     handling, i18n drift, CLI command structure, package misuse, and missing
     focused tests.
   - Public contract drift caused by hand-written or duplicated identifier
     schemas. Frontend code should consume generated schemas and stable re-export
     types, and CLI code should parse identifiers through the shared Rust DTOs
     rather than accepting arbitrary strings deep in command execution.
3. Security
   - Auth and authorization, hostile-code execution, Docker isolation, private
     benchmark leakage, path traversal, symlink handling, CORS, request limits,
     resource exhaustion, token storage, SSRF, XSS, and insecure defaults.
   - For Rust code that touches the operating system, treat memory safety as
     only the baseline. Review filesystem races, permission windows, path
     identity checks, Unix byte and UTF-8 assumptions, panic-based denial of
     service, and silently dropped errors.
4. Backend architecture
   - Domain boundaries, worker and API lifecycle, evaluation state machines,
     protocol ownership, database constraints, migrations, scaling limits, and
     terminology consistency.
5. Frontend architecture
   - API contract ownership, route data loading, UI state boundaries, component
     size, visual-system consistency, admin workflow separation, tests, and CLI
     extensibility.
6. Test quality
   - Find trivial or low-value tests and report them as P3 findings unless they
     mask a higher-risk issue. Flag tests that only restate constants, assert
     fields on freshly constructed structs, check generic library behavior,
     assert static labels without exercising workflow behavior, or duplicate
     coverage already provided by stronger contract or integration tests.
   - Recommend deletion when a test has no meaningful regression value.
     Recommend replacement when the surrounding code needs coverage of real
     behavior, edge cases, security properties, API contracts, or user-visible
     workflows.

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
- ZIP, artifact, and workspace handling must be reviewed as filesystem security
  code. Reject path traversal, symlinks that escape roots, oversized artifacts,
  and excessive file counts or disk usage. Also check repeated path operations
  that can create TOCTOU windows, `File::create` where `create_new` is required,
  create-then-`chmod` permission windows, lossy UTF-8 filename handling, and
  ignored extraction, copy, cleanup, or log errors.
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
- Domain identifiers should be explicit, validated, and canonical. Search for
  raw `String` or `&str` fields named like `*_id`, function parameters such as
  `challenge_id: &str`, and ambiguous names like `id_or_slug`,
  `challenge_id_or_slug`, `slug`, `identifier`, or `name_or_id`. Treat these as
  architectural smells unless they are immediate raw boundary inputs that are
  parsed into a newtype before any business logic, database lookup,
  authorization check, queue operation, or filesystem/storage path construction.
- Canonical lookup should use one public identifier unless product requirements
  explicitly demand aliases. Before MVP, do not preserve old locator aliases with
  compatibility shims; remove the alias and update API, CLI, web, docs, schemas,
  and tests together.
- Rust review passes should include a modernization check against
  `docs/new-rust-features-apis/en.md`, especially for `LazyLock`, let chains,
  `std::fs::exists`, `cfg_select!`, collection helpers, duration constructors,
  and newer path/string APIs.

## Subagent Instructions

When spawning a subagent for Rust backend, worker, or CLI review, explicitly ask
that subagent to read `docs/new-rust-features-apis/en.md` before reviewing code.
The subagent should report places where newer Rust features or APIs simplify
Agentics code without causing churn for its own sake.

For Rust security review, also ask subagents to scan for CVE-prone tool patterns
near untrusted input and OS boundaries. Use targeted `rg` searches as review
starting points, then manually judge context:

- `File::create|fs::metadata|fs::set_permissions|fs::remove_file`
- `from_utf8_lossy|str::from_utf8|String::from_utf8`
- `\.ok\(\)|unwrap_or_default|let _ =`
- `unwrap\(|expect\(|panic!|\[[^\]]+\]`
- `== Path::new|== "/"|PathBuf.*==`

For domain modeling review, use targeted searches for stringly typed IDs and
ambiguous locators, then inspect call flow manually:

- `pub [a-zA-Z0-9_]*_id: String|[a-zA-Z0-9_]*_id: &str`
- `challenge_id: String|challenge_id: &str|benchmark_target_id: String`
- `solution_submission_id: String|agent_id: String|metric_id: String`
- `id_or_slug|challenge_id_or_slug|slug|identifier|name_or_id`
- `bind\(&[a-zA-Z0-9_]*_id\)|join\(&[a-zA-Z0-9_]*_id\)`

Do not report a raw string merely because it appears at an HTTP path extractor,
CLI parser field, SQL bind, display formatter, or test fixture. Report it when
the raw value crosses into semantic code without validation, or when the same
logical ID can be looked up through multiple public aliases.

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

Do not add tests just to increase test count. A good test should protect a
specific behavior, contract, regression, security property, or workflow. Avoid
tests that merely restate implementation details or prove that a dependency does
what its own test suite already covers.

- Rust: `cargo fmt --all`, `cargo check`, targeted tests, and
  `cargo clippy --workspace --all-targets -- -D warnings`.
  Production crates opt into workspace Clippy lints for `unwrap`, `expect`,
  `panic`, indexing, and arithmetic side effects. Treat tests as the only
  blanket exemption.
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
