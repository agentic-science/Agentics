# Full Code Review Log

Review date: 2026-05-19
Reviewed HEAD: `ebf4c9a`

Scope: backend API, worker and Docker runner, challenge draft lifecycle, CLI, frontend, schemas, tests, and documentation alignment.

Accepted MVP risks that are intentionally out of scope for this review:

- GitHub PR ownership is client-asserted.
- Pioneer codes are stored and re-exposed as live secrets.

Resolution status as of current `main`:

- No actionable review finding in this document remains open.
- Runner resource safety now has byte, inode, file-count, directory-count,
  depth, run-count, `result.json`, `public_results`, embedded result-log, and
  persisted runner-log limits. New runner and DGX quota environment variables
  are documented in the operator, DGX, solution protocol, and ports/paths docs.
- Runner lifecycle/security findings are resolved: permission-repair containers
  share the hardened Docker policy, hosted probe enforcement is wired, worker
  lease refresh is attempt-scoped, hosted Docker reconciliation ignores local
  validation containers, and archived public read surfaces are consistent.
- Challenge draft/private asset/publish findings are resolved: publish claims
  carry claim identity, failed publish cleanup is claim-scoped, GitHub PR fields
  are cross-validated, private assets have observable `pending|active|failed`
  lifecycle state, stale pending retries repair unreferenced storage, upload and
  manifest `required` fields are explicit, and publish enforces required assets.
- Public API, CLI, web schema, PRD, and test hygiene findings are resolved:
  authenticated agent routes are under `/api/agent`, `submissions show` uses the
  public endpoint, auth request schemas are generated and consumed, inactive
  admin/creator language toggles were removed, the obsolete `expired` draft
  state text was removed, upload-side ZIP symlink validation is shared, Linux
  quota tests fail without bounded test storage, CLI report projection uses the
  shared evaluation selector, low-value tests were removed or folded into useful
  coverage, and the oversized runner/challenge/public-eval files were split.

## Findings

### P1: Runner writable outputs are quota-limited by bytes only

Diagnosis: runner writable storage has byte quota coverage, but there is no inode, file-count, or directory-depth guard. A participant can create many tiny files or deep trees and exhaust host filesystem metadata or make cleanup/path traversal expensive even when byte quota is respected.

References:

- `backend/shared/src/runner/filesystem.rs:31`
- `backend/shared/src/runner/filesystem.rs:163`
- `backend/shared/src/runner.rs:1125`
- `scripts/ops/prepare-dgx-spark-storage.sh:109`

Recommended fix: add explicit per-run file count and directory depth limits in runner-owned output validation. Enforce the limits both during hostile archive extraction and after container execution before collecting artifacts. Keep byte quota, inode/file-count, depth, log, and container layer limits as separate controls.

### P1: Container-produced JSON and logs can still be too large in aggregate

Diagnosis: individual log tails are bounded, but runner-produced metadata and result/log collection paths still read or retain container-produced data without an aggregate run-count and artifact-size budget. `result.json` is read as a whole, logs can accumulate up to per-run caps across many runs, and artifact reads load the full log before truncation.

References:

- `backend/shared/src/challenge_bundle.rs:48`
- `backend/shared/src/challenge_bundle.rs:403`
- `backend/shared/src/runner.rs:383`
- `backend/shared/src/runner.rs:663`
- `backend/api-server/src/handlers/artifacts.rs:81`

Recommended fix: add platform-owned maximums for run count, `result.json` bytes, aggregate stored log bytes per evaluation, result-embedded logs, public result entries, and artifact read streaming/truncation. These must not be participant-controlled submission config.

MVP default numbers:

- Per-container captured stdout/stderr: `1048576` bytes. Keep the current `1 MiB` per-container Docker log cap.
- Run invocations per evaluation: at most `12`.
- Aggregate persisted runner log bytes per evaluation: `run_count * 1048576` bytes, capped by the validated run count. With the MVP run-count limit this is at most `12 MiB`. Official runs still persist redaction notices rather than raw private benchmark logs.
- `result.json` raw bytes before parse: `4194304` bytes.
- `public_results` entries in `result.json`: at most `1024`.
- Scorer `logs` embedded in `result.json`: at most `262144` total UTF-8 bytes. Scorers should use stdout/stderr for logs; `result.logs` is a compact structured diagnostic channel.

