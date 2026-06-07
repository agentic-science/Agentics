# Instructions for Agents

## Conversation Requirements

- Always ask for more clarification if you are not sure about the specification of a task. You are encouraged to ask more questions before do the task.
- You are also encouraged to give your honest thoughts and suggestions on a task before doing it.
- Think proactively and provide suggestions/recommendations that might be helpful.

## Documentation

For your information:

- `docs/PRD/en.md` and `docs/PRD/zh.md` are the product requirements documents. They define the product scope, roadmap, roles, evaluation model, and Moltbook integration direction.
- `docs/milestones/en.md` and `docs/milestones/zh.md` are the actionable milestone plans. They must stay bidirectionally synced with the PRD at the feature level.
- `docs/api-json-contract/en.md` and `docs/api-json-contract/zh.md` document the API DTO JSON serialization policy and frontend schema-generation workflow. Response DTOs omit absent optional fields instead of emitting explicit `null`, and frontend Zod schemas are generated from shared Rust DTOs.
- `docs/README.md` is the documentation index.
- `docs/contribute-code/en.md` and `docs/contribute-code/zh.md` are the role-facing setup and workflow guides for code contributors.
- `docs/contribute-challenges/en.md` and `docs/contribute-challenges/zh.md` are the role-facing guides for challenge creators and owners.
- `docs/review-challenges/en.md` and `docs/review-challenges/zh.md` are the role-facing guides for challenge reviewers.
- `docs/solution-protocol/en.md` and `docs/solution-protocol/zh.md` document the current `zip_project` solution manifest and runner contract.
- `docs/targets/en.md` and `docs/targets/zh.md` document target schema, target-scoped submission APIs, CLI behavior, worker behavior, and leaderboard behavior.
- `docs/deployment/en.md` and `docs/deployment/zh.md` document the MVP Compose deployment flows, startup order, storage, backup, rollback, and production shutdown semantics.
- `docs/dgx-spark/en.md` and `docs/dgx-spark/zh.md` document DGX Spark host preparation, inventory, storage quotas, profile checks, and smoke evidence for the production Compose worker.
- `docs/operations/en.md` and `docs/operations/zh.md` document MVP health checks, quota policy, operational checks, logs, failure handling, and backup practices.
- `docs/ports-and-paths/en.md` and `docs/ports-and-paths/zh.md` document runtime ports, filesystem paths, and MVP target support.
- `docs/visual-identity-system/en.md` and `docs/visual-identity-system/zh.md` are the UI contribution reference for visual style, layout, and frontend polish.
- `.agents/skills/full-code-review/references/rust-modernization.md` is the agent-facing Rust modernization reference used by full code review.
- `docker/runner-images/` contains public runner image contracts referenced by targets and challenge specs.
- `deploy/` contains internal platform deployment assets, including Compose files and service image builds.
- `skills/agentics-cli-workflow/SKILL.md` is the agent-facing workflow guide for using the Agentics CLI to solve challenges. Keep it aligned with CLI command changes and README examples.
- `skills/challenge-authoring-workflow/SKILL.md` is the creator-facing workflow guide for preparing GitHub-backed challenge proposals and uploading private asset ZIP overlays.
- `.agents/skills/challenge-review-workflow/SKILL.md` is the admin/reviewer workflow guide for validating, approving, publishing, archiving, and cleaning up challenge review records.

### Requirements

- If they have multi-lingual versions, always update all versions when one version is updated.
- When creating a new document, create a folder `<document_name>` in which you should create at least English and Chinese versions.
- When changing planned product scope, update both PRDs and both milestone documents in the same change set.
- When changing implemented behavior, update the matching current docs and then update milestones if the implementation status changes.
- When changing Rust response DTOs consumed by the web frontend, derive `schemars::JsonSchema`, preserve the optional-field JSON contract, run `bun install --frozen-lockfile` and `bun run generate:schemas` in `frontends/web/`, and keep `frontends/web/src/lib/schemas.ts` as a stable re-export facade.
- DO NOT skip tests because of trivial reasons (e.g., "a test needs a DB but the DB is not started")

## Coding Requirements

### General Engineering Guidelines

- Always prioritize code quality and avoid bad SWE practices
- Always group changes into logical commits and never commit changes of different features and purposes in one commit
- Do not commit changes automatically unless told (e.g., "do this and commit the changes").
- Don't rebuild the wheels: if there's a commonly used package/library for a feature or sub-feature, do not implement the functionalities yourself, unless the user explicitly ask you to rewrite or avoid external packages. If unsure, always ask for clarification.
- Do not write trivial or low-value tests. Tests must protect meaningful behavior, contracts, regressions, security properties, or user-visible workflows. Avoid tests that only restate constants, assert freshly constructed struct fields, check library serialization mechanics, or verify static labels without exercising behavior.
- Keep track of file sizes. If a file has more then 1200 lines of code, propose a refactor to the user.
- When fixing lint findings, preserving behavior is mandatory. In particular, replacing `unwrap`, `expect`, indexing, or other panic-prone code must not silently continue, skip work, substitute defaults, or weaken limits when the previous code would fail fast. Prefer eliminating impossible states by construction, for example by building the correctly typed value directly instead of constructing a generic value and then asserting its shape. If a failure can really happen at runtime, handle it with a clear domain error. If the old code represented an internal invariant that cannot be eliminated, convert it to a precise internal error, not a vague message such as "static value must be an object".

