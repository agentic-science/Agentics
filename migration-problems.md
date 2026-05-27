# Migration Problems

## 2026-05-26 - Production draft validation could not read review checkout Git metadata

- Affected challenge: `world-map-frontier-cs-algorithmic-6`
- Symptom: admin draft validation failed with `failed to inspect repository with git: fatal: not a git repository`, even though the host checkout at `/srv/agentics/review-checkouts/agentics-challenges` was a valid Git repository at the reviewed commit.
- Cause: the host-created checkout had owner-only Git metadata such as `.git/HEAD` and `.git/index`. The production API container runs as UID `996`, so it could see the bind mount but could not read the Git metadata needed for the clean-checkout and commit checks.
- Fix: made the production review checkout runtime-readable with `chmod -R a+rwX /srv/agentics/review-checkouts/agentics-challenges`, then retried admin validation.
- Verification: `git rev-parse HEAD` and `git status --porcelain=v1` succeeded inside `agentics-prod-api-1`, and Agentics admin draft validation passed for the reviewed commit.
- Follow-up: keep production review checkouts clean and runtime-readable after each host-side fetch/checkout before calling admin validate or publish.

## 2026-05-26 - Worker A evaluator summaries missed Agentics `ScoreSummary` fields

- Affected challenge: `interval-dag-computer-frontier-cs-algorithmic-7`
- Symptom: the challenge published successfully, but the production smoke submission failed during official evaluation with redacted private-benchmark details.
- Cause: Worker A's evaluator template emitted `official_summary` with `accepted_cases` and `total_cases`, but omitted the Agentics-required `passed` and `total` fields. Local checker smoke passed because it did not deserialize `result.json` through the Agentics runner contract.
- Fix: patched Worker A generated evaluator files and merged a focused fix for the already-published problem 7 bundle. For the rehearsal deployment, repacked and replaced the stored runtime/public bundle objects with the corrected evaluator.
- Verification: the corrected evaluator produced `official_summary.passed` and `official_summary.total`; a new production smoke submission for problem 7 completed successfully.
- Follow-up: local migration smoke should validate evaluator `result.json` against the Agentics contract, not only run the source checker.

## 2026-05-27 - Checker diagnostics exceeded result log limits

- Affected challenge: `parenthesis-sequence-transformation-frontier-cs-algorithmic-205`
- Symptom: the challenge published successfully, but the production smoke submission failed during official evaluation and stored no aggregate metrics.
- Cause: invalid official outputs caused the Frontier-CS checker to print a very large expected-output diagnostic. The evaluator copied that diagnostic into `result.json` logs, which exceeded Agentics result limits before the zero-score result could be persisted.
- Fix: capped per-run checker messages and result logs in the evaluator, merged the repair, and repacked the already-published production public/private bundle objects with the corrected evaluator.
- Verification: local official evaluation emitted a 1.9 KB `result.json`, and the replacement production smoke submission `71f30c41-549b-4d73-b201-2a4fd03e9c95` completed with score `0.0`.
- Follow-up: migration smoke tests should include deliberately bad official outputs and enforce `result.json` size/log limits.

## 2026-05-27 - Smoke solution wrote excessive stdout

- Affected challenge: `interval-set-merge-frontier-cs-algorithmic-225`
- Symptom: the challenge published successfully, but the first production smoke submission failed during official evaluation before metrics were persisted.
- Cause: the simple baseline generated about 28 MB of stdout for each official run and spent more than seven seconds per run. That made it a poor production smoke solution even though the evaluator could score the output locally.
- Fix: replaced the test solution with a tiny invalid-output baseline so the official evaluator can complete and record a zero-score result.
- Verification: the repaired smoke solution emits a two-byte output, and production smoke submission `19695abc-93bc-4909-bb89-bbf48e70355c` completed with score `0.0`.
- Follow-up: generated smoke solutions should prefer small, cheap, clearly invalid outputs over large constructive attempts when a valid baseline is not necessary.
