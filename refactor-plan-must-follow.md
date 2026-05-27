# Modularity Refactor Plan: Config Completion, Crate Boundaries, Services, Persistence, Runner, Frontend

## Summary

Finish the incomplete `garde` config refactor and apply the modularity findings from the review pass without changing product behavior.

This is an internal pre-MVP refactor. It must not intentionally change HTTP routes, JSON DTO wire shapes, CLI commands, web routes, DB schema, challenge bundle schema, evaluator result contracts, runner quota/security behavior, or operational defaults.

The work should be split into focused commits so each boundary improvement is reviewable and reversible.

## Phase 1: Finish The Previous Config Refactor

### Goal

Replace the flat 70-field `Config` with real grouped config structs. The previous `garde` pass only added grouped validation views, which was incomplete.

### Key Changes

- Change `Config` from flat fields to grouped ownership:
  - `database: DatabaseConfig`
  - `api_web: ApiWebConfig`
  - `storage: StorageConfig`
  - `auth: AuthConfig`
  - `moltbook: MoltbookConfig`
  - `worker: WorkerConfig`
  - `quotas: QuotaConfig`
  - `github_oauth: GithubOauthConfig`
  - `runner: RunnerConfig`
  - `logging: LoggingConfig`
- Move `garde::Validate` derives onto the real grouped structs:
  - top-level `Config` uses `#[garde(dive)]` for groups.
  - field-local checks live on fields.
  - cross-field policy checks remain explicit methods.
- Keep raw env grouping as-is, but convert `RawAppEnv -> Config` by constructing grouped configs directly.
- Delete temporary borrowed validation view structs such as:
  - `ApiSecurityFields`
  - `StorageCommonFields`
  - `S3Fields`
  - `RunnerOutputLimitFields`
- Keep custom validator helpers where useful:
  - trimmed non-empty string
  - secret non-empty
  - cookie name
  - CORS origin list
  - optional absolute path
  - optional HTTP URL
  - S3 prefix
  - runner slot-class CSV
- Update all call sites:
  - `config.runner_max_runs` -> `config.runner.max_runs`
  - `config.storage_backend` -> `config.storage.backend`
  - `config.admin_password` -> `config.auth.admin_password`
  - and equivalent grouped paths everywhere.
- Preserve existing derived helper methods:
  - `Config::from_env`
  - `Config::validate`
  - `Config::validate_api_security`
  - `Config::validate_runner_storage`
  - `Config::requires_digest_pinned_images`
  - `Config::runner_runtime_root`
  - `Config::runner_writable_slot_classes_mb`
  - `Config::worker_gpu_probe_image`
  - `Config::cors_allowed_origin_values`

### Commit

`refactor(config): group runtime configuration`

## Phase 2: Clean Crate Dependency Boundaries

### Goal

Make crate dependencies match the intended architecture instead of leaking infrastructure types sideways.

### Key Changes

- Remove `sqlx` and `zip` dependencies from `agentics-domain`.
  - Domain keeps typed values, DTOs, API error envelope DTOs, and semantic models.
  - Database errors must be owned by persistence or services.
  - ZIP errors must be owned by contracts or archive validation.
  - `ServiceError` should not live in domain if it requires infrastructure error variants.
- Move transport-neutral application errors into `agentics-services`.
  - Add conversions from persistence, contracts, storage, and runner errors into `ServiceError`.
  - API converts `ServiceError` into `ApiError`.
  - CLI maps contract/runner errors directly where services are not involved.
- Remove unnecessary `agentics-storage` dependency from `agentics-persistence`.
  - Persistence should use `agentics_domain::storage::StorageKey`.
  - Maintenance code that needs actual storage operations should move to services or a maintenance service module.
- Remove direct `bollard` dependency from `agentics-services`.
  - Services should depend on `agentics-runner` abstractions, not Docker types.
  - Worker creates the Docker backend and passes a runner/backend trait object or concrete runner service to `EvaluationWorkerService`.
