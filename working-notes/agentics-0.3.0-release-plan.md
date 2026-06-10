# Agentics 0.3.0 Release Plan

Created: 2026-06-10

This file is the source-of-truth checklist for the 0.3.0 release pass. Before
finishing the release, reread this file and mark every item fixed, intentionally
deferred, or blocked.

## Product Decisions

- Release version is `0.3.0`.
- The CLI Cargo package is renamed from `agentics-cli` to `agentics`.
- The installed binary remains `agentics`.
- The `frontends/agentics-cli/` directory may stay unchanged to avoid noisy path
  churn.
- `agentics-cli` should not be published to crates.io.
- Crate `agentics` should be published as the CLI package.
- Public runner images are not republished in this pass.
- Ops/test/dev helper crates are not published.
- Challenge breakages caused by 0.3.0 contract changes are release blockers
  unless the challenge is explicitly disabled or removed as a catalog decision.

## Release Metadata Checklist

- [x] Bump workspace package version to `0.3.0`.
- [x] Rename CLI package to `agentics`.
- [x] Update `src/main.rs` in the CLI package to import the renamed library
  crate.
- [x] Update lockfile and package references.
- [x] Confirm `agentics --version` reports `0.3.0`.

## Documentation Checklist

- [x] Replace installed CLI docs with `cargo install --locked agentics`.
- [x] Replace developer CLI examples with
  `cargo run -p agentics --bin agentics -- ...`.
- [x] Keep the `skills/agentics-cli-workflow/SKILL.md` filename/link stable, but
  describe the installed CLI as `agentics`.
- [x] Update README, docs, skills, `.agents` skills, and frontend copy that says
  `agentics-cli` where it now means the user-facing CLI package.
- [x] Remove stale "v0.3" promises for unimplemented GitHub PR solution
  submission work; describe it as a future-version feature.
- [x] Rewrite implemented future-facing docs in present tense.
- [x] Document the 0.3.0 challenge contract expectations: metric-direction
  ranking, no `rank_score`, submission metadata, current execution modes, and
  stage resource profiles.

## Challenge Compatibility Checklist

- [x] Validate committed challenge bundles in `agentics-challenges`.
- [x] Validate dev seed catalog and test solutions.
- [x] Scan challenge evaluators/manifests for removed `rank_score` output.
- [x] Fix old resource-profile/execution fields.
- [x] Fix metric schemas missing primary metrics or directions.
- [x] Fix evaluator result payloads that do not match the current contract.
- [x] Fix metadata-aware evaluators or specs if they rely on old submission
  assumptions.
- [x] Fix private asset overlays, run manifests, and session manifests that no
  longer validate.
- [x] Add a working-note section listing challenge breakages found and fixes
  applied.
- [x] Run dev up/down and representative challenge smoke.
- [x] Run rehearsal up/down and health check.

## Publish Tooling Checklist

- [x] Add `agentics-publish` ops binary.
- [x] Add root `just publish`.
- [x] Use crates.io HTTP APIs with a proper `User-Agent`; do not use
  `cargo info` for availability checks.
- [x] Respect `429` and `Retry-After`.
- [x] Poll crates.io after publishing until each version is visible.
- [x] Support `--dry-run` and `--execute`.
- [x] Require `CARGO_REGISTRY_TOKEN` only for `--execute`.
- [x] Publish allowlist:
  - `agentics-error`
  - `agentics-domain`
  - `agentics-contracts`
  - `agentics-storage`
  - `agentics-config`
  - `agentics-persistence`
  - `agentics-services`
  - `agentics-runner`
  - `agentics`
  - `agentics-api-server`
  - `agentics-worker`
- [x] Exclude or mark non-publishable:
  - `agentics-ops`
  - `agentics-pre-commit`
  - `agentics-dev-checks`
  - `integration-tests`
- [x] Use Cargo workspace publish flow, with filtering for already-published
  packages based on API visibility.

## Production Backup And Restart Checklist

- [x] Freeze production writes by stopping `web`, `api`, `worker-cpu`, and
  `worker-gpu` if present while keeping PostgreSQL and RustFS running.
- [x] Back up PostgreSQL globals and database dump to a timestamped release
  backup directory.