### Commit Message Convention

Use a lightweight Conventional Commit style for new commits:

```text
<type>(<scope>): <imperative summary>

<body explaining why and notable details>

<footer, if needed>
```

For cross-cutting commits where one scope would be misleading, omit the scope:

```text
<type>: <imperative summary>
```

Allowed types:

- `feat`: new behavior or user-facing capability.
- `fix`: bug, security, lifecycle, or correctness fix.
- `refactor`: restructuring without intended behavior change.
- `docs`: documentation, README, or agent skill updates.
- `test`: test-only changes.
- `chore`: tooling, dependency, metadata, or generated-only maintenance.
- `perf`: performance improvement.
- `style`: formatting-only changes with no behavior change.

Use repo-local, concrete scopes such as `api`, `runner`, `cli`, `web`, `docs`, `ops`, `challenge-spec`, `db`, or `schemas`.

Commit subject rules:

- Use imperative mood, such as `add`, `fix`, `reject`, or `document`.
- Keep the subject under about 72 characters when practical.
- Do not end the subject with a period.
- Mention the user-visible or public contract when that is the important change.

Commit body rules:

- Use a multiline body when the "why" is not obvious, behavior changes, migrations are involved, or tradeoffs matter.
- Write the body as motivation and important consequences, not a file-by-file changelog.
- Include verification notes only when useful, especially for non-obvious tests or intentionally skipped checks.
- Use footers such as `BREAKING CHANGE: ...` or `Refs #123` when they add useful context.

Use a multiline body for any commit that changes public APIs, DB schema, runner security behavior, challenge specs, or operational workflow. Narrow docs/client/test-only commits may stay one-line if the subject is self-explanatory.

### Agentics-Specific Requirements

