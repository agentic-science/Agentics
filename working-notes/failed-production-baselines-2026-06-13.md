# Failed Production Baselines, 2026-06-13

This ledger tracks the 25 CPU baseline submissions that failed during the first broad production baseline pass for `agentics-official`.

Product decision: runner infrastructure should not turn participant timeout, participant EOF, wrong participant protocol, or participant nonzero exit into opaque platform failure when trusted evaluator code can still write a valid failed result. The trusted evaluator should be allowed to persist structured failed diagnostics, while evaluator failure, missing or invalid result JSON, interaction byte-limit breach, global timeout, storage failure, and infrastructure errors remain runner failures.

## Platform Checklist

- [x] `separated_evaluator` continues into the trusted evaluator after participant run timeout or nonzero exit when setup and build succeeded.
- [x] `separated_evaluator` materializes participant run outcome metadata for the evaluator.
- [x] `piped_stdio` already accepted evaluator-authored failed results when the participant exits badly after evaluator protocol closure, because the Docker interactive backend treats evaluator-success as the authoritative protocol closure.
- [x] Evaluator container failure, missing or invalid result JSON, byte-limit breach, global runner timeout, storage failure, and infrastructure errors remain runner failures.
- [x] Frontier-CS Testlib-style interactive wrappers convert timeout, missing report, EOF, and malformed output into structured failed results.
- [x] Frontier-CS Testlib-style interactive wrappers preserve valid Testlib reports even when the interactor exits with a nonzero score/report code, and only classify nonzero interactor exit as protocol failure when no report was produced.
- [x] Structured failed results include capped diagnostics and metrics such as `protocol_errors`, `case_count`, and `query_count` when the evaluator knows them.
- [x] All Frontier-CS Testlib wrapper scripts now apply a per-case `case_timeout_sec` guard, defaulting to 20 seconds, so a blocked participant can become a structured failed result.
- [x] `agentics-submit-baselines` defers the eight production-failed piped baselines by default until official replay proves they are protocol-safe.

## Submission Ledger

| Challenge | Observed failure shape | Likely root cause | Fix owner | Resubmission status |
| --- | --- | --- | --- | --- |
| `cycle-chord-identification-frontier-cs-algorithmic-16` | interactive evaluator timeout around 180s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | fixed locally with query budget and fallback chord |
| `dango-stick-grouping-frontier-cs-algorithmic-217` | resource or protocol abort around 64s | expensive recursive subset query strategy | evaluator wrapper, solution remains best effort | structured failure supported; solution still best-effort |
| `demagnetized-magnets-frontier-cs-algorithmic-255` | interactive evaluator timeout around 120s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | fixed locally with bounded anchor search and linear classification reserve |
| `disk-probing-frontier-cs-algorithmic-60` | piped protocol stalls on official data | solution handles protocol closure and sentinel framing poorly | solution + wrapper | default-deferred until official replay proves protocol safety |
| `dna-matching-probability-frontier-cs-algorithmic-121` | quick scale/resource failure | exponential baseline does not scale | solution | fixed locally with thresholded conservative fallback for large `m` |
| `graph-isomorphism-edge-match-frontier-cs-algorithmic-180` | participant run timeout around 10s | full huge-input materialization in identity baseline | solution | fixed locally with streaming first-token reader |
| `greedy-tree-blackbox-frontier-cs-algorithmic-93` | interactive evaluator timeout around 120s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | fixed locally with exact-mode cutoff and valid star-tree fallback |
| `hamiltonian-path-frontier-cs-algorithmic-5` | participant run timeout around 8s | heavy multi-start heuristic exceeds official cap | solution | fixed locally by removing insertion search and bounding start variants |
| `hidden-circuit-gates-frontier-cs-algorithmic-101` | quick protocol/correctness failure | final gate reconstruction and `-1`/EOF handling are fragile | solution + wrapper | default-deferred until official replay proves protocol safety |
| `hidden-cycle-length-frontier-cs-algorithmic-14` | interactive evaluator timeout around 120s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | fixed locally with walk/time budget and valid guess fallback |
| `impartial-game-graph-frontier-cs-algorithmic-231` | interactive evaluator timeout around 120s | official-scale strategy too slow or too chatty | evaluator wrapper | structured failure supported; current solver kept as meaningful but hard-scale best effort |
| `induced-triple-graph-frontier-cs-algorithmic-120` | interactive evaluator timeout around 120s | many queries per official case | evaluator wrapper + solution | default-deferred until existing timeout handling is proven to persist structured official results |
| `ink-pen-selection-frontier-cs-algorithmic-68` | interactive evaluator timeout around 120s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | fixed locally with bounded candidate sampling and try cap |
| `maximum-position-permutation-frontier-cs-algorithmic-17` | interactive evaluator timeout around 180s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | fixed locally with `30n` query budget and fallback position |
| `mineral-pairing-frontier-cs-algorithmic-125` | interactive evaluator timeout around 120s | query-heavy baseline exceeds official budget | solution + evaluator wrapper | default-deferred until existing timeout handling is proven to persist structured official results |
| `modulo-collision-size-frontier-cs-algorithmic-36` | participant run timeout around 10s | C++ random factoring baseline has too high a query/sample budget | solution | fixed locally by using the bounded Python small-divisor probing baseline |
| `online-mst-decisions-frontier-cs-algorithmic-153` | interactive evaluator timeout around 120s | per-edge Monte Carlo baseline exceeds official budget | solution + evaluator wrapper | fixed locally with immediate DSU accept-if-connects decisions |
| `poker-action-seeds-frontier-cs-algorithmic-143` | interactive evaluator timeout around 120s | RATE sampling baseline exceeds official budget | solution + evaluator wrapper | fixed locally with sample budget and CHECK fallback |
| `scp-maze-exit-frontier-cs-algorithmic-85` | interactive evaluator timeout around 120s | compiled entrypoint and navigation can overrun official budget | solution + evaluator wrapper | fixed locally with multi-case handling and move/query budgets |
| `sorted-mode-array-frontier-cs-algorithmic-257` | interactive evaluator timeout around 120s | singleton query scan can reach 200k round trips | solution + evaluator wrapper | fixed locally with 4096-query sampled anchors and sorted expansion |
| `steiner-tree-reconstruction-frontier-cs-algorithmic-89` | interactive evaluator timeout around 120s | all-pairs membership and memory are too large | solution + evaluator wrapper | fixed locally with exact cutoff and star-tree fallback |
| `substring-ab-program-frontier-cs-algorithmic-23` | separated evaluator timeout or resource failure around 135s | official-shaped cases `13..22` exceed the current evaluator budget | baseline/checker | intentionally deferred until a stronger official-scale baseline or checker path exists |
| `tree-centroid-guess-frontier-cs-algorithmic-54` | interactive evaluator timeout around 180s | full distance sweeps can exceed budget | solution + evaluator wrapper | fixed locally with query precheck fallback and non-aborting guesses |
| `weighted-tree-distances-frontier-cs-algorithmic-10` | interactive evaluator timeout around 120s | all-pairs queries and cubic edge detection are too slow | solution + evaluator wrapper | fixed locally with exact cutoff and capped root-distance star fallback |
| `world-map-frontier-cs-algorithmic-6` | quick participant failure | baseline may emit invalid candidate or crash on awkward graphs | solution | fixed locally with bounded grid construction and fallback for impossible disconnected edge-bearing graphs |