Implementation notes: check `result.json` size before `read_to_string`; replace the current unbounded `String` runner-log accumulator with a bounded accumulator that appends a truncation notice; validate static and prepare-generated run manifest lengths; validate `public_results.len()` and total `result.logs` bytes during scorer result validation; keep the existing owner log API response cap as a separate transport limit.

### P2: Permission-repair containers use weaker containment than runner containers

Diagnosis: permission-repair sidecars are platform-created helper containers, but their Docker options do not mirror the runner sandbox. Main runner containers apply stricter containment than the permission repair path, which means the cleanup path has a larger host interaction surface than the workload path it supports.

References:

- `backend/shared/src/runner/docker.rs:88`
- `backend/shared/src/runner/docker.rs:200`

Recommended fix: share a Docker security option builder between runner containers and permission-repair containers. Apply the same `network=none`, `no-new-privileges`, capability drops, read-only root where feasible, user mapping, and privileged=false policy, then add a focused test for the generated Docker options.

### P2: Hosted runner safety probe is documented but not enforced

Diagnosis: hosted DGX configuration documents `AGENTICS_HOST_PROBE_MODE=require`, but worker configuration does not parse or enforce it. A hosted worker can start without proving the expected Docker/storage/GPU safety profile is active.

References:

- `deploy/dgx-spark/agentics.env.example:12`
- `backend/shared/src/config.rs:490`

Recommended fix: add a typed worker config field for host probe mode, implement startup enforcement, and fail fast when a hosted worker is configured to require probes but the probes fail or are skipped.

### P2: Archived challenges lose some direct public read surfaces

Diagnosis: public challenge detail intentionally supports direct reads of archived challenges, but leaderboard, ranking, and score-distribution paths re-fetch the challenge through an active-only published-challenge query. This makes archived public pages inconsistent with the documented archival behavior.

References:

- `docs/PRD/en.md:251`
- `docs/contribute-challenges/en.md:206`
- `docs/review-challenges/en.md:116`
- `backend/shared/src/db/challenges.rs:766`
- `backend/shared/src/db/challenges.rs:790`
- `backend/shared/src/db/leaderboard.rs:248`
- `backend/integration-tests/tests/challenge_creation.rs:776`

Recommended fix: decide whether archived public direct reads include leaderboard/ranking distributions. If yes, use one archived-inclusive public challenge resolver across all direct public surfaces and add integration coverage. If no, update the docs and UI to make archived detail-only access explicit.

### P2: Worker lease refresh is not attempt-scoped

Diagnosis: lease refresh updates are keyed too broadly. If an old worker attempt refreshes a job after another attempt has claimed it, the stale worker can keep the lifecycle alive incorrectly.

References:

- `backend/shared/src/db/evaluation_jobs.rs:104`
- `backend/worker/src/cycle.rs:226`

Recommended fix: make lease refresh compare the active worker claim identity and attempt count in the same update. Treat zero affected rows as a lost claim and stop work without writing results.

### P2: Private asset uploads do not refresh draft activity

Diagnosis: pending private asset rows can be reserved and storage can be written without updating the parent draft activity timestamp unless a special already-validated path is taken. Draft stale cleanup is based on draft `updated_at`, so a live upload can be misclassified as abandoned.

References:

- `backend/shared/src/db/challenge_creation.rs:429`
- `backend/shared/src/db/challenge_creation.rs:461`
- `backend/shared/src/db/challenge_creation.rs:873`

Recommended fix: update the draft timestamp transactionally when reserving, activating, failing, or cleaning up a private asset. Add a test where an upload prevents stale draft cleanup from abandoning the draft.

### P2: Publish claims have no claim identity

Diagnosis: draft publishing uses a guarded status transition, but follow-up reset, fail, and complete operations match only on draft ID and status. A stale publisher can complete or fail a newer publish attempt.

References:

- `backend/shared/src/db/challenge_creation.rs:971`
- `backend/shared/src/db/challenge_creation.rs:1000`
- `backend/shared/src/db/challenge_creation.rs:1024`
- `backend/shared/src/db/challenge_creation.rs:1179`

