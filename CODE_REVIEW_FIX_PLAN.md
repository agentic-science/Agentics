# Code Review Fix Plan

This document plans fixes for the full-code-review findings, excluding the
Docker writable-layer quota issue that is tracked separately in
<https://github.com/ifsheldon/Agentics/issues/2>.
Transactional quota admission is also tracked in
<https://github.com/ifsheldon/Agentics/issues/3> so the implementation tradeoffs
remain easy to revisit.

The goal is to convert the review into executable engineering work. Each item
below should map to one focused commit unless the implementation naturally
requires a small paired frontend/backend change.

## Priorities

- P1 items are pre-hosted-MVP blockers.
- P2 items should be fixed before broad external usage, but can follow P1 work.
- Backlog refactors should be scheduled only after correctness and security
  fixes that affect public execution.
- For MVP, a verified GitHub creator identity is the challenge-creation trust
  boundary. The platform should help creators and admins see the PR/commit and
  bundle digest, but exact server-side commit materialization can be deferred to
  post-MVP hardening.

## Implementation Status

| Item | Status | Evidence |
| --- | --- | --- |
| 1. Verified creator GitHub identity | Fixed | `36fc9ba feat: require GitHub OAuth for creator drafts` |
| 2. Reviewer-visible commit and bundle identity | Fixed for MVP, exact server-side commit materialization deferred | `70a643d fix: freeze challenge draft review digest`; post-MVP hardening tracked in [GitHub issue #4](https://github.com/ifsheldon/Agentics/issues/4) |
| 3. Immutable published challenge versions | Fixed | `a1bf7df fix: make published challenge versions immutable` |
| 4. Freeze approved draft inputs | Fixed | `064aa11 fix: freeze approved challenge draft inputs`; `70a643d fix: freeze challenge draft review digest` |
| 5. Transactional quota admission | Fixed | `4757d72 fix: serialize solution submission quota admission`; private asset admission fixed in working tree pending commit; alternatives tracked in [GitHub issue #3](https://github.com/ifsheldon/Agentics/issues/3) |
| 6. Declared output symlink rejection | Fixed | `f96fb89 fix: reject symlinked solution outputs` |
| 7. Leaderboard losing rerun metadata | Fixed | `ce1eeeb fix: keep leaderboard official metadata tied to best run` |
| 8. Relative LocalStorage keys | Fixed | `f498fe7 fix: keep local storage keys relative` |
| 9. Browser admin credential persistence | Fixed | `5aa3e6d fix: use admin session cookies in web console` |
| 10. Official solution submission rendering | Fixed | `77df149 fix: show official submission detail results` |
| 11. CLI package size preflight | Fixed | `f9b27ed fix: preflight CLI solution package limits` |
| 12. CLI validation-run status | Fixed | `1a30799 fix: support validation run status in CLI` |
| 13. Digest-pinned runner image policy | Fixed behind explicit hosted policy flag | `a3d991f fix: enforce optional digest-pinned images` |
| 14. Managed immutable admin bundles | Fixed | `85f60d1 fix: copy admin bundles into managed storage` |
| 15. Globally unique worker instance ids | Fixed | `b223d8e fix: use UUID worker instance identifiers` |
| 16. Runner module split | Fixed | `e54ff7e refactor: split runner support modules` |
| 17. Web DTO contract fixtures | Fixed | `f1b0994 test: add web dto contract fixtures` |
| Docker writable-layer quota | Deferred operational hardening; hosted design decided | Tracked in [GitHub issue #2](https://github.com/ifsheldon/Agentics/issues/2) |

## Recommended Fix Order

1. Add GitHub OAuth for challenge creators and remove self-asserted identity.
2. Add separate creator web routes for draft creation and private asset upload.
3. Make challenge publishing immutable and freeze approved draft inputs.
4. Make quota admission transactional with explicit DB lock rows.
5. Harden output and storage path handling.
6. Fix leaderboard result consistency.
7. Fix user-visible frontend and CLI correctness gaps.
8. Reduce architecture debt that will make the next review/fix cycle expensive.

## P1 Fixes

### 1. Do not let agents self-assert GitHub identity

**Problem**

An authenticated agent can currently link arbitrary GitHub user metadata, then
create a challenge draft whose PR author matches that self-asserted metadata.
That breaks the intended GitHub ownership boundary.

**Fix design**

- Remove direct agent-controlled GitHub identity linking as an authoritative
  operation.
- Add GitHub OAuth for challenge creators:
  - store GitHub `user_id` as the stable identity,
  - store `login` only as display metadata,
  - use minimal OAuth scopes for identity,
  - issue an Agentics web session after OAuth succeeds.
- Reuse the same session infrastructure planned for admin auth, but keep creator
  and admin authorization checks as separate roles and route guards.
- Add separate creator web routes and pages. Creator pages may share the same
  frontend app as admin pages, but they must not use the admin identity model.
- During draft creation, fetch PR metadata server-side where practical and
  verify:
  - repository owner/name matches the allowed challenge repository policy,
  - PR author GitHub id matches the OAuth-linked creator identity,
  - requested PR head commit SHA matches the PR metadata if GitHub metadata was
    fetched successfully.
- MVP trust boundary: once the creator's GitHub identity is verified, the
  platform can trust the creator-provided challenge bundle inputs. The UI/API
  should still display PR URL, commit SHA, manifest hash, and bundle digest to
  help the creator and reviewer catch mistakes.
- Treat user-submitted GitHub metadata as hints only, never as authority.

**Likely code targets**

- `backend/api-server/src/challenge_creation_handlers.rs`
- `backend/shared/src/db/challenge_creation.rs`
- `backend/shared/src/challenge_creation.rs`
- `frontends/agentics-cli/src/commands.rs`
- `frontends/web/src/app/(creator)/...`
- `frontends/web/src/lib/creatorApi.ts`
- `.agents/skills/challenge-authoring-workflow/SKILL.md`
- `docs/versions/v0.2.5/challenge-creation/en.md`
- `docs/versions/v0.2.5/challenge-creation/zh.md`

**Tests**

- Creating a draft with no verified GitHub identity is rejected.
- Creating a draft with a verified identity that does not match the PR author is
  rejected.
- Creating a draft with a matching verified identity and matching PR head SHA is
  accepted.
- Creator private asset upload succeeds through creator auth and does not
  require admin auth.
- Admin auth cannot be used as a substitute for creator ownership.
- CLI/docs examples must no longer imply that agents can self-assert GitHub
  identity.

**Commit shape**

- Commit 1: backend GitHub OAuth session and verified creator identity model.
- Commit 2: creator draft/private-asset APIs and PR metadata checks.
- Commit 3: creator web pages.
- Commit 4: CLI/docs/skills update.

### 2. Help reviewers see commit and bundle identity

**Problem**

Drafts store `commit_sha`, but validation and publish assemble from a
caller-provided local repository path. A mutable checkout can publish scorer
code, data, or specs that were not in the reviewed PR commit. For MVP, the
accepted trust boundary is the verified GitHub creator identity, not a fully
server-enforced Git tree proof.

**Fix design**

- MVP helper mechanism:
  - require creator GitHub OAuth before draft creation,
  - record PR URL, repository, PR number, claimed head SHA, manifest hash, and
    assembled bundle digest,
  - show those values in creator and admin pages,
  - include the values in validation and publish audit records,
  - warn when a creator-provided checkout/manifest does not match stored draft
    metadata.
- Compute and store a content digest over the assembled bundle, including:
  - public files from the Git commit,
  - private asset overlay metadata and content digests,
  - normalized manifest/spec content.
- Validation should record the bundle digest it validated.
- Approval should freeze the digest that the reviewer accepted.
- Publish should require the current assembled digest to match the approved
  digest.
- Post-MVP hardening:
  - the server materializes the challenge bundle from stored `repo_url +
    commit_sha`,
  - the server uses a controlled checkout directory under managed storage,
  - the server verifies the checkout is clean after cloning/fetching the exact
    commit,
  - arbitrary local path publishing is removed.

**Likely code targets**

- `backend/api-server/src/challenge_creation_handlers.rs`
- `backend/shared/src/challenge_creation.rs`
- `backend/shared/src/challenge_bundle.rs`
- `backend/shared/src/db/challenge_creation.rs`
- `frontends/agentics-cli/src/commands.rs`

**Tests**

- A draft records PR URL, commit SHA, manifest hash, and bundle digest.
- A draft cannot be published if private overlay content differs from the
  approved digest.
- Publish succeeds when the current assembled digest matches the approved
  digest.
- Admin/creator UI displays the commit and digest values used for review.

**Commit shape**

- Commit 1: digest/audit metadata model.
- Commit 2: validation/approval/publish digest enforcement.
- Commit 3: creator/admin UI surfacing for PR, commit, and digest metadata.

### 3. Make published challenge versions immutable

**Problem**

Publishing the same `challenge_id + version` currently updates the existing row.
Historical solution submissions that reference that version can later execute
against different code, data, or specs.

**Fix design**

- Change publish semantics so `challenge_id + version` is immutable after first
  successful publish.
- Replace `ON CONFLICT DO UPDATE` with conflict rejection.
- Enforce `new_challenge` and `new_version` intent:
  - `new_challenge` fails if the challenge id already exists.
  - `new_version` fails if the challenge id does not exist.
  - duplicate version always fails.
- Store immutable storage keys for published bundle artifacts. If a source path
  is local/admin supplied, copy it into managed storage at publish time.

**Likely code targets**

- `backend/shared/src/db/challenges.rs`
- `backend/api-server/src/challenge_creation_handlers.rs`
- `backend/shared/src/storage/mod.rs`
- migrations, if constraints are missing

**Tests**

- Publishing duplicate `challenge_id + version` returns conflict.
- Publishing `new_challenge` for an existing challenge returns conflict.
- Publishing `new_version` for a missing challenge returns not found or conflict,
  depending on the API convention chosen.
- Previously published bundle path/spec remains unchanged after a failed
  duplicate publish.

**Commit shape**

- Single backend commit with migration and regression tests.

### 4. Freeze approved draft inputs

**Problem**

Approved drafts can still accept private asset uploads. The asset set reviewed
at approval can change before publish.

**Fix design**

- Treat approval as a freeze point.
- Store approved manifest hash, private asset list, private asset content
  digests, and assembled bundle digest.
- Reject private asset uploads while a draft is approved.
- If edits are needed after approval, require an explicit unapprove or reopen
  transition that clears approval metadata.
- Use compare-and-swap style state transitions for approve, reject, abandon, and
  publish so stale reviewers or repeated requests cannot overwrite newer state.

**Likely code targets**

- `backend/api-server/src/challenge_creation_handlers.rs`
- `backend/shared/src/db/challenge_creation.rs`
- migrations for approval freeze metadata
- admin web review controls if they expose these transitions

**Tests**

- Upload after approval is rejected.
- Reopen/unapprove allows upload and clears prior approval metadata.
- Publish fails if current draft digest differs from approved digest.
- Concurrent publish/reject or approve/upload races leave exactly one valid
  state transition.

**Commit shape**

- Commit 1: approval freeze schema and backend state machine.
- Commit 2: admin web/CLI/docs updates if command behavior changes.

### 5. Make quota admission transactional

**Problem**

Submission quotas and active queue limits are checked before artifact storage
and job insertion. Concurrent requests can pass the count together and exceed
capacity.

**Fix design**

- Move quota admission into the same database transaction that creates the
  solution submission and evaluation job.
- Use explicit database lock rows:
  - add a small `quota_admission_locks` table keyed by scope string,
  - derive deterministic scopes such as `global:official-active`,
    `agent:{agent_id}:official-daily`, and
    `challenge:{challenge_id}:target:{target_id}:official-active`,
  - insert missing scope rows with `ON CONFLICT DO NOTHING`,
  - lock relevant rows with `SELECT ... FOR UPDATE` in sorted scope order,
  - count current canonical rows and create the admitted record in the same
    transaction.
- Keep PostgreSQL advisory locks, serializable transactions with retry, and
  counter/ledger tables as documented alternatives in GitHub issue #3.
- Apply the same design to private asset upload quotas.
- Keep artifact storage ordering safe:
  - If the artifact must be stored before DB insert, write to a temporary key and
    promote after transactional admission.
  - If the transaction rejects, delete the temporary artifact best-effort and
    record cleanup failures.

**Likely code targets**

- `backend/api-server/src/handlers.rs`
- `backend/api-server/src/challenge_creation_handlers.rs`
- `backend/shared/src/db/solution_submissions.rs`
- `backend/shared/src/db/evaluation_jobs.rs`
- `backend/shared/src/db/challenge_creation.rs`
- migration for `quota_admission_locks`

**Tests**

- Concurrent official submissions over the quota result in exactly the allowed
  count being accepted.
- Concurrent active queue admissions over the limit result in exactly the
  allowed count being queued.
- Rejected submissions do not leave durable artifacts or jobs.
- Private asset upload quota has a concurrent regression test that starts two
  uploads below the per-asset limit but above the combined per-draft limit and
  asserts that exactly one succeeds.

**Commit shape**

- Commit 1: official submission transactional quota admission.
- Commit 2: private asset quota admission.
- Commit 3: cleanup and operational docs.

**Implemented**

- Solution submission quota admission uses explicit lock rows and performs
  quota checks inside the transaction that creates the submission and job.
- Private asset quota admission now uses the same explicit lock-row pattern for
  each draft's private assets, so the byte sum and asset-row insert are
  serialized.
- Concurrent private asset upload regression coverage asserts that exactly one
  of two racing uploads can fit under the combined per-draft byte quota.

### 6. Reject declared output symlinks

**Problem**

Declared output checks use `is_file()`, which follows symlinks. A solution can
create an output symlink that the scorer later resolves inside the scorer
container.

**Fix design**

- Check declared outputs with `symlink_metadata`.
- Reject symlinks explicitly.
- Require regular files for declared output paths.
- Keep path validation centralized with the challenge bundle path rules.
- Consider recursively rejecting symlinks under `/io/output`, not only declared
  files, before mounting solution outputs into the scorer container.

**Likely code targets**

- `backend/shared/src/runner.rs`
- `backend/shared/src/challenge_bundle.rs`
- integration tests under `backend/integration-tests`

**Tests**

- A run script that creates `output/answer.txt` as a symlink fails with a clear
  runner error.
- A normal regular output file still passes.
- If recursive output scanning is added, nested symlink outputs are rejected.

**Commit shape**

- Single backend security fix commit with regression test.

### 7. Fix losing reruns corrupting leaderboard metadata

**Problem**

A worse official rerun can leave `best_solution_submission_id` and
`best_rank_score` pointing at the old best while overwriting
`official_score`/`official_metrics` with the losing run.

**Fix design**

- Make the best-entry upsert return whether the candidate became the current
  best.
- Update official score/metrics only when the candidate became best, or
  constrain the update by `best_solution_submission_id = candidate_id`.
- Preserve a separate per-submission official result if needed, but do not mix it
  into best-entry metadata.

**Likely code targets**

- `backend/shared/src/db/leaderboard.rs`
- `backend/shared/src/db/evaluations.rs`
- integration tests around official reruns

**Tests**

- Submit official result A with better score, then official result B with worse
  score for the same challenge/target. Leaderboard best id, best score,
  official score, and official metrics all remain A.
- Submit a later better result and verify all best fields update together.

**Commit shape**

- Single backend correctness commit with regression tests.

## P2 Fixes

### 8. Make LocalStorage keys opaque and relative

**Problem**

The storage trait says storage paths are relative, but `LocalStorage::put`
returns absolute paths and `resolve` accepts arbitrary strings. Absolute paths
or parent components can escape the configured root on get/delete.

**Fix design**

- Store and return opaque relative storage keys, never absolute local paths.
- Reject absolute paths, root components, parent components, Windows prefixes,
  empty paths, and symlinks where relevant.
- Keep a separate internal method for trusted local filesystem access if a local
  path is genuinely required by the runner.
- Add storage-root containment checks after canonicalization for paths that must
  touch the filesystem.

**Likely code targets**

- `backend/shared/src/storage/mod.rs`
- callers that persist or resolve storage URIs
- integration tests that currently assume absolute paths

**Tests**

- `../escape`, `/tmp/escape`, and platform-prefix paths are rejected.
- Normal generated storage keys can be put, read, and deleted.
- Delete cannot remove a file outside the configured storage root.

**Commit shape**

- Single storage security commit if call-site updates are small. Split if runner
  path handling needs a larger refactor.

### 9. Remove persistent admin credentials from browser storage

**Problem**

The admin console stores reusable Basic auth credentials in `sessionStorage`
when remember mode is enabled.

**Fix design**

- Replace browser-sent Basic auth with server-owned admin sessions:
  - login endpoint verifies admin credentials,
  - server sets HttpOnly, Secure, SameSite cookie,
  - admin API accepts the session cookie,
  - logout clears the cookie.
- Use the same session foundation as creator GitHub OAuth, but keep admin and
  creator authorization separate.
- Add CSRF protection for unsafe session-authenticated admin and creator
  requests.

**Likely code targets**

- `backend/api-server/src/auth` or admin extractor layer
- `backend/api-server/src/router.rs`
- `frontends/web/src/components/admin/AdminConsole.tsx`
- `frontends/web/src/lib/adminApi.ts`
- deployment docs for cookie/security settings

**Tests**

- Admin login succeeds and sets an HttpOnly cookie.
- Admin routes reject missing/invalid session cookie.
- Browser storage no longer contains username/password.
- Logout invalidates the session.

**Commit shape**

- Commit 1: backend admin session API.
- Commit 2: frontend admin auth refactor.
- Commit 3: docs and tests.

### 10. Render official solution submission results correctly

**Problem**

The public solution submission detail page falls back from validation result to
legacy evaluation result, but not to `official_evaluation`. Completed official
submissions can render as empty or `n/a`.

**Fix design**

- Normalize the displayed public evaluation in one frontend helper.
- Prefer `official_evaluation ?? validation_evaluation ?? evaluation`, unless
  the backend DTO is simplified first.
- Ensure private benchmark details remain redacted.

**Likely code targets**

- `frontends/web/src/app/(observer)/solution-submissions/[id]/page.tsx`
- `frontends/web/src/lib/schemas.ts`
- frontend tests

**Tests**

- Route/rendering test for an official-only result.
- Route/rendering test that redacted private details do not appear.

**Commit shape**

- Single frontend correctness commit.

### 11. Bound CLI workspace packaging before ZIP/base64

**Problem**

The CLI reads all files into memory, builds a full ZIP buffer, and base64
encodes the whole archive. API and runner limits are enforced later, which means
large local workspaces fail late and waste memory.

**Fix design**

- Share or mirror protocol limits in the CLI:
  - maximum file count,
  - maximum individual file size,
  - maximum uncompressed archive size,
  - maximum compressed ZIP size before base64.
- Preflight file metadata before reading file contents.
- Stream files into the ZIP writer with bounded buffers.
- Avoid following symlinks and respect `.gitignore` as currently intended.
- Produce actionable CLI errors before upload.

**Likely code targets**

- `frontends/agentics-cli/src/package.rs`
- `frontends/agentics-cli/src/config.rs`
- `backend/shared/src/zip_project.rs`, if limits should become shared constants

**Tests**

- Oversized file is rejected before archive creation.
- Too many files are rejected.
- `.gitignore` is respected.
- Symlinked files are rejected or skipped according to the chosen policy.

**Commit shape**

- Single CLI packaging commit with tests.

### 12. Make CLI status work for validation runs

**Problem**

`agentics status` always fetches a solution submission, but `validate --no-wait`
returns validation run ids and the client already has a validation-run fetcher.

**Fix design**

- Add an explicit command shape, for example:
  - `agentics status solution-submission <id>`
  - `agentics status validation-run <id>`
- Remove or deprecate ambiguous `agentics status <id>` before MVP. Explicit
  subcommands are easier to reason about and document.
- Reuse `render_validation_run_status`.

**Likely code targets**

- `frontends/agentics-cli/src/lib.rs`
- `frontends/agentics-cli/src/commands.rs`
- `frontends/agentics-cli/src/api.rs`
- `.agents/skills/agentics-cli-workflow/SKILL.md`
- hosted CLI onboarding docs in both languages

**Tests**

- `status validation-run <id>` renders validation status.
- `status solution-submission <id>` renders official submission status.
- Old ambiguous command behavior is removed or errors with migration guidance.

**Commit shape**

- Single CLI behavior/docs commit.

## Backlog Refactors and Hardening

### 13. Require digest-pinned runner images before public hosted execution

**Why**

Tag-only images can change under the same name. That makes results less
auditable and can change the benchmark environment without a challenge version
change.

**Plan**

- Require image references with digests for hosted official execution.
- Allow tag-only images in local/dev mode if explicitly configured.
- Validate image references during challenge bundle validation.
- Document challenge-owner responsibility for image provenance.

### 14. Copy admin-published bundles into managed immutable storage

**Why**

Published versions should not depend on mutable local filesystem paths.

**Plan**

- During publish, copy bundle and statement artifacts to managed storage under a
  content-addressed or version-addressed key.
- Persist only storage keys.
- Use storage keys during worker execution.

### 15. Give workers globally unique instance IDs

**Why**

PID-only worker identities are ambiguous across hosts and restarts. Job leases
and stale-actor protections need a stable worker instance identity.

**Plan**

- Generate a worker instance UUID at process startup.
- Include hostname or deployment id for observability only, not as authority.
- Use the instance UUID in heartbeats and job claims.
- Preserve attempt number or claim token in completion writes.

### 16. Split `backend/shared/src/runner.rs`

**Why**

The runner is above the repo's 1200-line refactor threshold and mixes Docker
container execution, phase orchestration, filesystem validation, log handling,
and scorer result normalization.

**Plan**

- Split into modules with clear ownership:
  - `runner/orchestrator.rs`
  - `runner/docker.rs`
  - `runner/filesystem.rs`
  - `runner/logs.rs`
  - `runner/scorer.rs`
  - `runner/errors.rs`
- Move tests with the code they exercise.
- Do this after security fixes unless a fix becomes easier with a local split.

**Implemented**

- Split Docker execution, filesystem/archive checks, log helpers, and phase
  error helpers into `backend/shared/src/runner/` submodules.
- Kept `runner.rs` focused on orchestration and reduced it below the 1200-line
  refactor threshold.

### 17. Replace manually mirrored web schemas

**Why**

Backend DTOs and frontend Zod schemas can drift. Review already found one DTO
usage mismatch around official results.

**Plan**

- Short term: add serializer-backed contract tests with JSON fixtures generated
  from Rust DTOs and consumed by frontend schema tests.
- Longer term: generate TypeScript types and validation schemas from the
  backend contract, or define OpenAPI and generate clients.

**Implemented**

- Added Rust DTO serialization contract tests for challenge detail, official
  solution submission detail, and admin capacity responses.
- Reused the same JSON fixtures in frontend Zod schema tests so DTO drift is
  caught on both sides before hosted MVP work.
- Added Rust `schemars` JSON Schema export for API DTOs and a frontend
  `bun run generate:schemas` pipeline that converts those schemas into generated
  Zod validators and TypeScript types. Frontend API clients now import generated
  schemas from `frontends/web/src/lib/generated/schemas.ts` through the local
  schema barrel.

## Resolved Decisions

These product and engineering choices are now fixed for the implementation plan:

1. Challenge creators use GitHub OAuth for identity verification.
2. Creator draft and private-asset upload pages are separate from admin pages and
   do not use admin credentials.
3. Admin auth uses server-owned HttpOnly cookie sessions rather than persistent
   browser-stored Basic credentials.
4. Quota admission uses explicit database lock rows for MVP. Alternatives and
   tradeoffs are tracked in GitHub issue #3.
5. CLI status uses explicit subcommands instead of ambiguous ID detection.
6. Exact server-side Git commit materialization is post-MVP hardening. For MVP,
   verified creator identity plus visible PR/commit/digest metadata is the trust
   boundary.
7. Hosted runner disk isolation should use an Agentics-owned Docker daemon whose
   data root lives on a loopback XFS image mounted with project quotas. Docker
   writable-layer quotas should be enforced through `storage_opt.size` and
   verified by a startup probe. Separately, solution setup, build, and run
   phases, plus scorer prepare and score phases, should receive their writable
   paths through per-phase loopback filesystem images so bind-mounted scratch
   paths have hard limits as well.
8. Strict host capability checks should use an explicit Agentics flag such as
   `AGENTICS_HOST_PROBE_MODE=off|warn|require`, not the generic `CI` variable.
   Mac-local development defaults to `off`; hosted DGX/staging uses `require`.
