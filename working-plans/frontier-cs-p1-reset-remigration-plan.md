# P1 Frontier-CS Reset And Remigration Plan

Status: Active source of truth
Date: 2026-05-28

## Summary

Reset and remigrate only the 35 confirmed P1 faithfulness breaks from
`challenge-repos/agentics-challenges/migrations/frontier-cs-faithfulness-qa-issues.md`.
P2 issues stay out of this wave.

The reset is a rollback first, then a fresh migration. The lead agent owns
service shutdown, destructive cleanup, GitHub issue/project state,
publish/admin actions, private-bundle deletion, and final QA. Subagents prepare
remigration branches, private bundles, and honest test solutions.

## Reset Scope

Use the P1 list from the QA log as the reset candidate set:

`3,4,10,13,14,16,17,25,28,30,36,40,52,53,54,60,63,68,69,73,77,79,81,82,85,86,89,93,101,104,106,107`,
plus:
`llm-router-frontier-cs-llm-router`,
`mamba2-scan-frontier-cs-mamba2-scan`,
`sql-parser-coverage-frontier-cs-grammar-fuzzing-seed-sql`.

Before deleting each candidate, do a quick current-state recheck. If a challenge
has already been faithfully remigrated after the QA log, do not delete it; mark
it `already fixed` in the reset ledger and update the issue/tracker instead.

## Candidate Work Packages

- Worker A: `lamp-ring-permutation-frontier-cs-algorithmic-3`,
  `matrix-kth-smallest-frontier-cs-algorithmic-4`,
  `weighted-tree-distances-frontier-cs-algorithmic-10`,
  `grid-robot-trap-frontier-cs-algorithmic-13`,
  `hidden-cycle-length-frontier-cs-algorithmic-14`
- Worker B: `cycle-chord-identification-frontier-cs-algorithmic-16`,
  `maximum-position-permutation-frontier-cs-algorithmic-17`,
  `graph-connectivity-oracle-frontier-cs-algorithmic-25`,
  `diverc-autofill-words-frontier-cs-algorithmic-28`,
  `moving-mole-tree-frontier-cs-algorithmic-30`
- Worker C: `modulo-collision-size-frontier-cs-algorithmic-36`,
  `bracket-sequence-recovery-frontier-cs-algorithmic-40`,
  `permutation-segment-geemu-frontier-cs-algorithmic-52`,
  `inter-active-permutation-frontier-cs-algorithmic-53`,
  `tree-centroid-guess-frontier-cs-algorithmic-54`
- Worker D: `disk-probing-frontier-cs-algorithmic-60`,
  `space-thief-stars-frontier-cs-algorithmic-63`,
  `ink-pen-selection-frontier-cs-algorithmic-68`,
  `magic-word-spells-frontier-cs-algorithmic-69`,
  `inversion-recovery-frontier-cs-algorithmic-73`
- Worker E: `improv-rating-wagers-frontier-cs-algorithmic-77`,
  `modpow-timing-key-frontier-cs-algorithmic-79`,
  `binary-slate-machine-frontier-cs-algorithmic-81`,
  `bitwise-or-permutation-frontier-cs-algorithmic-82`,
  `scp-maze-exit-frontier-cs-algorithmic-85`
- Worker F: `hidden-tree-median-frontier-cs-algorithmic-86`,
  `steiner-tree-reconstruction-frontier-cs-algorithmic-89`,
  `greedy-tree-blackbox-frontier-cs-algorithmic-93`,
  `hidden-circuit-gates-frontier-cs-algorithmic-101`,
  `dishonest-attendance-frontier-cs-algorithmic-104`
- Worker G: `hidden-bipartite-graph-frontier-cs-algorithmic-106`,
  `divisor-count-gcd-frontier-cs-algorithmic-107`,
  `llm-router-frontier-cs-llm-router`,
  `mamba2-scan-frontier-cs-mamba2-scan`,
  `sql-parser-coverage-frontier-cs-grammar-fuzzing-seed-sql`

## Implementation Steps

1. Create a reset ledger with challenge handle, Frontier-CS source path, issue
   number, old execution mode, intended execution mode, public dir, test
   solution dir, private backup prefix, production object prefix, and reset
   status.
2. Resolve per-challenge GitHub issues by title or handle and record Project #1
   item IDs.