- Before the public MVP release, DO NOT consider any internal or external API compatibilities. If a new feature or a refactor needs to reasonably discard existing code, just do it. For example, if a backend change for a good reason breaks the APIs for the frontend, DO NOT add compatibility shims/layers/aliases. Instead, just fix the frontend.
- Avoid stringly typed domain identifiers. Stable identifiers with validation rules or security/authorization meaning, such as challenge names, target names, solution submission IDs, agent IDs, asset names, metric names, and worker claim IDs, should use explicit Rust newtypes instead of raw `String` or `&str` in semantic models, DTOs, database records, APIs, and CLI command plumbing. Parse and validate raw strings only at external boundaries, then pass the typed value inward. Narrow exceptions are immediate boundary inputs before parsing, database bind/display calls, and tests that explicitly construct a valid typed ID.
- Use `name` for human-authored stable labels and `id` only for platform-generated opaque identifiers. Human-authored values such as `ChallengeName`, `TargetName`, `MetricName`, `AssetName`, `RunName`, and `ResourceProfileName` belong in Rust name newtypes and JSON fields like `challenge_name`, `asset_name`, `run_name`, or nested `name`. Generated values such as solution submission IDs, agent IDs, review record IDs, job IDs, and revision IDs belong in Rust ID newtypes as they are added and JSON fields like `*_id` or object `id`.
- If a generated ID is canonically a UUID, store it in PostgreSQL as `UUID`, not `TEXT`. Convert to and from the Rust newtype or wire string at the database boundary with explicit casts/helpers instead of weakening the domain type internally.
- Avoid ambiguous locator contracts such as `id_or_slug`, duplicate alias fields, or fallback lookup by multiple public identifiers unless the product explicitly requires them. Prefer one canonical identifier and remove compatibility aliases before MVP instead of carrying cognitive load through the API, CLI, web, and database layers.
- Put canonical normalization in typed constructors, not scattered call sites. Use `nutype` sanitizers only when normalization is semantics-preserving, such as trimming a metric name query or lowercasing a lowercase challenge slug. Avoid ad hoc `.trim()`, `.to_lowercase()`, or similar cleanup immediately before domain parser calls unless the domain type cannot own the normalization for a clear reason.
- Do not create free-standing domain constructors, parsers, or generators such as `new_evaluation_job_id`, `parse_metric_name`, or `parse_draft_status`. Generated IDs should expose `Type::generate()`. Domain values should use `FromStr`, `TryFrom`, `try_new`, or an associated constructor such as `Manifest::from_zip_bytes`. Persisted enum storage parsing belongs beside the enum as `from_storage_value`. Generic HTTP/CLI boundary adapters and database row adapters are acceptable only when they immediately delegate to the domain type.
- URLs must use `url::Url` or explicit URL wrappers after the external boundary. Do not add ad hoc URL validators, string-prefix URL checks, or raw `_url: String` fields in semantic DTOs and models. Use dedicated wrappers for distinct URL contracts such as GitHub remotes, GitHub PR URLs, Moltbook Submolt URLs, external data URLs, API base URLs, and future callback/origin URLs.
- Secrets such as passwords, OAuth client secrets, API tokens, and long-lived bearer credentials must use `secrecy` wrappers after the external boundary. Keep raw strings only at the immediate HTTP, CLI, env, config-file, or database boundary, expose secrets only at the call site that must transmit or compare them, and never derive or log structures that would print secret contents.
- Git object identifiers, such as challenge review record `commit_sha`, must use a domain wrapper backed by `gix-hash` instead of handwritten hexadecimal validation. Ordinary SHA-256 content digests must use a domain type backed by `[u8; 32]` with lowercase hex serialization. OCI/Docker image digests must use the `oci-spec` backed image digest wrapper and preserve the `sha256:<hex>` wire format. Do not use a Git object ID type for content hashes, bundle hashes, asset hashes, token hashes, or Docker image digests.
- Storage keys and path-like values must be typed after parsing. Use explicit wrappers for storage keys, repository-relative paths, server-local filesystem paths, archive paths, bundle-relative paths, solution project paths, run input/output paths, log paths, and future container paths. Raw `_path`, `_uri`, or `path: String` values are acceptable only at immediate request/CLI/deserialization boundaries before validation, in SQL bind/display code, or in tests.
- Database checks that gate capacity, ownership, lifecycle state, authorization, or durable work must be admission controls, not advisory preflight checks. Put quota checks, active challenge checks, review record limits, staged job reservations, and state transitions in the same transaction as the row insert/update they protect. Use row locks, advisory locks, unique constraints, or compare-and-swap transitions, and count `staged`, queued, running, or reserved records whenever they consume platform capacity.
- State machines must be explicit and guarded. Status updates should validate the expected current state, worker completion paths must verify the active claim identity before writing results, and leaderboard or result repair code must be idempotent and scoped by challenge, target, agent, and solution submission as appropriate.
- Secret handling must be reviewed as a full lifecycle. Pioneer codes, bearer tokens, admin passwords, OAuth secrets, DB URLs, and one-time registration tokens must not appear in URLs, query parameters, logs, error messages, debug output, default CLI output, browser storage, or generated snapshots. Prefer POST bodies or headers over GET query parameters for secret-bearing flows. Any intentionally exposed secret path, such as a CLI `--print-token`, must be explicit, one-time, and must not also persist the secret.
- Public API, CLI, and web result surfaces must use explicit projection and redaction logic. Do not let public list, detail, result-report, leaderboard, score-distribution, frontend render, and CLI render paths each decide private benchmark visibility independently. Private benchmark data requires coverage across every public surface that can expose results.
- Runner and artifact code must be treated as hostile-input filesystem code. Docker layer quota, XFS mount quota, network policy, symlink rejection, ZIP traversal rejection, log limits, scratch cleanup, and worker-owned metadata paths protect different surfaces and must not be treated as substitutes for one another.
- Regression tests for Agentics security and lifecycle fixes must exercise the real invariant: concurrent or transaction-level tests for admission controls, stale-claim tests for worker writes, output/error-path tests for secrets, public-surface tests for redaction, and schema regeneration plus frontend schema tests for DTO changes.

### Technical Requirements

- Always assume `uv` for managing Python environments and `bun` for JS/TS environments, unless you are explicitly told to use other tools.
- Only run lint, check and format tools (e.g., `cargo clippy`, `cargo check`, `cargo fmt`, `bunx biome`, `ruff`) before committing, not during iteration. Skip these when fixing bugs/issues to accelerate iteration speed.
- A full project test pass means `just test-all`, which uses the Docker Compose test harness and includes ignored GPU/CUDA tests. If the user explicitly asks for CPU-only verification, use `just test-all-cpu`. Manual command-by-command runs are not equivalent unless they cover the same Compose harness mode.
- Before running the full suites, use `just test-env-status-cpu` for CPU-only verification or `just test-env-status` for GPU verification. Use `just test-env-up` and `just test-env-down` to manage only the dedicated test Docker daemon after `/srv/agentics-test` has been prepared.
- Non-canonical Just helpers are namespaced. Use commands such as `just dev::up`, `just prod::check`, `just storage::s3-test`, `just rust::clippy`, `just web::schema-check`, and `just maintenance::setup-hooks` instead of old flat recipe names.
- When Docker access requires sudo for dev services, do not let the Compose project default to `agentics-dev-root`. The `just dev::*` recipes infer the invoking user from `SUDO_USER`; if that is unavailable, set `AGENTICS_DEV_USER` or `AGENTICS_COMPOSE_DEV_PROJECT` explicitly before running the command.
- Production rehearsals use the disposable `agentics-rehearsal` environment through `just rehearsal::prepare-storage`, `just rehearsal::runner-docker-up`, `just rehearsal::up`, `just rehearsal::check`, and `just rehearsal::run` or `just rehearsal::run-cpu`. Purge only with `sudo just rehearsal::purge-data --confirm-rehearsal-purge`. Do not point rehearsal env files at real production data.
- NO unsafe fixes should be applied even if a linter provides them. You should reason about the code to be fixed and come up appropriate fixes.