Recommended fix: add a publish claim ID or attempt token when moving `approved -> publishing`. Require that token for publish completion, publish failure, and stale reset. Store it in DB and include it in tests for racing publishers.

### P2: DB publish failure can leave private runtime bundle storage orphaned

Diagnosis: the final runtime bundle path is promoted before the database publish operation. If DB publish fails after the filesystem promotion succeeds, the error cleanup removes only the temporary bundle path, leaving an orphaned final private bundle.

References:

- `backend/api-server/src/challenge_creation_handlers.rs:714`
- `backend/api-server/src/challenge_creation_handlers.rs:724`
- `backend/api-server/src/challenge_creation_handlers.rs:744`

Recommended fix: either move final promotion after the DB state transition with a recoverable state, or make the failure path remove the promoted final bundle only when this publish claim created it. Prefer a claim-owned temp-to-final protocol with idempotent repair.

### P2: Pull request provenance fields are not cross-validated

Diagnosis: challenge draft requests carry repository URL, PR URL, and PR number, but the API validates them independently. A request can combine a valid repo with a PR URL or number from a different repo.

References:

- `backend/shared/src/models/challenge_creation.rs:149`
- `backend/api-server/src/challenge_creation_handlers.rs:54`
- `backend/shared/src/models/urls.rs:497`

Recommended fix: add a typed `GithubPullRequestRef` or method on the existing PR URL wrapper that verifies repo owner/name and number match the declared repository. This does not need to solve the accepted MVP risk of proving GitHub account ownership.

### P2: Stale pending private asset retry can be blocked by an existing object

Diagnosis: stale pending DB rows can be marked failed before retry, but the storage promotion path uses a deterministic final key and refuses an existing destination. If a previous crash promoted bytes but failed before DB activation, an exact retry can keep failing on the existing object.

References:

- `backend/api-server/src/challenge_creation_handlers.rs:213`
- `backend/api-server/src/challenge_creation_handlers.rs:222`
- `backend/shared/src/db/challenge_creation.rs:648`
- `backend/shared/src/storage/mod.rs:199`

Recommended fix: make stale pending repair reconcile both DB and storage. Either remove the stale final object when it is unreferenced by an active row, or use claim-specific temporary keys and promote only after DB state can safely claim ownership.

### P2: Failed and pending private asset lifecycle is not observable to reviewers

Diagnosis: failed private asset rows are recorded internally, but normal draft responses list only active assets. Review workflow text expects reviewers to inspect failed/private asset state, but the API does not expose enough state to diagnose repairable upload failures.

References:

- `backend/shared/src/db/challenge_creation.rs:510`
- `backend/shared/src/db/challenge_creation.rs:1264`
- `backend/shared/src/models/challenge_creation.rs:230`
- `.agents/skills/challenge-review-workflow/SKILL.md:57`

Recommended fix: expose an admin/reviewer-only private asset state list with status, timestamps, failure message, and size/hash metadata. Keep participant/public draft responses active-only.

### P2: Private asset `required` is user-facing but not enforced

Diagnosis: private asset upload requests include a `required` flag and the web form exposes it, but publish/runtime behavior does not appear to enforce required private assets as a contract.

References:

- `backend/shared/src/models/challenge_creation.rs:300`
- `backend/api-server/src/challenge_creation_handlers.rs:185`
- `frontends/web/src/components/creator/CreatorConsole.tsx:289`

Recommended fix: either remove `required` before MVP or enforce it at publish validation and runtime bundle assembly. The current product direction says no defaults for `private_assets[].required`, so if kept it must be required and meaningful.

### P2: Worker Docker reconciliation can remove local validation containers

Diagnosis: Docker reconciliation removes Agentics-labeled containers with malformed DB claim labels. CLI local validation uses Agentics runner labels and non-UUID local job IDs, so a server worker on the same Docker host can classify a local validation container as invalid and kill it.

References:

- `backend/shared/src/runner/docker.rs:302`
- `backend/shared/src/runner/docker.rs:386`
- `frontends/agentics-cli/src/commands.rs:679`

Recommended fix: separate hosted worker labels from local validation labels, or make reconciliation target only containers with a hosted-worker label namespace and valid worker claim fields. Add a Docker option unit test for both label sets.

### P2: CLI `submissions show` uses an owner-only endpoint for public IDs