3. Stop production services with `just compose-prod-down --runner clean`, then
   stop the dedicated production runner daemon if running with
   `sudo just compose-prod-runner-docker-down`.
4. Reset disposable production-rehearsal data by removing only the production
   Compose `postgres_data` and `rustfs_data` volumes for the resolved project,
   default `agentics-prod`.
5. Keep persistent private-bundle backup RustFS available until targeted P1
   bundle deletion is complete.
6. In `agentics-challenges`, delete only reset candidates:
   `challenges/<handle>/` and `test-solutions/<handle>/`. Do not delete QA logs
   under `migrations/`.
7. Delete backup RustFS keys under `private-bundle-backups/<handle>/...` for
   each reset candidate, and delete production/rehearsal RustFS keys under the
   same logical prefix if present. Production/rehearsal RustFS can also be
   cleared by removing the disposable production Compose RustFS volume before
   restarting services.
8. Dry-run or list object deletions before deleting and save the object
   inventory in the reset ledger.
9. For each reset candidate, comment on the issue:
   `Reset for faithful remigration; previous bundle removed because the execution mode/evaluator contract was materially different from Frontier-CS.`
10. Move each issue to Project #1 status `Todo` and edit tracker issue #9 to
    uncheck the corresponding Frontier-CS item.
11. Spawn GPT-5.5/xhigh workers for the seven work packages. Workers may prepare
    files, private ZIPs, issue notes, and test solutions. The lead serializes
    GitHub project changes, draft creation, approval, publishing, Moltbook fake
    links, and tracker updates.
12. Open fresh PRs for remigrated challenges, create Agentics drafts, upload
    private assets, validate, approve, publish, attach fake Moltbook URLs, submit
    meaningful test solutions, monitor completion, inspect observer views, back
    up new private bundles, move issues to `Post-merge`, and tick issue #9 only
    after the full lifecycle succeeds.

## Remigration Rules

- Read the Frontier-CS README and evaluator/interactor/benchmark code before
  writing files.
- Source-interactive algorithmic problems must use `piped_stdio`, preserve
  hidden state, query protocol, query limits, final-answer validation, and
  source scoring.
- `piped_stdio` specs must include `acknowledge_stdio_protocol_framing: true`.
  Statements must document session start, termination, EOF behavior, malformed
  output handling, and result ownership.
- Research P1s must preserve the source participant-visible resource contract
  and source-scale official/private data.
- Keep public validation small and deterministic.
- Keep official inputs, hidden answers, seeds, reference outputs, and
  evaluator-only metadata out of Git.

## Meaningful Test-Solution Rule

Each remigrated challenge must include `test-solutions/<handle>/`.

The solution must be an honest simple baseline that uses the documented
participant interface and general problem logic. It may be approximate or
suboptimal, but it must not hardcode public fixtures, encode private answers,
read evaluator/private files, exploit session metadata, or special-case runner
artifacts. If an obvious valid baseline exists, it should earn a nonzero public
score.

## Test Plan

- Reset verification:
  - Public challenge and test-solution dirs for reset candidates are absent
    after the reset commit.
  - Backup and production object stores have no old P1 private bundle keys.
  - Issue #9 and per-challenge project statuses match the reset ledger.
- Per-challenge validation:
  - Run `python3 scripts/validate_challenges.py` in `agentics-challenges`.
  - Run public local smoke with the meaningful test solution.
  - Inspect private ZIPs for traversal, symlinks, public overwrites, leaked
    answers, and expected `private-benchmark/...` shape.
  - For interactive challenges, test a valid baseline, malformed output, query
    limits, and EOF/sentinel behavior.
- Production lifecycle:
  - Fresh production rehearsal starts clean after volume reset.
  - Private bundles restore only from current backup objects.
  - Official submission completes and observer views are correct.

## Platform Bugs And Problems Log

Use this section for platform problems discovered during this reset/remigration.
For each entry, record symptom, cause, fix or decision, verification, and
affected challenge.

### 2026-05-28: Production Down Partially Stops Workers Before Runner Cleanup Failure

- Symptom: `just compose-prod-down --runner clean` stopped `worker-cpu` and
  `worker-gpu`, then failed with `Error in the hyper legacy client: client error
  (Connect)`.
- Cause: the clean shutdown path stopped workers before proving it could reach
  the dedicated runner Docker daemon used to list and remove runner containers.
