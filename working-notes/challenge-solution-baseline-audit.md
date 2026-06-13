# Challenge Solution Baseline Audit

## Summary

This note tracks production-submission readiness for checked-in `agentics-challenges` test solutions. The goal is to avoid treating every historical `smoke` string as evidence that a solution is bad while still keeping genuinely public-only, tiny-fixture, token-flood, or placeholder solutions out of broad production submission.

The 2026-06-13 metadata review split the previous broad “not submitter-ready” bucket into two groups:

- 180 solutions are now considered submitter-ready by the local readiness scanner.
- 68 solutions remain deferred by the production baseline submitter.

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

- `cube-sphere-packing-frontier-cs-algorithmic-48`
- `functional-cycle-reach-frontier-cs-algorithmic-252`
- `hamiltonian-path-frontier-cs-algorithmic-5`
- `signed-rooted-tree-frontier-cs-algorithmic-57`

The 2026-06-13 review also cleared stale wording for many existing official-capable baselines, including `distinct-bakery-types-frontier-cs-algorithmic-141`, `line-recovery-frontier-cs-algorithmic-117`, `llm-sql-small-frontier-cs-llm-sql-small`, `llm-sql-large-frontier-cs-llm-sql-large`, `palindromic-grid-paths-frontier-cs-algorithmic-256`, `poker-action-seeds-frontier-cs-algorithmic-143`, `repaired-road-set-frontier-cs-algorithmic-253`, `snake-path-minima-frontier-cs-algorithmic-233`, `sorted-mode-array-frontier-cs-algorithmic-257`, `treasure-hunt-choices-frontier-cs-algorithmic-70`, and `world-map-frontier-cs-algorithmic-6`.

## Deferred From Broad Production Submission

These 68 solutions remain out of broad production submission. Some are live-failed, some are small public fixtures, and some still need manual solution work before they should be submitted by `agentics-official`:

- `adaptive-impostor-search-frontier-cs-algorithmic-245`: live production official run failed during bounded CPU baseline smoke.
- `adventure-rank-segmentation-frontier-cs-algorithmic-61`: live production official run failed during bounded CPU baseline smoke.
- `average-permutation-frontier-cs-algorithmic-124`: live production official run failed during bounded CPU baseline smoke.
- `beacon-string-arrangement-frontier-cs-algorithmic-302`: simple public stdin/stdout smoke solution.
- `big-integer-subset-sum-frontier-cs-algorithmic-179`: conservative public smoke output.
- `binary-quadratic-assignment-frontier-cs-algorithmic-181`: conservative public smoke output.
- `binary-square-substrings-frontier-cs-algorithmic-228`: simple public stdin/stdout smoke solution.
- `boolean-expression-synthesis-frontier-cs-algorithmic-241`: simple public stdin/stdout smoke solution.
- `bridge-blasting-harvest-frontier-cs-algorithmic-306`: simple public stdin/stdout smoke solution.
- `brush-stroke-area-frontier-cs-algorithmic-133`: public smoke solution.
- `cant-late-ha-loose-large-frontier-cs-cbl-ha-ll`: simple smoke baseline still needs official-readiness review.
- `cant-late-ha-loose-small-frontier-cs-cbl-ha-ls`: simple smoke baseline still needs official-readiness review.
- `cant-late-ha-tight-large-frontier-cs-cbl-ha-tl`: simple smoke baseline still needs official-readiness review.
- `cant-late-ha-tight-small-frontier-cs-cbl-ha-ts`: simple smoke baseline still needs official-readiness review.
- `cant-late-la-loose-large-frontier-cs-cbl-la-ll`: simple smoke baseline still needs official-readiness review.
- `cant-late-la-loose-small-frontier-cs-cbl-la-ls`: simple smoke baseline still needs official-readiness review.
- `cant-late-la-tight-large-frontier-cs-cbl-la-tl`: simple smoke baseline still needs official-readiness review.
- `cant-late-la-tight-small-frontier-cs-cbl-la-ts`: simple smoke baseline still needs official-readiness review.
- `cant-late-ma-loose-large-frontier-cs-cbl-ma-ll`: simple smoke baseline still needs official-readiness review.
- `cant-late-ma-loose-small-frontier-cs-cbl-ma-ls`: simple smoke baseline still needs official-readiness review.
- `cant-late-ma-tight-large-frontier-cs-cbl-ma-tl`: simple smoke baseline still needs official-readiness review.
- `cant-late-ma-tight-small-frontier-cs-cbl-ma-ts`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-ha-loose-large-frontier-cs-cblm-ha-ll`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-ha-loose-small-frontier-cs-cblm-ha-ls`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-ha-tight-large-frontier-cs-cblm-ha-tl`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-ha-tight-small-frontier-cs-cblm-ha-ts`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-la-loose-large-frontier-cs-cblm-la-ll`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-la-loose-small-frontier-cs-cblm-la-ls`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-la-tight-large-frontier-cs-cblm-la-tl`: simple smoke baseline still needs official-readiness review.
- `cant-late-multi-la-tight-small-frontier-cs-cblm-la-ts`: simple smoke baseline still needs official-readiness review.
- `center-basket-transfer-frontier-cs-algorithmic-113`: public smoke solution.
- `cleaning-duty-automaton-frontier-cs-algorithmic-170`: conservative public smoke output.
- `clique-cover-frontier-cs-algorithmic-187`: deliberately simple valid-output smoke solution.
- `cloudcast-broadcast-frontier-cs-cloudcast`: simple smoke baseline still needs official-readiness review.
- `colored-ball-pole-sorting-frontier-cs-algorithmic-142`: public smoke solution.
- `communication-robot-network-frontier-cs-algorithmic-211`: conservative public smoke output.
- `completely-multiplicative-function-frontier-cs-algorithmic-83`: public smoke solution.
- `defensive-lineup-permutation-frontier-cs-algorithmic-313`: simple public stdin/stdout smoke solution.
- `delivery-route-selection-frontier-cs-algorithmic-152`: public smoke solution.
- `digit-grid-prefix-frontier-cs-algorithmic-110`: deliberately simple valid-output smoke solution.
- `distinct-xor-set-frontier-cs-algorithmic-111`: zero-set stub.
- `editor-width-discovery-frontier-cs-algorithmic-122`: small public-width baseline; private assumptions still need review.
- `fighter-base-strike-planning-frontier-cs-algorithmic-210`: empty or conservative public smoke output.
- `graph-coloring-frontier-cs-algorithmic-186`: color-by-index style stub.
- `heap-tree-sum-frontier-cs-algorithmic-209`: token-flood interactive fallback.
- `hidden-bipartite-graph-frontier-cs-algorithmic-106`: exhaustive only for small graphs and not a private-benchmark strategy.
- `imagenet-1m-frontier-cs-imagenet-1m`: cheap public-validation model path.
- `imagenet-2-5m-frontier-cs-imagenet-2-5m`: cheap public-validation model path.
- `imagenet-200k-frontier-cs-imagenet-200k`: cheap public-validation model path.
- `imagenet-500k-frontier-cs-imagenet-500k`: cheap public-validation model path.
- `imagenet-5m-frontier-cs-imagenet-5m`: cheap public-validation model path.
- `independent-set-complement-score-frontier-cs-algorithmic-183`: all-zero stub.
- `inversion-recovery-frontier-cs-algorithmic-73`: brute force only for small validation sizes.
- `knight-tour-path-frontier-cs-algorithmic-109`: zero-path stub.
- `limited-shuffle-restore-frontier-cs-algorithmic-59`: tiny public interactive session.
- `magic-word-spells-frontier-cs-algorithmic-69`: built for small validation sizes.
- `nbody-random-100k-frontier-cs-nbody-100k`: brute-force path is not practical for the 100k benchmark.
- `permutation-segment-geemu-frontier-cs-algorithmic-52`: brute-force only for tiny public cases.
- `sequence-transform-operations-frontier-cs-algorithmic-247`: only handles identical arrays or prints `No`.
- `skating-rink-route-frontier-cs-algorithmic-171`: empty or conservative route output.
- `space-thief-stars-frontier-cs-algorithmic-63`: path-shaped smoke-graph strategy only.
- `sphere-point-spread-frontier-cs-algorithmic-112`: deliberately simple valid-output smoke solution still needs meaningful scoring review.
- `symreg-mccormick-frontier-cs-symreg-mccormick`: cheap public-validation baseline.
- `symreg-mixed-polyexp-frontier-cs-symreg-mixed-polyexp`: cheap public-validation baseline.
- `symreg-peaks-frontier-cs-symreg-peaks`: cheap public-validation baseline.
- `symreg-ripple-frontier-cs-symreg-ripple`: cheap public-validation baseline.
- `symreg-sincos-frontier-cs-symreg-sincos`: cheap public-validation baseline.
- `uniform-cave-explorer-frontier-cs-algorithmic-80`: tiny public interactive session with random moves.

## Audit Rules

- Keep honest labels for public-only or intentionally cheap solutions; do not hide them by renaming them to baselines.
- Remove `smoke` wording only when the implementation is already a legitimate baseline for the challenge interface.
- A solution can be production-submission ready even when it is slow or uncompetitive, but it must not hardcode public validation answers or depend on private benchmark leakage.
- GPU readiness depends on host availability. At the time of this note, Agentics GPU scheduling works, but live GPU submissions are blocked by unrelated host processes consuming most GPU memory.