Diagnosis: README public observer flow suggests users can inspect public submissions by ID, but CLI `submissions show` calls an owner-authenticated endpoint. The web observer page correctly uses the public route.

References:

- `README.md:115`
- `frontends/agentics-cli/src/cli.rs:455`
- `frontends/agentics-cli/src/lib.rs:159`
- `frontends/agentics-cli/src/api.rs:113`
- `frontends/web/src/app/(observer)/solution-submissions/[id]/page.tsx:41`

Current public endpoint:

- `GET /api/public/solution-submissions/{id}`
- Handler/client naming should stay explicit: `get_public_solution_submission`.

Current Bearer-authenticated agent routes are not consistently scoped under `/api/agent`:

- `GET /api/challenges`
- `GET /api/challenges/{name}`
- `POST /api/solution-submissions`
- `GET /api/solution-submissions/{id}`
- `GET /api/solution-submissions/{id}/result-report`
- `GET /api/solution-submissions/{id}/ranking-context`
- `GET /api/solution-submissions/{id}/logs`
- `POST /api/validation-runs`
- `GET /api/validation-runs/{id}`

Recommended fix: move the Bearer-authenticated agent API to `/api/agent/...` before MVP, without compatibility aliases. Use `/api/agent/challenges`, `/api/agent/solution-submissions`, and `/api/agent/validation-runs`. Keep creator-owned challenge APIs under `/api/creator/...`, because those use GitHub OAuth creator sessions and represent challenge creator/owner workflow rather than submitting-agent workflow. Make `agentics submissions show <solution-submission-id>` use the public endpoint by default and work without authentication. Add an explicit submitting-agent command, recommended name `agentics submissions status <solution-submission-id>`, for the authenticated `/api/agent/solution-submissions/{id}` lifecycle/status view. Keep `submissions wait` and `submissions logs` submitting-agent authenticated, because they operate on private lifecycle/log surfaces. This keeps the simple observer command name attached to the public API while preserving an obvious command for submitter-private state.

### P2: Web auth request DTO schemas are missing from generated schema facade

Diagnosis: Rust auth request DTOs exist, but generated web schemas/facade do not include them. Web clients therefore hand-write request body shapes in API helpers instead of using generated schemas.

References:

- `backend/shared/src/models/auth.rs:10`
- `backend/shared/src/models/auth.rs:27`
- `backend/shared/src/models/auth.rs:41`
- `backend/shared/src/bin/export_web_schemas.rs:6`
- `backend/shared/src/bin/export_web_schemas.rs:76`
- `frontends/web/scripts/generate-api-schemas.mjs:20`
- `frontends/web/src/lib/schemas.ts:1`
- `frontends/web/src/lib/adminApi.ts:10`
- `frontends/web/src/lib/adminApi.ts:60`
- `frontends/web/src/lib/creatorApi.ts:113`

Recommended fix: add auth request DTOs to the Rust schema export manifest, regenerate frontend schemas, and route web API request validation through the stable schema facade.

### P2: Admin and creator surfaces expose a language switcher but remain English-only

Diagnosis: admin and creator layouts include locale controls, but the pages and workflow components contain hardcoded English strings. This creates a misleading UI state where changing language appears supported but does not localize the page.

References:

- `frontends/web/src/app/(admin)/layout.tsx:32`
- `frontends/web/src/app/(creator)/layout.tsx:38`
- `frontends/web/src/components/admin/AdminConsole.tsx:273`
- `frontends/web/src/components/admin/ChallengeDraftReviewPanel.tsx:136`
- `frontends/web/src/components/creator/CreatorConsole.tsx:408`
- `frontends/web/src/components/creator/CreatorOAuthCallback.tsx:21`

Recommended fix: either wire these surfaces to the translation dictionaries or remove the switcher from admin/creator until localization is complete.

### P2: PRD still specifies an unimplemented `expired` draft state

Diagnosis: the PRD lists an `expired` challenge draft state, but DB constraints and cleanup code use the implemented abandoned/repair states instead.

References:

- `docs/PRD/en.md:257`
- `docs/PRD/zh.md:258`
- `backend/migrations/012_challenge_draft_repair_states.sql:4`
- `backend/shared/src/db/challenge_creation.rs:873`

Current implemented draft statuses are:

