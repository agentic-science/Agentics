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

## General Review Lanes

Before the manual review lanes, run the repository large-file scanner from the
workspace root:

```bash
cargo run -p agentics-ops --bin agentics-scan-large-files -- --threshold 1200 --watch-threshold 900
```

Treat every `FAIL` line as a P2 maintainability finding that requires a
refactor before substantial new behavior is added to that file. Include the
scanner output or a concise summary in the review log. The command exits
nonzero when oversized files are found; that is a review signal, not a reason to
skip the finding. Treat `WARN` lines as a watch list and call them out when the
reviewed change adds more code to those files. If the scanner cannot run, record
that as residual risk and run an equivalent native line-count pass.

Cover these project-agnostic lanes when the user asks for a complete review:

1. Backend Rust code quality
   - Non-idiomatic Rust, weak error handling, avoidable `unwrap` or `expect`,
     duplicated logic, excessive coupling, missing regression tests, and
     reinvented functionality that a mature crate should handle.
   - Stringly typed domain identifiers. Flag stable names or IDs with validation,
     authorization, ranking, storage, routing, or security meaning when they are
     represented as raw `String` or `&str` beyond the external parsing
     boundary. Prefer explicit validated newtypes for human-authored names
     such as challenge names, target names, asset names, run names, resource
     profile names, and metric names, and for generated IDs such as solution
     submission IDs, agent IDs, draft IDs, job IDs, and worker claim IDs.
   - Stringly typed URLs, storage keys, and paths. Flag raw `_url`, `_uri`,
     `_path`, and `path: String` fields after external parsing boundaries.
     Prefer `url::Url` or contract-specific URL wrappers, `StorageKey` for
     storage-relative keys, and explicit wrappers for repository-relative,
     server-local filesystem, bundle-relative, solution-project, run I/O, log,
     archive, and container paths. Also flag ad hoc URL validators, string
     prefix URL checks, and scattered `.trim()`/case conversion near domain
     parser calls when the normalization belongs in the newtype constructor.
     Git object identifiers such as PR commit SHAs should use the repo's
     `gix-hash` backed domain type, not handwritten hex validators. Keep Git
     object IDs separate from ordinary SHA-256 content digests, token hashes,
     and Docker image digests. Ordinary SHA-256 content digests should be stored
     as `[u8; 32]` in a domain type with lowercase hex serialization. OCI/Docker
     image digest fields should use the `oci-spec` backed image digest wrapper
     and preserve the `sha256:<hex>` wire format.
   - Secret handling. Flag passwords, OAuth client secrets, API tokens, bearer
     tokens, and other long-lived credentials stored as plain `String` beyond
     immediate HTTP, CLI, env, config-file, or database boundaries. Prefer
     `secrecy` wrappers and require any `ExposeSecret` call to be located at the
     exact transmission or comparison boundary.
   - Check whether code can be simplified with current Rust language features
     and standard-library APIs documented in `references/rust-modernization.md`.
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

- Private benchmark data must not leak through public DTOs, logs, run names,
  per-case metrics, evaluator messages, artifacts, or frontend render paths.
- Public projection and redaction should be centralized. Review public
  submission lists, public submission details, result reports, ranking context,
  leaderboards, score distributions, frontend rendering, and CLI rendering as
  one redaction matrix. Do not accept separate ad hoc redaction logic in each
  route or component when the same private benchmark policy is being enforced.
- Official evaluations must have quota, rate, queue, and storage controls before
  public deployment.
- Admission controls must be transactional. Quotas, active challenge checks,
  draft limits, staged job reservations, shortlist eligibility, owner checks,
  archive-state gates, and active official-job limits should be enforced in the
  same database transaction that creates or mutates the durable row. Treat
  check-then-insert or check-then-queue code as a likely P1 unless a database
  constraint, lock, or compare-and-swap transition makes the race impossible.