## Verification Notes

- [x] Python syntax checks passed for touched Python evaluator wrappers and touched Python solutions.
- [x] C++ compile checks passed for touched C++ solutions.
- [x] `cargo test -p agentics-runner` passed, including failed participant run metadata persistence.
- [x] Subagent public/interactor smokes passed for disk probing, hidden circuit gates, world map, sorted mode, Steiner tree, tree centroid, weighted tree, maximum position, mineral pairing, online MST, poker action, SCP maze, cycle chord, hidden cycle, greedy tree, demagnetized magnets, and ink pen.
- [x] Local validation passed through the Agentics CLI for each touched solution.
  - Re-ran all 25 affected baselines with `sudo`, the dedicated test Docker daemon at `unix:///srv/agentics-test/docker.sock`, and isolated storage under `/srv/agentics-test/tmp/failed-baseline-validation`.
  - The prior `Operation not permitted` blocker was caused by the non-root local validation workspace setup, not by Docker or challenge behavior.
  - All 25 public validations completed with evaluator-authored results: `cycle-chord-identification-frontier-cs-algorithmic-16` 99.875, `dango-stick-grouping-frontier-cs-algorithmic-217` 100.0, `demagnetized-magnets-frontier-cs-algorithmic-255` 36.0, `disk-probing-frontier-cs-algorithmic-60` 87.5, `dna-matching-probability-frontier-cs-algorithmic-121` 100.0, `graph-isomorphism-edge-match-frontier-cs-algorithmic-180` 100.0, `greedy-tree-blackbox-frontier-cs-algorithmic-93` 100.0, `hamiltonian-path-frontier-cs-algorithmic-5` 100.0, `hidden-circuit-gates-frontier-cs-algorithmic-101` 100.0, `hidden-cycle-length-frontier-cs-algorithmic-14` 98.529886, `impartial-game-graph-frontier-cs-algorithmic-231` 100.0, `induced-triple-graph-frontier-cs-algorithmic-120` 35.39823, `ink-pen-selection-frontier-cs-algorithmic-68` 100.0, `maximum-position-permutation-frontier-cs-algorithmic-17` 96.428571, `mineral-pairing-frontier-cs-algorithmic-125` 99.9992, `modulo-collision-size-frontier-cs-algorithmic-36` 100.0, `online-mst-decisions-frontier-cs-algorithmic-153` 100.0, `poker-action-seeds-frontier-cs-algorithmic-143` 0.0, `scp-maze-exit-frontier-cs-algorithmic-85` 100.0, `sorted-mode-array-frontier-cs-algorithmic-257` 100.0, `steiner-tree-reconstruction-frontier-cs-algorithmic-89` 100.0, `substring-ab-program-frontier-cs-algorithmic-23` 100.0, `tree-centroid-guess-frontier-cs-algorithmic-54` 100.0, `weighted-tree-distances-frontier-cs-algorithmic-10` 100.0, and `world-map-frontier-cs-algorithmic-6` 51.851844.