- [x] Verify PostgreSQL dump with `pg_restore --list`.
- [x] Record backup checksums.
- [x] Back up RustFS bucket/prefix contents and record object inventory.
- [x] Stop the full production stack without deleting volumes.
- [x] Take cold tar archives of PG18 and RustFS volumes.
- [x] Rebuild production app/web images from the 0.3.0 checkout.
- [x] Restart production services.
- [x] Run `just prod::check`.
- [x] Inspect logs/status and fix any release or challenge-contract issues.

Production backup root:
`/srv/agentics-backups/releases/0.3.0/20260610T202434Z`.

Artifacts in that directory include:

- `globals.sql`
- `agentics.dump`
- `agentics.dump.list`
- `rustfs-inventory.jsonl`
- `rustfs-objects/`
- `agentics-prod_postgres_data_pg18.tar`
- `agentics-prod_rustfs_data.tar`
- `SHA256SUMS`

Production restart note: the first restart failed because migration 004 had
been edited after it was already applied in production. The fix restored
`004_evaluations.sql` to its immutable historical contents and kept the current
schema changes in migrations 007 and 008. After rebuilding the app image,
`just prod::up` applied migrations 7 and 8, `just prod::check` passed, and the
production catalog responded with 248 public challenges.

Post-repair verification note: after restoring migration 004, a targeted
`just test::integration` run passed the ignored CUDA smoke and all integration
tests. A final canonical `just test-env-status && just test-all` pass also
completed successfully, including fmt, clippy, workspace tests, web checks, and
the full GPU/CUDA Compose integration suite.

## Verification Checklist

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace --exclude integration-tests`
- [x] Targeted `cargo test -p agentics`
- [x] Publish-tool unit tests
- [x] `cd frontends/web && bun run generate:schemas:check`
- [x] `cd frontends/web && bunx biome check`
- [x] `cd frontends/web && bunx tsc --noEmit`
- [x] `cd frontends/web && bun test`
- [x] `just test-env-status`
- [x] `just test-all`
- [x] `just publish --dry-run`
- [ ] After publish, crates.io API confirms all selected `0.3.0` packages are
  visible.
- [ ] Clean temp install verifies `cargo install agentics --version 0.3.0`.

## Challenge Breakages Found

Record any challenge-specific issues discovered during validation, dev smoke, or
rehearsal smoke here.

- 2026-06-11: `challenge-repos/agentics-challenges/scripts/validate_challenges.py`
  validated 248 committed challenges. A scan over committed and dev challenge
  manifests/evaluators found no remaining `rank_score`, `primary_score`,
  `official_score`, top-level `scorer`, old `scorer_*` resource fields, or old
  stage network fields. Directly invoking `agentics-local-dev prepare` without
  the dev Compose environment failed because local-dev roots were unset; dev
  catalog validation must be exercised through `just dev::up`.
- 2026-06-11: `sudo env AGENTICS_DEV_USER=maplespark HOME=/home/maplespark
  PATH="$PATH" just dev::up` used project `agentics-dev-maplespark`, prepared
  12 local dev challenges, verified 12 runtime bundles, staged no missing test
  submissions, served web on `127.0.0.1:3010`, and shut down cleanly with
  `just dev::down`. The dev API and web ports did not overlap production.
- 2026-06-11: `sudo env HOME=/home/maplespark PATH="$PATH" just
  rehearsal::up` started `agentics-rehearsal` on `13100/13001` with PG 18 and
  both CPU/GPU workers. The first `rehearsal::check` raced API startup and
  failed health/catalog checks before the API was listening; rerunning the same
  check passed API health, public catalog, web reachability, Docker reachability,
  and GitHub egress. `rehearsal::down --runner clean` shut the stack down
  cleanly.
- 2026-06-11: Structural scan over 260 committed/dev bundle `spec.json` files
  found zero missing primary metrics, undeclared primary metrics, or missing
  metric directions. Execution-mode coverage in the catalog is 126
  `separated_evaluator`, 74 `piped_stdio`, and 60 `coexecuted_benchmark`
  bundles. A removed-field scan over challenge files found no `rank_score`,
  `primary_score`, `official_score`, `best_rank_score`, `scorer`, old
  `scorer_*`, or old flat stage-network fields.
