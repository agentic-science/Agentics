# Rank Score Direction Refactor

## Product Decision

Agentics must not encode leaderboard ordering through a platform-owned `rank_score` where higher is always better.
Challenge authors declare a primary metric in `metric_schema.ranking.primary_metric_name`, each metric declares its own `direction`, and the platform ranks by those natural metric values.

For example, a challenge with:

```json
{
  "ranking": { "primary_metric_name": "size_product" },
  "metrics": [{ "name": "size_product", "direction": "minimize" }]
}
```

ranks smaller `size_product` values ahead of larger ones.
Evaluators must emit natural `aggregate_metrics`; they must not negate values to satisfy a platform-side "higher is better" convention.

## New Contract

- Evaluator `result.json` must not contain `rank_score`.
- Completed official results must contain the declared primary metric in `aggregate_metrics`.
- Ranking uses the declared primary metric direction.
- Tie-breakers, when present and available, use their own declared directions.
- Final ordering uses deterministic fallback keys only after metric comparisons are equal or unavailable.
- Public/API/CLI/web result surfaces expose the official primary metric, not `rank_score` or `best_rank_score`.

## Implementation Checklist

- [x] Backend evaluator result DTO removes `rank_score`.
- [x] Evaluator outputs containing `rank_score` are rejected clearly.
- [x] Persistence no longer stores or queries `evaluations.rank_score`.
- [x] Leaderboard rows no longer store or expose `best_rank_score`.
- [x] Leaderboard best-entry replacement compares natural metrics by direction.
- [x] Public API DTOs and projections remove `rank_score` and `best_rank_score`.
- [x] CLI renderers and fixtures remove `rank_score` and `best_rank_score`.
- [x] Web schemas are regenerated and web render paths remove rank-score labels.
- [x] English and Chinese docs remove rank-score semantics.
- [x] Existing challenge evaluators stop emitting `rank_score`.
- [x] Minimization challenges use natural positive duration/cost/size metrics.
- [x] A guard test or validation check prevents evaluator `rank_score` reintroduction.
- [x] Dev stack starts with explicit dev identity, seeds challenges, exercises `hello-world-rs`, and stops cleanly.
- [x] Rehearsal stack starts without stopping production, passes checks, and stops cleanly.
- [ ] Rehearsal authenticated CPU harness smoke runs.
      Blocked: `just rehearsal::run-cpu` requires `AGENTICS_ADMIN_SERVICE_TOKEN` or `--admin-service-token-stdin`; the disposable rehearsal env does not currently provide one, and no ops bootstrap command exists.
      Do not hand-insert an admin token into the DB just for this smoke.
- [x] Final `just test-env-status` and `just test-all` pass.

## Challenge Issues Found During Smoke

No challenge contract failures were found.

Smoke notes:

- `hello-world-rs` local validation passed after rebuilding the CLI binary: `size_product` is emitted as a positive minimized metric and no `rank_score` appears in the result.
- Dev startup initially hit the expected pre-MVP migration checksum break on an old disposable dev DB.
  Resetting only the `agentics-dev-maplespark` volumes fixed it; fresh PG18 migrations, challenge preparation, seed submission staging, worker evaluation, and shutdown then passed.
- Rehearsal `check` initially raced API startup; retrying after API listened on port 3100 passed. Production remained up throughout.

## Final Audit

Implementation audit performed after the first backend/frontend/challenge sweep:

- `cargo check --workspace --all-targets` passes.
- Frontend schemas were regenerated from Rust DTOs.
- Repo scan finds no `rank_score` or `best_rank_score` references outside the intentional migration, rejection tests, and unrelated upstream source repos.
- Challenge evaluator Python syntax pass with `compileall` passed after the mechanical payload cleanup.

Additional verification completed:

- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `just test-env-status-cpu && just test-all-cpu` passes.
- `just test-env-status && just test-all` passes, including the ignored `cuda_smoke` integration test.
- Dev stack up/down smoke passed with explicit `AGENTICS_DEV_USER=maplespark`.
- Rehearsal stack up/check/down smoke passed with production still running.

Only the authenticated rehearsal harness mutation pass remains blocked on a disposable admin service token.
