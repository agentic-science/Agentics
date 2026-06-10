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
- [ ] Confirm `agentics --version` reports `0.3.0`.

## Documentation Checklist

- [ ] Replace installed CLI docs with `cargo install --locked agentics`.
- [ ] Replace developer CLI examples with
  `cargo run -p agentics --bin agentics -- ...`.
- [ ] Keep the `skills/agentics-cli-workflow/SKILL.md` filename/link stable, but
  describe the installed CLI as `agentics`.
- [ ] Update README, docs, skills, `.agents` skills, and frontend copy that says
  `agentics-cli` where it now means the user-facing CLI package.
- [ ] Remove stale "v0.3" promises for unimplemented GitHub PR solution
  submission work; describe it as a future-version feature.
- [ ] Rewrite implemented future-facing docs in present tense.
- [ ] Document the 0.3.0 challenge contract expectations: metric-direction
  ranking, no `rank_score`, submission metadata, current execution modes, and
  stage resource profiles.

## Challenge Compatibility Checklist

- [ ] Validate committed challenge bundles in `agentics-challenges`.
- [ ] Validate dev seed catalog and test solutions.
- [ ] Scan challenge evaluators/manifests for removed `rank_score` output.
- [ ] Fix old resource-profile/execution fields.
- [ ] Fix metric schemas missing primary metrics or directions.
- [ ] Fix evaluator result payloads that do not match the current contract.
- [ ] Fix metadata-aware evaluators or specs if they rely on old submission
  assumptions.
- [ ] Fix private asset overlays, run manifests, and session manifests that no
  longer validate.
- [ ] Add a working-note section listing challenge breakages found and fixes
  applied.
- [ ] Run dev up/down and representative challenge smoke.
- [ ] Run rehearsal up/down and health check.

## Publish Tooling Checklist

- [ ] Add `agentics-publish` ops binary.
- [ ] Add root `just publish`.
- [ ] Use crates.io HTTP APIs with a proper `User-Agent`; do not use
  `cargo info` for availability checks.
- [ ] Respect `429` and `Retry-After`.
- [ ] Poll crates.io after publishing until each version is visible.
- [ ] Support `--dry-run` and `--execute`.
- [ ] Require `CARGO_REGISTRY_TOKEN` only for `--execute`.
- [ ] Publish allowlist:
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
- [ ] Exclude or mark non-publishable:
  - `agentics-ops`
  - `agentics-pre-commit`
  - `agentics-dev-checks`
  - `integration-tests`
- [ ] Use Cargo workspace publish flow, with filtering for already-published
  packages based on API visibility.

## Production Backup And Restart Checklist

- [ ] Freeze production writes by stopping `web`, `api`, `worker-cpu`, and
  `worker-gpu` if present while keeping PostgreSQL and RustFS running.
- [ ] Back up PostgreSQL globals and database dump to a timestamped release
  backup directory.
- [ ] Verify PostgreSQL dump with `pg_restore --list`.
- [ ] Record backup checksums.
- [ ] Back up RustFS bucket/prefix contents and record object inventory.
- [ ] Stop the full production stack without deleting volumes.
- [ ] Take cold tar archives of PG18 and RustFS volumes.
- [ ] Rebuild production app/web images from the 0.3.0 checkout.
- [ ] Restart production services.
- [ ] Run `just prod::check`.
- [ ] Inspect logs/status and fix any release or challenge-contract issues.

## Verification Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --exclude integration-tests`
- [ ] Targeted `cargo test -p agentics`
- [ ] Publish-tool unit tests
- [ ] `cd frontends/web && bun run generate:schemas:check`
- [ ] `cd frontends/web && bunx biome check`
- [ ] `cd frontends/web && bunx tsc --noEmit`
- [ ] `cd frontends/web && bun test`
- [ ] `just test-env-status`
- [ ] `just test-all`
- [ ] `just publish --dry-run`
- [ ] After publish, crates.io API confirms all selected `0.3.0` packages are
  visible.
- [ ] Clean temp install verifies `cargo install agentics --version 0.3.0`.

## Challenge Breakages Found

Record any challenge-specific issues discovered during validation, dev smoke, or
rehearsal smoke here.

- None recorded yet.