- Reduce direct persistence/storage imports from `api-server`.
  - Handlers may keep pool creation, app state, auth extraction, and response wrapping.
  - Business workflow calls should go through services.
  - Direct persistence access may remain only for simple auth/session extraction until that is explicitly service-wrapped.

### Commit

`refactor(crates): tighten architecture dependencies`

## Phase 3: Make Runner Backend Boundary Real

### Goal

Make execution topology orchestration independent from database and Docker implementation details.

### Key Changes

- Keep Docker as the only implemented backend for MVP.
- Remove `PgPool` and `sqlx` usage from `agentics-runner`.
  - Runner should not decide whether a DB claim is current.
  - Runner should inspect containers and return typed container facts/actions.
  - Services compare those facts against persistence state and decide keep/kill/requeue.
- Split runner modules:
  - `context`: runner context and immutable execution inputs.
  - `labels`: Agentics Docker/container label model.
  - `limits`: output/log/result/interaction limits.
  - `plans`: backend-neutral container execution plans.
  - `setup`: setup/build/prepare phase orchestration helpers.
  - `result`: evaluator result loading and validation.
  - `backend`: `RunnerBackend` trait and backend result types.
  - `docker`: Docker backend implementation only.
- Replace duplicated setup/prepare helpers with one shared phase runner:
  - separated evaluator prepare uses evaluator setup stage.
  - piped stdio prepare uses evaluator setup stage.
  - coexecuted benchmark prepare uses evaluator setup stage.
  - command args/env/mounts are parameters, not separate near-clone functions.
- Make `RunnerBackend` own backend operations only:
  - pre-pull image
  - run one container
  - run interactive stdio session
  - inspect/list/remove Agentics containers
  - cleanup backend resources
- Keep topology logic above backend:
  - `separated_evaluator`
  - `piped_stdio`
  - `coexecuted_benchmark`
- Preserve all current behavior:
  - labels
  - GPU device requests
  - network policy
  - Docker layer quota
  - writable slot quota
  - log limits
  - interaction byte limits
  - permission repair behavior
  - public/private bundle mounting rules

### Commit

`refactor(runner): decouple backend execution from database state`

## Phase 4: Split Storage Into Backend Modules

### Goal

Make durable storage easier to audit and extend by separating trait, intents, local backend, S3 backend, and archive helpers.

### Key Changes

- Split `crates/storage/src/lib.rs` into modules:
  - `error`
  - `intent`
  - `key_ext` if storage-specific helpers are needed
  - `backend`
  - `local`
  - `s3`
  - `factory`
  - `archive`
- Keep public exports narrow:
  - `Storage`
  - `StorageError`
  - `StorageWriteIntent`
  - `LocalStorage`
  - `S3Storage`
  - `build_storage`
  - archive pack/unpack helpers that are intentionally public
- Remove service-layer conversions from storage.
  - `impl From<StorageError> for ServiceError` belongs in services.
- Reduce dependency on global `Config`.
  - Add storage-owned option structs:
    - `LocalStorageOptions`
    - `S3StorageOptions`
    - `StorageFactoryOptions`
  - Config constructs these options.
  - Storage does not need to know unrelated config fields.
- Preserve S3/local behavior:
  - no-overwrite semantics
  - size intent enforcement
  - temp-to-durable promotion behavior
  - head-object length verification
  - stale temp cleanup
  - local symlink rejection

### Commit

`refactor(storage): split object storage backends`

## Phase 5: Split Challenge Domain And Contract Models

### Goal

Reduce the overloaded challenge model file and make challenge contracts, execution topology, targets, metrics, and DTOs easier to reason about.

### Key Changes

- Split challenge-related domain modules by concern:
  - lifecycle/status and IDs
  - published challenge DTOs
  - target/resource profile DTOs
  - execution topology DTOs
  - metric schema/result DTOs
  - dataset/statement metadata
  - draft/public/admin DTOs where appropriate