- Fix: preflight runner cleanup with a dry-run cleanup call before stopping
  workers in the non-dry-run clean path.
- Verification: `cargo test -p agentics-ops compose_prod` passed after the fix.
- Affected challenge: migration-wide rollback setup, before individual
  challenge remigration.

### 2026-05-28: Runner Daemon Stop Requires Built Binary Under Sudo

- Symptom: after the partial `compose-prod-down --runner clean` failure, the
  dedicated production runner daemon still needed to be stopped. Running
  `sudo just compose-prod-runner-docker-down` was not reliable on this host
  because root does not have the same Rust toolchain setup as the user, while
  non-sudo Docker API access to `/srv/agentics/docker.sock` was denied by Unix
  socket permissions.
- Cause: the ops workflow mixes a root-owned runner daemon/socket with a
  source-build `cargo run` style command. The built binary works under sudo,
  but invoking the just recipe under sudo can fail before reaching the ops code.
- Fix or decision: use the already-built
  `target/debug/agentics-compose-prod runner-docker-down` under sudo for this
  reset. A follow-up ops improvement should make privileged runner daemon
  lifecycle commands reproducible without depending on root's Rustup state.
- Verification: `sudo target/debug/agentics-compose-prod runner-docker-down`
  stopped the runner daemon, and production Compose `postgres_data` and
  `rustfs_data` volumes were removed afterward.
- Affected challenge: migration-wide rollback setup, before individual
  challenge remigration.

### 2026-05-28: Stale Runner Socket Directory Blocks Runner Startup

- Symptom: after resetting production data and restarting production Compose,
  `runner-docker-up` failed with `Is a directory (os error 21)`.
- Cause: `/srv/agentics/docker.sock`, which should be a Unix socket, existed
  as a stale empty directory. The runner startup cleanup path only attempted
  `remove_file`, so it surfaced a low-context OS error instead of repairing the
  stale socket path.
- Fix: the runner Docker cleanup path now removes an empty stale directory at
  the socket or pidfile path and returns a clear invalid-config error if such a
  directory is non-empty.
- Verification: targeted `agentics-ops` tests cover empty-directory cleanup and
  non-empty-directory rejection.
- Affected challenge: migration-wide production setup, before individual
  challenge remigration.

### 2026-05-28: Docker Rejects Combined Runner Bridge And BIP Flags

- Symptom: after stale socket cleanup, `runner-docker-up` spawned dockerd but it
  failed to become ready. The daemon log ended with
  `You specified -b & --bip, mutually exclusive options`.
- Cause: the ops command asked dockerd to both use a named bridge and assign
  the bridge IP with `--bip`; the Docker version on this host rejects that
  combination.
- Fix: the ops command now prepares the dedicated host bridge with `ip link`
  and `ip addr`, then starts dockerd with the named bridge only.
- Verification: `cargo fmt --check` and `cargo test -p agentics-ops
  runner_docker` passed; after rebuilding the ops binary,
  `sudo target/debug/agentics-compose-prod runner-docker-up` reported the
  dedicated daemon ready at `unix:///srv/agentics/docker.sock` with the
  `agentics0` bridge.
- Affected challenge: migration-wide production setup, before individual
  challenge remigration.

### 2026-05-28: Private Bundle Restore Doubles Backup Prefix

- Symptom: `compose-prod-restore-private-bundles` restored backup objects whose
  source keys already started with `private-bundle-backups/` into production
  keys under `prod/private-bundle-backups/private-bundle-backups/...`.
- Cause: the backup-copy command always prepended the destination logical
  prefix without first making source keys relative to that same logical prefix.
- Fix: normalize source keys by stripping an existing destination prefix before
  building the production destination key.
- Verification: targeted `agentics-ops` tests cover stripping an existing
  prefix and preserving already-relative keys; production RustFS will be swept
  and restored again after this fix.
- Affected challenge: migration-wide production setup and rehearsal restore,
  before individual challenge remigration.

## Assumptions

- Scope is P1 only; P2 issues remain for a separate targeted-fix/private-bundle
  QA pass.
- Existing production-rehearsal data is disposable.
- Challenge handles remain the canonical public names.
- No platform API/schema changes are planned for this wave.
- If a platform bug blocks remigration and the fix is clear, fix it in Agentics
  and log it here; otherwise stop and report the blocker.