- [ ] Representative separated timeout case smoked in dev or rehearsal.
- [ ] Representative piped timeout case smoked in dev or rehearsal.
- [ ] Representative protocol bug case smoked in dev or rehearsal.
- [ ] Representative corrected positive-score case smoked in dev or rehearsal.
- [x] `agentics challenge-creator check` passed for all 25 affected challenge directories.
- [ ] Repaired CPU baselines were resubmitted to production one at a time.
  - Production resubmission should happen after the updated runner is deployed and affected challenge bundles are refreshed so production uses the hardened evaluator wrappers.
- [x] Final table below was filled in for local repair status.

## Second Production Pass Interactive Failures

The later CPU baseline pass found eight piped-stdio baselines that could still wedge official Testlib sessions before a structured result reached persistence. These are default-deferred until an official-private replay proves the solution and wrapper pair is protocol-safe.

| Challenge | Current action |
| --- | --- |
| `adaptive-impostor-search-frontier-cs-algorithmic-245` | Default-deferred; wrapper timeout guard added. |
| `disk-probing-frontier-cs-algorithmic-60` | Default-deferred; wrapper timeout guard added. |
| `heap-tree-sum-frontier-cs-algorithmic-209` | Default-deferred; wrapper timeout guard added. |
| `hidden-circuit-gates-frontier-cs-algorithmic-101` | Default-deferred; wrapper timeout guard added. |
| `induced-triple-graph-frontier-cs-algorithmic-120` | Default-deferred; wrapper timeout guard rechecked because older timeout handling still surfaced opaque production failure. |
| `inversion-recovery-frontier-cs-algorithmic-73` | Default-deferred; wrapper timeout guard added. |
| `mineral-pairing-frontier-cs-algorithmic-125` | Default-deferred; wrapper timeout guard rechecked because older timeout handling still surfaced opaque production failure. |
| `space-thief-stars-frontier-cs-algorithmic-63` | Default-deferred; wrapper timeout guard added. |

`substring-ab-program-frontier-cs-algorithmic-23` is also default-deferred, but for a different reason: official-shaped separated-evaluator cases `13..22` exceed the current evaluator budget and need a stronger official-scale baseline or checker path. `colored-ball-pole-sorting-frontier-cs-algorithmic-142` remains the older default-deferred constructive baseline.

## Final Outcome

| Outcome | Challenges |
| --- | --- |
| Completed after first repair wave | `cycle-chord-identification-frontier-cs-algorithmic-16`, `demagnetized-magnets-frontier-cs-algorithmic-255`, `dna-matching-probability-frontier-cs-algorithmic-121`, `graph-isomorphism-edge-match-frontier-cs-algorithmic-180`, `greedy-tree-blackbox-frontier-cs-algorithmic-93`, `hamiltonian-path-frontier-cs-algorithmic-5`, `hidden-cycle-length-frontier-cs-algorithmic-14`, `ink-pen-selection-frontier-cs-algorithmic-68`, `maximum-position-permutation-frontier-cs-algorithmic-17`, `modulo-collision-size-frontier-cs-algorithmic-36`, `online-mst-decisions-frontier-cs-algorithmic-153`, `poker-action-seeds-frontier-cs-algorithmic-143`, `scp-maze-exit-frontier-cs-algorithmic-85`, `sorted-mode-array-frontier-cs-algorithmic-257`, `steiner-tree-reconstruction-frontier-cs-algorithmic-89`, `tree-centroid-guess-frontier-cs-algorithmic-54`, `weighted-tree-distances-frontier-cs-algorithmic-10`, `world-map-frontier-cs-algorithmic-6` |
| Structured failed with diagnostics after first repair wave | `dango-stick-grouping-frontier-cs-algorithmic-217`, `impartial-game-graph-frontier-cs-algorithmic-231` |
| Default-deferred pending official-protocol-safe replay | `adaptive-impostor-search-frontier-cs-algorithmic-245`, `disk-probing-frontier-cs-algorithmic-60`, `heap-tree-sum-frontier-cs-algorithmic-209`, `hidden-circuit-gates-frontier-cs-algorithmic-101`, `induced-triple-graph-frontier-cs-algorithmic-120`, `inversion-recovery-frontier-cs-algorithmic-73`, `mineral-pairing-frontier-cs-algorithmic-125`, `space-thief-stars-frontier-cs-algorithmic-63` |
| Intentionally deferred for stronger algorithms | `colored-ball-pole-sorting-frontier-cs-algorithmic-142`, `substring-ab-program-frontier-cs-algorithmic-23` |
