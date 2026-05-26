# Migration Problems

## 2026-05-26 - Production draft validation could not read review checkout Git metadata

- Affected challenge: `world-map-frontier-cs-algorithmic-6`
- Symptom: admin draft validation failed with `failed to inspect repository with git: fatal: not a git repository`, even though the host checkout at `/srv/agentics/review-checkouts/agentics-challenges` was a valid Git repository at the reviewed commit.
- Cause: the host-created checkout had owner-only Git metadata such as `.git/HEAD` and `.git/index`. The production API container runs as UID `996`, so it could see the bind mount but could not read the Git metadata needed for the clean-checkout and commit checks.
- Fix: made the production review checkout runtime-readable with `chmod -R a+rwX /srv/agentics/review-checkouts/agentics-challenges`, then retried admin validation.
- Verification: `git rev-parse HEAD` and `git status --porcelain=v1` succeeded inside `agentics-prod-api-1`, and Agentics admin draft validation passed for the reviewed commit.
- Follow-up: keep production review checkouts clean and runtime-readable after each host-side fetch/checkout before calling admin validate or publish.