- Keep wire shapes unchanged.
  - Field names and serialization behavior must not change.
  - Optional-field omission behavior must remain unchanged.
- Move contract-only validation logic to `agentics-contracts`.
  - Domain can define shared DTO/newtype shapes if needed.
  - Contracts owns challenge bundle parsing and validation policy.
  - Do not let domain depend on archive parsing or ZIP implementation details.
- Keep generated frontend schema output semantically unchanged.
  - Regenerate only if Rust type module paths or derive locations require it.

### Commit

`refactor(domain): split challenge models by concern`

## Phase 6: Move DTO Projection Out Of Persistence

### Goal

Persistence should return rows/records, not public/admin HTTP DTOs.

### Key Changes

- Keep persistence responsible for:
  - SQL
  - transactions
  - row locks
  - admission primitives
  - row-to-record adapters
  - storage-key parsing from rows
  - DB enum parsing
- Move DTO construction to services:
  - admin challenge list projection
  - public challenge detail/list projection
  - solution submission public/admin/owner projection
  - leaderboard and score-distribution response construction
- Repository facades should return record types:
  - `ChallengeRecord`
  - `ChallengeDraftRecord`
  - `SolutionSubmissionRecord`
  - `EvaluationRecord`
  - `LeaderboardRecord`
  - focused list item records where SQL shape differs materially
- Tighten repository exports:
  - remove broad `pub use db::*`
  - export repositories, input structs, record structs, and error types only
  - keep SQL helper functions private
- Split repository responsibilities:
  - challenge catalog/metadata
  - challenge ownership/shortlist
  - challenge draft publication primitives
  - submission admission/quota
  - submission reads
  - leaderboard mutation/listing
  - score-distribution query support
  - maintenance primitives

### Commit

`refactor(persistence): return records instead of DTO projections`

## Phase 7: Split Public Projection Service

### Goal

Keep backend-owned redaction and projection centralized, but make it auditable by surface.

### Key Changes

- Split `public_projection` into focused modules:
  - `visibility`: audience and result visibility decisions.
  - `challenge`: public/agent challenge detail projection.
  - `submission`: owner/public/admin submission projection.
  - `leaderboard`: leaderboard and ranking-context response construction.
  - `score_distribution`: metric visibility and distribution construction.
  - `metrics`: primary metric display helpers and metric value formatting inputs.
- Add a small projection context type:
  - challenge record
  - parsed challenge spec
  - target name
  - audience
  - visibility policy
- Keep all public redaction decisions in services.
  - Frontend and CLI continue to render backend-provided fields.
  - Persistence must not decide public/private metric visibility.
- Preserve behavior:
  - official result-of-record selection
  - validation visibility
  - official-only metric redaction
  - archived challenge direct-read behavior
  - close-time visibility
  - artifact/log visibility

### Commit

`refactor(services): split public projection surfaces`

## Phase 8: Split Challenge Draft And Submission Services Further

### Goal

The service layer now owns the right workflows, but several modules are still large enough to become hard to audit.

### Key Changes

- Split challenge draft service:
  - `create`
  - `read`
  - `private_assets`
  - `validation`
  - `review`
  - `publishing`
  - `cleanup`
  - `moltbook_metadata` if not already separate
- Split solution submission service:
  - `admission`
  - `artifact_staging`
  - `quota`
  - `job_staging`
  - `cleanup`
  - `presentation`
- Keep state machines service-owned.
  - DB still enforces transitions and transactions.
  - Services own cross-boundary orchestration and cleanup.
- Preserve rollback behavior:
  - temp object cleanup
  - durable object cleanup
  - staged DB row cleanup
  - job readiness transition
  - private asset failure marking

### Commit

`refactor(services): split draft and submission workflows`

## Phase 9: Thin API Handlers Further

### Goal

Handlers should be transport adapters, not workflow participants.

### Key Changes

- Handlers keep:
  - path/query/body extraction
  - auth/session extraction
  - CSRF checks
  - HTTP status mapping
  - `Json(...)` wrapping
