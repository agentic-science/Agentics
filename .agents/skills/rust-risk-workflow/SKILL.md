---
name: rust-risk-workflow
description: Run and interpret Agentics Rust change-risk analysis with cargo llvm-cov and cargo crap. Use when asked to run Rust coverage, CRAP metrics, change-risk reports, coverage-informed complexity analysis, or to investigate high-risk Rust functions in this repository.
---

# Rust Risk Workflow

Use this skill to produce coverage-informed CRAP reports for the Agentics Rust
workspace. Prefer the checked-in `just` recipes because they encode the local
LCOV paths and `.cargo-crap.toml` filters. Do not skip tests when running the
integration workflow, including tests marked `#[ignore]`.

## Quick Workflow

1. Check the worktree first with `git status --short`.
2. For a fast local signal that needs no database or DGX quota root, run:

   ```bash
   just rust-risk-unit
   ```

   The LCOV output is `target/llvm-cov/agentics-workspace.lcov`.

3. For the better signal that includes DB-backed integration coverage, ensure a
   local Postgres test database is available, then run the full integration
   workflow, including ignored hardware tests:

   ```bash
   just infra-up
   AGENTICS_DATABASE_URL='postgres://agentics:agentics@127.0.0.1:5432/agentics_test' \
     just rust-risk-integration
   ```

   If the user or environment already provides `DATABASE_URL` or
   `AGENTICS_DATABASE_URL`, use that instead of inventing one. The LCOV output
   is `target/llvm-cov/agentics-workspace-with-integration.lcov`. If the test
   run fails, stop and report the failing tests; do not run `cargo crap` against
   an older LCOV file.

4. Use `AGENTICS_CRAP_TOP=<n>` to limit or expand the ranked report:

   ```bash
   AGENTICS_CRAP_TOP=20 just rust-risk-integration
   ```

## Direct Commands

Use direct commands only when changing or debugging the recipes:

```bash
mkdir -p target/llvm-cov
cargo llvm-cov --workspace --exclude integration-tests \
  --lcov --output-path target/llvm-cov/agentics-workspace.lcov
cargo crap --workspace \
  --lcov target/llvm-cov/agentics-workspace.lcov \
  --top "${AGENTICS_CRAP_TOP:-30}"
```

For integration coverage:

```bash
mkdir -p target/llvm-cov
DATABASE_URL="$DATABASE_URL" cargo llvm-cov --workspace \
  --lcov --output-path target/llvm-cov/agentics-workspace-with-integration.lcov \
  -- --include-ignored
cargo crap --workspace \
  --lcov target/llvm-cov/agentics-workspace-with-integration.lcov \
  --top "${AGENTICS_CRAP_TOP:-30}"
```

Quota-sensitive integration tests require a prepared Linux DGX XFS quota root
from `agentics-prepare-dgx-spark-test-storage` and the matching
`AGENTICS_TEST_RUNNER_*` environment variables. If that root is unavailable,
report the failure or ask the user to prepare the test root; do not hide it with
`--skip`.

Ignored hardware tests, such as DGX CUDA smoke coverage, are part of this
workflow. If the host lacks the required hardware or published images, report
that failure directly instead of dropping `--include-ignored`.

## Interpretation

- Prefer `rust-risk-integration` for API, DB, worker, and runner decisions. The
  unit-only report can mark integration-covered production code as 0% covered.
- Report the top offenders with CRAP score, cyclomatic complexity, coverage
  percentage, function name, and file/line.
- Treat 0% coverage as a prompt to verify whether a function is untested,
  requires external infrastructure, or is only exercised through a path excluded
  from the selected workflow.
- Do not fail or block work on legacy CRAP findings unless the user asked for a
  gate. For mature code, prefer a baseline or no-regression policy.
- High CRAP usually calls for one of three actions: add meaningful tests,
  decompose the function, or explicitly document why the path needs manual
  validation.

## Troubleshooting

- If `cargo llvm-cov` or `cargo crap` is missing, report that the cargo
  subcommand is not installed and stop unless the user asks you to install it.
- If integration coverage fails because `DATABASE_URL` is missing, ask the user
  for the intended database URL or use the documented local URL only after
  confirming the local platform DB is running.
- `target/llvm-cov/` is build output and should remain untracked.
