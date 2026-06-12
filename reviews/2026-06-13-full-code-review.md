# Full Code Review: 2026-06-13

## Scope

This review covered the current Agentics workspace after the strict required-nullable challenge contract work, GPU/dev/rehearsal/prod workflow changes, baseline submission tooling, and recent CLI-first/auth/frontend changes.

Review subagents used GPT-5.5 with xhigh effort across backend/services/persistence/API, runner/storage/contracts, CLI/ops, frontend/docs/schema, and architecture/test quality.

## Fixed Findings

### P1: Private bundle staging could inherit permissive scratch permissions

Private runtime bundles and bundle tar files were assembled under `AGENTICS_STORAGE_WORK_ROOT` without enforcing private permissions on the work root, staging parents, and temporary tar files. On a normal `022` umask, private benchmark overlays could briefly live in group/world-readable scratch directories or files.

Status: fixed.

Fix:

- `agentics-storage` now exposes and uses private directory creation helpers.
- Bundle tar packing creates private parent directories and `0600` temporary files.
- Challenge review record assembly tightens the storage work root and staging parent before private/public runtime bundle assembly.

### P1: Strict challenge contracts left active fixtures/generators stale

Several integration and rehearsal fixture generators still emitted pre-strict run manifests and specs, including top-level evaluator-specific fields such as `expected`, missing required nullable setup fields, missing hardware nullable fields, missing metric nullable fields, and missing coexecuted `solution.run: null`.

Status: fixed.

Fix:

- Integration fixtures and support generators now emit explicit required nullable fields.
- Run manifests move evaluator-specific values under `metadata`.
- Rehearsal-generated separated, piped, and coexecuted bundles now use the current contract shape.

### P2: Private asset `required_paths` were checked against the merged bundle

The publish path checked manifest-declared private asset `required_paths` only after all private overlays were merged into the runtime bundle. An asset could pass by relying on a path from the public bundle or a different private asset.

Status: fixed.

Fix:

- Private asset upload validation now checks the uploaded ZIP envelope itself for each declared required path.
- Publish-time assembly revalidates stored private asset ZIPs before extraction.
- Regression tests cover missing required paths and directory requirements satisfied by child files.

### P2: Baseline submitter could duplicate official submissions after interruption

The baseline submitter recorded a submission ID before waiting for terminal status, but a retry only skipped records with `status == "completed"`. An interrupted or wait-failed run could POST a duplicate official submission.

Status: fixed.

Fix:

- The submitter resumes waiting on a known nonterminal submission ID unless `--resubmit` is explicit.
- Remote plain HTTP API URLs are rejected unless loopback or `AGENTICS_ALLOW_INSECURE_REMOTE_HTTP=true`.

### P2: Runner-generated evaluator manifest filename could collide with a valid run name

Separated evaluator output directories are keyed by `run_name`, while the runner also writes `agentics-runs.json` into the same root. A challenge run named `agentics-runs.json` could collide with runner metadata.

Status: fixed.

Fix:

- The challenge contract now reserves `agentics-runs.json` as an invalid run name.
- A regression test covers the reservation.

### P2: Rehearsal GPU heartbeat parser accepted malformed payloads

The production rehearsal GPU heartbeat check stringified the whole payload and tokenized it, so malformed payloads containing the word `gpu` could pass.

Status: fixed.

Fix:

- Heartbeats must now expose `payload.accelerators` as an array containing `"gpu"`.
- Tests cover valid GPU arrays, CPU-only arrays, and malformed string payloads.

### P2: `challenge-creator check --json` lost structured reports on failure

The command built a JSON report but returned it through `anyhow::bail!`, so CLI users got an error string instead of machine-readable JSON.

Status: fixed.

Fix:

- JSON-mode failures now carry structured output while still exiting nonzero.
- Table-mode failure behavior remains unchanged.

### P2: Public frontend schema still defaulted missing `metric_schema`

The Rust public challenge bundle DTO still had a serde default on `metric_schema`, and the generated frontend schema accepted missing metrics as the legacy default.

Status: fixed.

Fix:

- Removed the serde default from the public challenge bundle metric schema.
- Added a frontend schema regression test for missing `spec.metric_schema`.

### P2: Dev stack check used the wrong Docker daemon for runner probing

`just dev::check` verified Compose services with the dedicated dev runner socket but then ran `agentics-check-local-mvp` without passing that socket through.

Status: fixed.

Fix:

- The dev check now passes `DOCKER_HOST`, `AGENTICS_DOCKER_HOST`, and `AGENTICS_DOCKER_SOCKET_PATH` consistently.

### P2: Production/rehearsal profile resolution could silently drop env-file GPU profiles

Ambient `COMPOSE_PROFILES` took precedence over profiles declared in the production/rehearsal env file, so a stale shell could omit `gpu` even when the env file requested it.

Status: fixed.

Fix:

- Compose profiles are now merged from env-file and process values.

### P2: CPU-only production checks could fail on missing CUDA probe image

The runner PyPI egress probe always used `AGENTICS_WORKER_GPU_PROBE_IMAGE` with `--pull never`.

Status: fixed.

Fix:

- The PyPI probe is skipped when the GPU Compose profile is inactive.

### P2: Accepted private-material network risk lacked checker warnings

Earlier reviews accepted the MVP risk that official participant-containing stages may have network access when private benchmark material exists, but required warnings in checking scripts.

Status: fixed.

Fix:

- `agentics challenge-creator check` now emits warnings when official evaluation may expose private material and a participant-containing official stage has network enabled.

### P2: Rehearsal agent token lived in a debug-printable string

The production rehearsal state stored the temporary agent bearer token as `String`.

Status: fixed.

Fix:

- The token is stored as `SecretString` and exposed only at HTTP request construction.

## Accepted MVP Risks Rechecked

The following previously accepted risks still match prior decisions and were not changed in this pass:

- Public aggregate stats may include broader non-public submission activity.
- Public stats visibility policy remains partly owned by persistence SQL for MVP.
- Pioneer-code plaintext/re-display remains accepted for MVP.
- Permission-repair container trust remains accepted for MVP.
- Coexecuted benchmark and writable `/io` trust-boundary limitations remain documented MVP tradeoffs.
- Challenge publish storage cleanup remains best-effort for now.

## Deferred Maintainability Items

### P2: Baseline submitter duplicates solution packaging policy

The ops baseline submitter has local packaging logic similar to the CLI package builder. This is maintainability debt rather than an immediate correctness bug.

Status: deferred.

Recommended follow-up: move the shared packaging policy into a crate-level helper consumed by CLI and ops.

### P3: Stale browser creator workflow helper exports remain

The web creator console is CLI-first now, but `creatorApi.ts` still exports browser helpers for older creator workflow operations.

Status: deferred.

Recommended follow-up: remove unused browser creator operation helpers after confirming no tests or pages still import them.

### P3: Large files approaching refactor threshold

The large-file scan still has several files above 900 lines, especially `ops/src/production_rehearsal.rs`, `ops/src/compose_prod.rs`, and persistence/service modules.

Status: deferred.

Recommended follow-up: split rehearsal phases, compose-prod commands, and solution submission persistence by responsibility before they cross the 1200-line threshold.

## Verification Notes

Targeted Rust package tests, frontend schema checks, frontend unit tests, and the canonical full GPU suite were run after fixes.
The first `just test-all` attempt exposed stale strict-contract fields in the CUDA smoke and public-eval inline challenge fixtures; those fixtures were fixed and the final `just test-all` pass completed successfully.