- Move remaining direct persistence use into services where it represents a workflow:
  - admin dashboard/list reads if they assemble multiple records
  - creator stats and shortlist mutation
  - artifact/log visibility checks
  - challenge catalog public reads
  - health checks may remain direct if they only check DB reachability
- Keep direct storage in app state.
  - Passing `Arc<dyn Storage>` into services is fine.
  - Handlers should not construct storage keys or storage intents except for purely transport-local download responses.

### Commit

`refactor(api): delegate remaining workflows to services`

## Phase 10: Frontend Data And Component Modularity

### Goal

Keep the existing UX but reduce shell/component coupling, duplicated mutation code, and direct fetch calls.

### Key Changes

- Creator frontend:
  - `CreatorConsole` becomes a shell.
  - Move identity/session logic into hooks.
  - Move draft form state into a hook.
  - Move private asset upload state and mutation into a hook.
  - Move owner stats/participants/shortlist loading into hooks.
  - Panels receive data/actions as props and do not fetch directly.
- Admin frontend:
  - Split admin panels into focused files:
    - overview
    - challenge registry
    - draft review
    - capacity
    - operations
    - submission actions
  - Move draft review mutations into hooks.
  - Move private asset row cache into a hook or SWR resource.
- Shared frontend data layer:
  - keep one typed `fetchJson`.
  - keep one API error parser.
  - ensure admin/creator endpoint modules validate outgoing bodies.
  - mutation hooks call endpoint wrappers and invalidate SWR keys.
- Shared UI primitives:
  - alert banner
  - tab list
  - stat card
  - table shell
  - text area
  - select
  - file input
  - empty state
- Preserve:
  - routes
  - translations
  - visual design
  - DTO shapes
  - SWR key semantics
  - observer pages except where using shared helper is trivial

### Commit

`refactor(web): split console data hooks and panels`

## Phase 11: CLI Output And Command Module Split

### Goal

Make CLI rendering and command workflows easier to maintain without changing commands or output semantics.

### Key Changes

- Split CLI output module:
  - `output/auth`
  - `output/challenges`
  - `output/submissions`
  - `output/validation`
  - `output/drafts`
  - `output/metrics`
  - `output/table`
  - `output/json`
- Introduce a shared render context:
  - output format
  - localization/display preferences if currently implicit
  - terminal/table helpers
- Split command workflows:
  - `commands/auth`
  - `commands/challenges`
  - `commands/submissions`
  - `commands/validation`
  - `commands/drafts`
  - `commands/local_validation`
  - `commands/config`
- Preserve CLI behavior:
  - flags
  - command names
  - JSON output shape
  - human-readable output text unless intentionally corrected by tests
  - local validation flow

### Commit

`refactor(cli): split command and output modules`

## Phase 12: Ops Module Cleanup

### Goal

Reduce large ops modules after product-facing refactors are stable.

### Key Changes

- Split Compose production support:
  - env rendering
  - compose command execution
  - service status
  - backup/restore helpers
  - runner cleanup helpers
- Split DGX profile checks:
  - Docker probe
  - GPU probe
  - quota probe
  - path/port probe
  - report rendering
- Split DGX storage helpers:
  - env parsing
  - XFS project quota model
  - slot metadata
  - command execution
  - verification
- Keep command behavior unchanged.

### Commit

`refactor(ops): split deployment and DGX helpers`

## Documentation Updates

- Update architecture docs in both languages:
  - `docs/architecture/en.md`
  - `docs/architecture/zh.md`
- Document the final crate responsibilities:
  - `domain`: semantic values and DTOs only, no SQL/ZIP/Docker.
  - `contracts`: challenge/solution contract parsing and validation.
  - `config`: grouped runtime config and env parsing.
  - `storage`: durable object storage backends.
  - `persistence`: SQL repositories and records.
  - `services`: workflows, state machines, redaction/projection.
  - `runner`: backend-neutral execution orchestration plus Docker backend.
  - `api-server`: HTTP transport.
  - `worker`: process loop, probes, shutdown, service calls.
  - `web`: typed API layer, SWR hooks, presentation components.
  - `cli`: command adapters, local validation, renderers.
  - `ops`: deployment and host-check tools.