- Capacity counts must include every state that consumes capacity. For Agentics
  this usually includes `staged`, `queued`, and `running` jobs, pending draft
  validations, active drafts, reserved storage, and disabled-but-not-cleaned
  resources when they still occupy quota.
- Validation and official modes must be distinct in both product behavior and
  API exposure.
- Secrets must be traced end to end. Pioneer codes, bearer tokens, admin
  passwords, OAuth client secrets, OAuth state, database URLs, and one-time
  registration tokens must not appear in query parameters, logs, debug output,
  default CLI output, browser storage, generated snapshots, or schema fixtures.
  Secret-bearing login or registration starts should use POST bodies or headers,
  not GET query strings. Explicit print-once paths are acceptable only when they
  do not also persist the secret.
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
- Challenge and solution workflows must be reviewed as workflows, not only as
  files. Walk through agent registration, creator OAuth login, draft creation,
  private asset upload, challenge publication, solution submission, worker
  claim/requeue/complete, public result viewing, leaderboard reads, and score
  distribution reads. Confirm who is authorized, what transaction protects the
  state change, what durable rows are written, and what public data becomes
  visible afterward.
- Accepted MVP risks must be explicit. If a finding is accepted for MVP, record
  the risk, the compensating controls, the operational assumption, and the
  follow-up issue. Do not silently downgrade risks such as short pioneer-code
  entropy or writable-rootfs runner behavior into "not a problem".
- Domain locators should be explicit, validated, and canonical. Human-authored
  values must be named `*Name`, not `*Id`; generated opaque values must be
  named `*Id`. Search for raw `String` or `&str` fields named like `*_id` or
  `*_name`, function parameters such as `challenge_name: &str`, and ambiguous
  names like `id_or_slug`, `challenge_name_or_slug`, `slug`, `identifier`, or
  `name_or_id`. Treat these as architectural smells unless they are immediate
  raw boundary inputs that are parsed into a newtype before any business logic,
  database lookup, authorization check, queue operation, or filesystem/storage
  path construction.
- Canonical lookup should use one public identifier unless product requirements
  explicitly demand aliases. Before MVP, do not preserve old locator aliases with
  compatibility shims; remove the alias and update API, CLI, web, docs, schemas,
  and tests together.
- Domain constructors, parsers, and generators should be owned by the domain
  type. Flag free-standing helpers such as `new_*_id`, `parse_*_name`,
  `parse_*_status`, or `parse_*_manifest` when they create or parse a specific
  domain value. Prefer `Type::generate()` for generated IDs, `FromStr`,
  `TryFrom`, `try_new`, or associated constructors for value parsing, and
  `from_storage_value` beside persisted enums. Generic HTTP/CLI boundary
  adapters and database row adapters are acceptable only when they immediately
  delegate to the domain type and do not encode domain rules themselves.
- Rust review passes should include a modernization check against
  `references/rust-modernization.md`, especially for `LazyLock`, let chains,
  `std::fs::exists`, `cfg_select!`, collection helpers, duration constructors,
  and newer path/string APIs.

## Subagent Instructions

When spawning a subagent for Rust backend, worker, or CLI review, explicitly ask
that subagent to read `references/rust-modernization.md` before reviewing code.
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

For database admission and state-machine review, scan for code that checks a
condition separately from the durable state change it protects. Then inspect
whether a transaction, lock, unique constraint, or compare-and-swap transition
actually closes the race:

- `COUNT\\(\\*\\)|count_.*\\(`
- `INSERT INTO .*solution_submissions|INSERT INTO .*evaluation_jobs|INSERT INTO .*challenge_drafts`
- `UPDATE .* SET .*status|status = 'staged'|status = 'queued'|status = 'running'`
- `FOR UPDATE|pg_advisory|ON CONFLICT|WHERE .*status`
- `begin\\(\\)\\.await|commit\\(\\)\\.await`
- `challenge.*active|archived|quota|limit|capacity|owner|shortlist`