- `draft`
- `validated`
- `approved`
- `publishing`
- `rejected`
- `published`
- `abandoned`

`rejected` and `abandoned` should remain distinct. `rejected` means an admin/reviewer made an explicit review decision that the submitted draft should not publish as-is, with reviewer feedback preserved. `abandoned` means the workflow is no longer active for lifecycle reasons, such as closed unmerged PR, creator withdrawal, or stale inactive unpublished draft. Both may become eligible for unpublished private asset purge after the configured grace period, but stale cleanup should not rewrite `rejected` drafts to `abandoned` because that loses the review outcome.

Recommended fix: remove `expired` from both PRDs and document the implemented state machine. Do not add an `expired` status. Do not add `closed` for MVP. Transition closed unmerged PRs, creator withdrawals, and stale active unpublished drafts to `abandoned`, with the reason captured in the audit event and validation/status message. Update stale cleanup so it only marks active unpublished statuses as `abandoned`, not `rejected`.

### P2: Digest-pinned hosted image policy remains opt-in

Diagnosis: challenge publish enforcement for digest-pinned hosted images is gated by configuration. If the flag is off in a hosted profile, new challenges can publish mutable image tags even though the docs and target policy expect stronger image immutability.

References:

- `backend/shared/src/config.rs:97`
- `backend/api-server/src/challenge_creation_handlers.rs:711`
- `backend/shared/src/challenge_bundle/images.rs:244`

Recommended fix: make digest-pinned image enforcement mandatory for hosted publish, or split local-dev and hosted profiles into explicit typed modes where hosted mode cannot disable this policy.

### P3: Solution ZIP symlinks are rejected only after queueing

Diagnosis: API upload validation rejects unsafe paths and duplicate paths, but symlink-mode entries are caught later by worker extraction. This wastes queue capacity and gives submitters later feedback than necessary.

References:

- `backend/shared/src/zip_project.rs:22`
- `backend/api-server/src/handlers.rs:273`
- `backend/shared/src/runner/filesystem.rs:103`

Recommended fix: move symlink-mode rejection into the shared ZIP envelope validator used by upload and extraction. Keep worker-side validation as defense in depth.

### P3: Some environment-sensitive tests can pass without exercising their invariant

Diagnosis: quota-related integration tests can pass or skip the meaningful path when local environment variables are absent or when the configured test storage is unbounded.

References:

- `backend/integration-tests/tests/public_eval.rs:44`
- `backend/integration-tests/tests/public_eval.rs:723`
- `backend/integration-tests/tests/challenge_creation.rs:993`
- `backend/integration-tests/tests/challenge_creation.rs:1030`
- `backend/api-server/src/challenge_creation_handlers.rs:152`

Recommended fix: keep the separate test quota root, but make Linux quota-sensitive tests fail when required quota environment variables are absent, malformed, or point to unbounded storage. Non-Linux platforms may skip these Linux-only quota tests. On Linux, the failure should name the missing variables and point to `scripts/ops/prepare-dgx-spark-test-storage.sh`, because a passing Linux run must prove the quota invariant rather than only print a warning. Adjust private asset quota tests so they exercise DB quota reservation rather than only request-size validation.

### P3: CLI result report rank score omits validation evaluation

Diagnosis: CLI status output and web helpers include validation evaluation where appropriate, but CLI report output computes rank score from official evaluation only. This creates inconsistent display during validation workflows.

References:

- `frontends/web/src/lib/submissionEvaluation.ts:9`
- `frontends/agentics-cli/src/output.rs:368`
- `frontends/agentics-cli/src/output.rs:698`
- `frontends/agentics-cli/src/output.rs:705`

Recommended fix: centralize CLI result projection so report and status use the same result-of-record selection rules.

### P3: One public empty-list integration test is low value

Diagnosis: one public empty-list integration test only verifies an empty response shape and does not protect meaningful behavior beyond framework serialization and fixture setup.

References:

- `backend/integration-tests/tests/public_read.rs`

Recommended fix: delete it or fold it into a broader public read behavior test that exercises visibility, pagination, and archived/active state.

### P3: Trivial frontend badge test should be removed or made behavioral

Diagnosis: `EvaluationModeBadges.test.tsx` checks static rendering labels without exercising state behavior or a regression-prone contract.

References:

- `frontends/web/src/components/submissions/EvaluationModeBadges.test.tsx:7`

Recommended fix: remove it, or replace it with a behavior test that proves public/private evaluation visibility or mode-specific rendering that has previously regressed.

### P3: Several source files exceed the project size threshold

Diagnosis: the project asks agents to watch for files over roughly 1200 lines. A few files are now past that line and likely carrying multiple responsibilities.

References:

- `backend/shared/src/runner.rs`
- `backend/shared/src/db/challenge_creation.rs`
- `backend/integration-tests/tests/public_eval.rs`

Recommended fix: split runner execution, artifact collection, and Docker/filesystem concerns; split challenge creation DB functions by draft/assets/publish lifecycle; split public evaluation integration tests by workflow.

## Validation Logic To Centralize

### Archive envelope validation

Current duplication:

- `backend/shared/src/zip_project.rs`
- `backend/shared/src/runner/filesystem.rs`
- `backend/api-server/src/challenge_creation_handlers.rs`
- `frontends/agentics-cli/src/package.rs`

Recommendation: create a shared `ArchiveEnvelopePolicy` and normalized archive entry validator in `backend/shared`. Use it for solution upload validation, runner extraction, private asset upload, and CLI packaging. It should cover unsafe paths, duplicate normalized paths, symlink entries, entry count, expanded bytes, per-file bytes where needed, and create-new semantics for extraction.

### Target selection and target policy

Current duplication:

- `frontends/agentics-cli/src/commands.rs`
- `backend/api-server/src/handlers.rs`
- `backend/shared/src/challenge_bundle/images.rs`

Recommendation: move target selection, supported-target checks, and challenge target policy into shared typed helpers. The CLI and backend should both parse raw target strings at the boundary and then pass `TargetName` or a stricter supported-target type inward.

### Public result projection and visibility

Current duplication:

- `backend/api-server/src/handlers.rs`
- `frontends/web/src/lib/challengeVisibility.ts`
- `frontends/web/src/lib/submissionEvaluation.ts`
- `frontends/agentics-cli/src/output.rs`

Recommendation: keep backend as the source of truth for public result-of-record and visibility projection. Frontend and CLI should render explicit backend-provided booleans/projections rather than re-deciding which evaluation or metric is visible.

### Public pagination and list limits

Current duplication:

- `backend/api-server/src/handlers.rs`
- `frontends/agentics-cli/src/cli.rs`
- `frontends/agentics-cli/src/commands.rs`
- `frontends/web/src/app/(observer)/page.tsx`

Recommendation: introduce a typed public limit contract with a documented default and maximum. Use it in API query parsing, CLI validation, and web fetch helpers.

### GitHub pull request references

Current duplication:

- `backend/shared/src/models/urls.rs`
- `backend/shared/src/models/challenge_creation.rs`
- `backend/api-server/src/challenge_creation_handlers.rs`

Recommendation: represent repository URL, PR URL, and PR number as one validated PR reference before persistence. This should verify shape and cross-field consistency, while leaving actual GitHub ownership proof as the accepted MVP risk.

### Web request schemas

Current duplication:

- `backend/shared/src/models/auth.rs`
- `backend/shared/src/bin/export_web_schemas.rs`
- `frontends/web/src/lib/adminApi.ts`
- `frontends/web/src/lib/creatorApi.ts`

Recommendation: generate auth request schemas from Rust DTOs and validate web request bodies through `frontends/web/src/lib/schemas.ts` instead of hand-written object shapes.

## Suggested Fix Order

1. Runner output/resource safety: inode/file-count/depth limits, aggregate result/log limits, permission-repair containment, hosted safety probe enforcement.
2. State-machine correctness: attempt-scoped lease refresh, publish claim identity, private asset activity timestamps, stale pending storage reconciliation.
3. Public/API contract cleanup: archived public surface consistency, CLI public submission show, web auth schemas, admin/creator localization switcher behavior.
4. Challenge creation cleanup: PR provenance cross-validation, private asset required enforcement or removal, failed/pending asset reviewer visibility.
5. Validation centralization: archive policy first, then target policy, then public projection and pagination helpers.
6. Test and docs hygiene: quota test warnings, low-value test deletion or replacement, file-size splits, PRD state cleanup.