- Update docs only for architecture/development workflow because product behavior should not change.

### Commit

`docs(architecture): document modular boundaries`

## Test Plan

### After Phase 1

- `cargo test -p agentics-config`
- targeted config tests:
  - zero numeric values still fail.
  - whitespace-only required strings still fail.
  - optional absolute paths still reject relative paths.
  - CORS/cookie/S3/runner slot validation behavior unchanged.
  - cross-field policies still fail with intended messages.

### After Crate Boundary And Runner Changes

- `cargo test -p agentics-domain`
- `cargo test -p agentics-contracts`
- `cargo test -p agentics-storage`
- `cargo test -p agentics-config`
- `cargo test -p agentics-runner`
- `cargo test -p agentics-services`
- `cargo test -p worker`
- Targeted runner/integration scenarios:
  - separated evaluator validation.
  - piped stdio validation.
  - coexecuted benchmark validation.
  - stale container reconciliation.
  - capacity requeue.
  - successful evaluation finish.
  - failed evaluation finish.

### After Persistence/Services/API Changes

- `cargo test -p agentics-persistence`
- `cargo test -p agentics-services`
- `cargo test -p api-server`
- integration tests with clean DB:
  - `request_validation`
  - `challenge_creation`
  - `public_eval`
  - `public_read`
  - `evaluation_claims`
  - submission creation and validation flows
  - public redaction and leaderboard flows

### After Frontend Changes

- `cd frontends/web && bunx biome check`
- `cd frontends/web && bunx tsc --noEmit`
- `cd frontends/web && bun test`
- Meaningful frontend tests:
  - shared fetch error parsing.
  - admin mutation invalidates expected SWR keys.
  - creator draft refresh after upload/action.
  - split panels render from hook-provided data.
  - no new trivial static-label tests.

### After CLI Changes

- `cargo test -p agentics-cli`
- CLI output tests:
  - JSON output unchanged.
  - key human output snapshots or assertions unchanged.
  - local validation still works.

### Final Verification Before Each Commit Batch

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- package tests for touched crates
- targeted integration tests for touched workflows

### Final Full Verification

- Docker-backed full suite:
  - start compose test stack.
  - run integration suite.
  - run RustFS/S3-backed storage tests.
  - run Docker runner smoke tests.
  - run CUDA smoke if host supports it.
  - stop compose test stack.

## Commit Plan

Use focused commits in this order:

1. `refactor(config): group runtime configuration`
2. `refactor(crates): tighten architecture dependencies`
3. `refactor(runner): decouple backend execution from database state`
4. `refactor(storage): split object storage backends`
5. `refactor(domain): split challenge models by concern`
6. `refactor(persistence): return records instead of DTO projections`
7. `refactor(services): split public projection surfaces`
8. `refactor(services): split draft and submission workflows`
9. `refactor(api): delegate remaining workflows to services`
10. `refactor(web): split console data hooks and panels`
11. `refactor(cli): split command and output modules`
12. `refactor(ops): split deployment and DGX helpers`
13. `docs(architecture): document modular boundaries`

If a phase is too large, split within that phase but keep the same scope, for example:

- `refactor(config): add grouped structs`
- `refactor(config): migrate call sites to grouped config`
- `refactor(config): remove temporary validation views`

## Assumptions

- No public product behavior changes are intended.
- No DB migration is intended.
- No compatibility aliases are needed before MVP.
- Existing tests should remain behavior-preserving; update tests only for Rust module paths or non-public internal API changes.
- Generated frontend schemas should not semantically change. If Rust DTO derives or module exports move, regenerate and verify schema freshness.
- Do not add trivial tests. New tests must cover behavior, boundaries, lifecycle invariants, redaction, runner behavior, or meaningful frontend data flow.
