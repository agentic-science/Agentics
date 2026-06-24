# Challenge Solution Baseline Audit

## Summary

This note tracks production-submission readiness for checked-in `agentics-challenges` test solutions. The goal is to avoid treating every historical `smoke` string as evidence that a solution is bad while still keeping genuinely public-only, tiny-fixture, token-flood, or placeholder solutions out of broad production submission.

The 2026-06-13 metadata review split the previous broad “not submitter-ready” bucket into explicit challenge-name deferrals and text-marker deferrals. Follow-up work on 2026-06-24 cleared the last two challenge-name deferrals.

- No challenge names are currently explicitly deferred by the default production baseline submitter.
- Other skips come from the secondary text-marker guard for local solution directories that still honestly describe themselves as smoke, cheap public, public-only, tiny public, or equivalent.

The submitter source of truth is `agentics-submit-baselines`: it defers the explicit default list and still has a secondary text-marker guard for local solution directories whose README or manifest says `smoke`, `cheap public`, `public-only`, `tiny public`, or equivalent language.

## Ready Baselines With Cleaned Metadata

These GPU-oriented solutions use baseline wording instead of smoke wording in README or `agentics.solution.json` metadata. They are honest baseline implementations, although not necessarily competitive:

- `cross-entropy-kernel-frontier-cs-cross-entropy`
- `decoding-attn-kernel-frontier-cs-decoding-attn`
- `flash-attn-kernel-frontier-cs-flash-attn`
- `fused-linear-ce-kernel-frontier-cs-fused-linear-ce`
- `fused-linear-jsd-kernel-frontier-cs-fused-linear-jsd`
- `gdpa-attention-kernel-frontier-cs-gdpa-attn`
- `gemm-annoying-frontier-cs-gemm-annoying`
- `gemm-k-skewed-frontier-cs-gemm-k-skewed`
- `gemm-near-tile-frontier-cs-gemm-near-tile`
- `gemm-rectangles-frontier-cs-gemm-rectangles`
- `gemm-transformer-frontier-cs-gemm-transformer`
- `group-gemm-frontier-cs-group-gemm`
- `mamba2-scan-frontier-cs-mamba2-scan`
- `qknorm-frontier-cs-qknorm`
- `quant-dot-int4-frontier-cs-quant-dot-int4`
- `ragged-attention-frontier-cs-ragged-attn`
- `vector-add-2-24-frontier-cs-vector-add-2-24`
- `vector-add-2-28-frontier-cs-vector-add-2-28`
- `vector-addition-frontier-cs-vector-addition-2-20`

These algorithmic baselines were upgraded after the first audit and are ready for broad baseline submission:

- `colored-ball-pole-sorting-frontier-cs-algorithmic-142`
- `cube-sphere-packing-frontier-cs-algorithmic-48`
- `functional-cycle-reach-frontier-cs-algorithmic-252`
- `hamiltonian-path-frontier-cs-algorithmic-5`
- `signed-rooted-tree-frontier-cs-algorithmic-57`
- `substring-ab-program-frontier-cs-algorithmic-23`

The 2026-06-13 review also cleared stale wording for many existing official-capable baselines, including `distinct-bakery-types-frontier-cs-algorithmic-141`, `line-recovery-frontier-cs-algorithmic-117`, `llm-sql-small-frontier-cs-llm-sql-small`, `llm-sql-large-frontier-cs-llm-sql-large`, `palindromic-grid-paths-frontier-cs-algorithmic-256`, `poker-action-seeds-frontier-cs-algorithmic-143`, `repaired-road-set-frontier-cs-algorithmic-253`, `snake-path-minima-frontier-cs-algorithmic-233`, `sorted-mode-array-frontier-cs-algorithmic-257`, `treasure-hunt-choices-frontier-cs-algorithmic-70`, and `world-map-frontier-cs-algorithmic-6`.

## Explicitly Deferred From Broad Production Submission

No checked-in solution is currently challenge-name deferred from broad production submission.

The two previous hard cases were cleared on 2026-06-24:

- `colored-ball-pole-sorting-frontier-cs-algorithmic-142` now uses a divide-and-conquer interval partition construction. It validates on the upstream-shaped `50 x 400` Frontier-CS cases under the 2,000,000-operation cap, with about 1.19M to 1.23M moves and roughly 45-47% of the public reference score.
- `substring-ab-program-frontier-cs-algorithmic-23` keeps its general A=B substring-program baseline and increases the trusted checker/evaluator budget for the official-shaped exhaustive cases. Public upstream-shaped hard cases `13`, `14`, `15`, and `22` completed with ratio `1.0` in about 37-39 seconds per checker invocation.

Production confirmation:

- `colored-ball-pole-sorting-frontier-cs-algorithmic-142`, target `linux-arm64-cpu`, submission `112bfd88-b7e6-4be9-92db-cdce9fce2518`, completed with primary metric `score = 46.815`.
- `substring-ab-program-frontier-cs-algorithmic-23`, target `linux-arm64-cpu`, submission `c1385f37-fc62-49e4-b910-14b0f04774e3`, completed with primary metric `score = 100.0`.

The eight previously failed piped-stdio baselines are no longer challenge-name deferred. Corrected 2026-06-14 official-private replay proved they are protocol-safe: `adaptive-impostor-search-frontier-cs-algorithmic-245`, `disk-probing-frontier-cs-algorithmic-60`, `heap-tree-sum-frontier-cs-algorithmic-209`, `hidden-circuit-gates-frontier-cs-algorithmic-101`, `induced-triple-graph-frontier-cs-algorithmic-120`, `inversion-recovery-frontier-cs-algorithmic-73`, `mineral-pairing-frontier-cs-algorithmic-125`, and `space-thief-stars-frontier-cs-algorithmic-63`.

## Upgraded From The Deferred Set

On 2026-06-13, 67 previously deferred solutions were replaced or reclassified as meaningful baselines after focused subagent review and local validation evidence. This includes the Cant-Late families, ImageNet nearest-centroid baselines, symbolic-regression expression baselines, interactive hidden-state baselines, scientific/geometric baselines, and constructive algorithmic baselines tracked in `working-notes/deferred-baseline-solution-progress.md`.

## Audit Rules

- Keep honest labels for public-only or intentionally cheap solutions; do not hide them by renaming them to baselines.
- Remove `smoke` wording only when the implementation is already a legitimate baseline for the challenge interface.
- A solution can be production-submission ready even when it is slow or uncompetitive, but it must not hardcode public validation answers or depend on private benchmark leakage.
- GPU readiness depends on host availability. Later production checks confirmed representative GPU submissions completed after the earlier host-level GPU memory contention cleared.
