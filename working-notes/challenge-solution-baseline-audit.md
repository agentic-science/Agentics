# Challenge Solution Baseline Audit

## Summary

This note tracks the production-submission readiness of checked-in `agentics-challenges` test solutions. The goal is to avoid treating every `smoke` string as evidence that a solution is bad. Some solution READMEs used stale smoke-test language even though the implementation is a legitimate baseline; those labels should be cleaned. Other solutions really are public-only, tiny-fixture, token-flood, or placeholder baselines and should stay out of broad production submission until they are replaced.

## Ready Baselines With Cleaned Metadata

These GPU-oriented solutions now use baseline wording instead of smoke wording in their README or `agentics.solution.json` metadata. They are honest baseline implementations, although not necessarily competitive:

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

These algorithmic baselines were upgraded after the first audit and are now ready for broad baseline submission:

- `cube-sphere-packing-frontier-cs-algorithmic-48`
- `functional-cycle-reach-frontier-cs-algorithmic-252`
- `hamiltonian-path-frontier-cs-algorithmic-5`
- `signed-rooted-tree-frontier-cs-algorithmic-57`

## Keep Out Of Broad Production Submission For Now

These solutions are deliberately cheap, public-only, tiny-fixture, or incomplete. They should not be added to the production baseline submitter allowlist until they have a meaningful official-capable baseline:

- `adaptive-impostor-search-frontier-cs-algorithmic-245`: live production official run failed during bounded CPU baseline smoke.
- `average-permutation-frontier-cs-algorithmic-124`: live production official run failed during bounded CPU baseline smoke.
- `distinct-bakery-types-frontier-cs-algorithmic-141`: token-flood interactive fallback.
- `editor-width-discovery-frontier-cs-algorithmic-122`: public-only baseline from the earlier audit.
- `heap-tree-sum-frontier-cs-algorithmic-209`: token-flood interactive fallback.
- `imagenet-200k-frontier-cs-imagenet-200k`: cheap public-validation model path.
- `imagenet-500k-frontier-cs-imagenet-500k`: cheap public-validation model path.
- `imagenet-1m-frontier-cs-imagenet-1m`: cheap public-validation model path.
- `imagenet-2-5m-frontier-cs-imagenet-2-5m`: cheap public-validation model path.
- `imagenet-5m-frontier-cs-imagenet-5m`: cheap public-validation model path.
- `limited-shuffle-restore-frontier-cs-algorithmic-59`: tiny public interactive session.
- `line-recovery-frontier-cs-algorithmic-117`: single-line public case plus fallback.
- `llm-sql-small-frontier-cs-llm-sql-small`: cheap public-validation model path.
- `llm-sql-large-frontier-cs-llm-sql-large`: cheap public-validation model path.
- `palindromic-grid-paths-frontier-cs-algorithmic-256`: tiny-grid exhaustive public baseline.
- `poker-action-seeds-frontier-cs-algorithmic-143`: token-flood interactive fallback.
- `repaired-road-set-frontier-cs-algorithmic-253`: token-flood interactive fallback.
- `snake-path-minima-frontier-cs-algorithmic-233`: token-flood interactive fallback.
- `sorted-mode-array-frontier-cs-algorithmic-257`: token-flood interactive fallback.
- `symreg-sincos-frontier-cs-symreg-sincos`: cheap public-validation baseline.
- `symreg-mccormick-frontier-cs-symreg-mccormick`: cheap public-validation baseline.
- `symreg-mixed-polyexp-frontier-cs-symreg-mixed-polyexp`: cheap public-validation baseline.
- `symreg-peaks-frontier-cs-symreg-peaks`: cheap public-validation baseline.
- `symreg-ripple-frontier-cs-symreg-ripple`: cheap public-validation baseline.
- `treasure-hunt-choices-frontier-cs-algorithmic-70`: tiny public interactive session.
- `uniform-cave-explorer-frontier-cs-algorithmic-80`: tiny public interactive session.
- `world-map-frontier-cs-algorithmic-6`: tiny public path-graph case.

## Audit Rules

- Keep honest labels for public-only or intentionally cheap solutions; do not hide them by renaming them to baselines.
- Remove `smoke` wording only when the implementation is already a legitimate baseline for the challenge interface.
- A solution can be production-submission ready even when it is slow or uncompetitive, but it must not hardcode public validation answers or depend on private benchmark leakage.
- GPU readiness depends on host availability. At the time of this note, Agentics GPU scheduling works, but live GPU submissions are blocked by unrelated host processes consuming most GPU memory.