For secret-lifecycle review, trace each secret from boundary input to storage,
output, and errors. Search broadly, then manually classify whether the value is
secret material, a hash, an opaque row id, or a non-secret display value:

- `pioneer_code|token|bearer|password|secret|client_secret|oauth|csrf`
- `URLSearchParams|query\\(|authorization_url|GET .*login`
- `println!|eprintln!|tracing::|format!|Debug|to_string\\(\\)`
- `sessionStorage|localStorage|clipboard|window\\.location`
- `ExposeSecret|expose_secret|SecretString`

For domain modeling review, use targeted searches for stringly typed IDs and
ambiguous locators, then inspect call flow manually:

- `pub [a-zA-Z0-9_]*_(id|name): String|[a-zA-Z0-9_]*_(id|name): &str`
- `challenge_name: String|challenge_name: &str|target: String`
- `solution_submission_id: String|agent_id: String|metric_name: String`
- `challenge_name|target_id|asset_id|run_id|resource_profile\.id|metric_id`
- `id_or_slug|challenge_name_or_slug|slug|identifier|name_or_id`
- `bind\(&[a-zA-Z0-9_]*_id\)|join\(&[a-zA-Z0-9_]*_id\)`

For domain helper ownership review, scan for free-standing Rust functions that
look like constructors, parsers, or generators. Then inspect whether they own
domain validation that belongs on a type:

- `fn new_[a-zA-Z0-9_]+\\(`
- `fn parse_[a-zA-Z0-9_]+\\(`
- `Uuid::new_v4\\(\\).*try_new|try_new\\(.*Uuid::new_v4`
- `match .*\\.as_str\\(\\).*=>.*Status|unknown .*status`

Do not report protocol boundary helpers such as HTTP response parsing, bearer
token parsing, test-only helpers, or row adapters whose only job is to extract a
database column and call the domain type. Do report row adapters that contain
hard-coded enum string matches or UUID generation outside an ID newtype.

For typed-boundary review, spawn or assign a specific scan for URL, storage, and
path contracts. Start with these searches and then inspect whether each raw
string is only an immediate boundary value or has leaked into semantic code:

- `_[uU]rl: String|_[uU]rl: &str|url: String|url: &str`
- `_[uU]ri: String|_[uU]ri: &str|storage_uri|storage_key: String`
- `_[pP]ath: String|_[pP]ath: &str|path: String|path: &str`
- `validate_.*url|urlish|starts_with\\(\"https://|contains\\(\"github.com\"`
- `\\.trim\\(\\).*parse|parse_.*\\(.*\\.trim\\(|try_new\\(.*\\.trim\\(`
- `to_lowercase\\(\\).*try_new|try_new\\(.*to_lowercase\\(`
- `commit_sha: String|commit_sha: &str|validate_commit_sha|[sS][hH][aA].*chars\\(\\).*is_ascii_hexdigit`
- `password: String|client_secret: String|api_token: String|bearer.*String|secret.*String`
- `ExposeSecret|expose_secret`

Do not report raw path strings for literal Docker mount points, human-readable
messages, test fixtures, SQL display/bind code, or request/CLI fields that are
parsed immediately before business logic. Do report them when they participate
in authorization, filesystem/storage access, repository lookup, artifact
assembly, runner contracts, or public DTO schemas without a typed contract.

Do not report a raw string merely because it appears at an HTTP path extractor,
CLI parser field, SQL bind, display formatter, or test fixture. Report it when
the raw value crosses into semantic code without validation, or when the same
logical ID can be looked up through multiple public aliases.

For public-result redaction review, build a table of result-bearing surfaces and
confirm they share the same private-benchmark policy:

- public challenge solution-submission list
- public solution-submission detail
- public result report
- public ranking context
- public leaderboard
- public score distribution
- observer web render path
- agentics-cli render path

Report any surface that reads private aggregate metrics, run metrics, logs,
case-level messages, artifacts, or evaluator output without passing through the
public projection/redaction boundary.

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
