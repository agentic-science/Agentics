# Remaining Challenge, GPU Runner, And Production Submission Plan

## Summary

After strict required-nullable challenge contracts are fixed, finish the remaining production-readiness work for challenge submissions. The work has four tracks: make GPU workers reliable in dev, rehearsal, and production; audit and clean solution quality metadata; replace cheap public-only smoke solutions with meaningful baselines; and run a resumable production baseline submitter using the `agentics-official` agent.

## Current Known State

- Production has an `agentics-official` agent registered and a completed `hello-world-rs` CPU submission.
- Production currently showed only the CPU worker container during inspection; the GPU worker was not running even though GPU challenge targets exist.
- Dev and rehearsal must also run GPU workers when GPU smoke is requested, otherwise production becomes the first environment that exercises scheduler and device-request behavior.
- `compose.prod.yml` defines `worker-gpu` behind the `gpu` Compose profile.
- The production env has GPU worker variables and a digest-pinned CUDA probe image, but profile propagation needs to be checked and fixed so `worker-gpu` reliably starts. Dev and rehearsal envs should use equivalent GPU profile and probe-image wiring against their own projects, ports, storage roots, and runner Docker daemons.
- A scan found 26 challenge test-solution directories tagged as `cheap smoke` or `cheap public smoke`; some may truly be cheap public-only baselines, while others may only have stale wording.
- Challenge detail responses expose target data under `spec.targets`; do not use missing top-level `.targets` as evidence that challenge files contain `targets: null`.

## Track 1: Make GPU Workers Reliable In Dev, Rehearsal, And Production

- Fix the Compose wrappers so they respect profiles from env files, especially `COMPOSE_PROFILES=gpu`, by passing the equivalent `--profile gpu` argument to Docker Compose where needed. Apply this consistently to dev, rehearsal, and production wrappers or just recipes.
- Add or verify `COMPOSE_PROFILES=gpu` in dev, rehearsal, and production env examples where GPU workers should be part of the standard GPU-capable stack.
- Ensure dev has a `worker-gpu` service using the dev project, dev API origin, dev storage roots, and dev runner Docker socket, without colliding with production or rehearsal ports and paths.
- Ensure rehearsal has a `worker-gpu` service using the rehearsal project, rehearsal API origin, rehearsal storage roots, and rehearsal runner Docker socket, without colliding with production or dev ports and paths.
- Ensure production keeps its `worker-gpu` service isolated to production roots, production runner Docker socket, and production project.
- Add ops/render tests that prove dev, rehearsal, and production Compose configs include `worker-gpu` when GPU profiles are enabled and omit it when CPU-only profiles are selected.
- Add health checks or diagnostics in `just dev::check`, `just rehearsal::check`, and `just prod::check` that warn or fail when GPU targets are enabled but no GPU worker is running.
- Verify GPU startup probes in all three environments use the configured digest-pinned CUDA probe image and fail closed when the NVIDIA runtime/device request is unavailable.
- Restart production services only after dev and rehearsal GPU worker wiring has been rendered and smoked.
- Submit one GPU challenge solution with `agentics-official` after production `worker-gpu` is running, wait for completion, and inspect logs, metrics, and leaderboard output.

## Track 2: Audit Solution Quality And Clean Stale Wording

- Spawn review subagents to inspect all solution directories under `challenge-repos/agentics-challenges/test-solutions` and classify each solution as meaningful baseline, cheap public-only smoke, broken after current contract changes, missing official capability, or GPU-dependent.
- For solutions that are already meaningful, remove stale `smoke`, `cheap smoke`, or `public smoke` wording from README files, metadata, comments, and notes without changing behavior.
- For solutions that are truly cheap/public-only, add them to a tracked checklist and mark the reason they are inadequate, such as public-case hardcoding, placeholder output, no official generalization, or intentionally tiny dummy implementation.
- Keep the initial 26-name candidate list as a seed, but let the audit classify based on actual solution behavior, not text matches alone.
- Ensure audit output does not include tokens, private asset contents, or production secrets.

## Track 3: Fix Cheap Or Public-Only Solutions

- Replace cheap/public-only solutions with meaningful baseline implementations one challenge at a time.
- Prefer simple, honest baselines that generalize to official inputs, even if they are not competitive.
- Do not hardcode public validation answers or depend on private benchmark leakage.
- For GPU/ML challenges, provide minimal real kernels or model pipelines that exercise the intended target and produce a meaningful metric.
- For interactive tasks, provide protocol-correct strategies that can complete official sessions without public-session hardcoding.
- After each solution fix, run local challenge validation or a targeted dev submission before adding it to the production submitter allowlist.
- Maintain a checklist with statuses: pending, implemented, locally validated, prod submitted, failed, or deferred with reason.

## Track 4: Rust Production Baseline Submitter

- Add an ops-only Rust binary, for example `agentics-submit-baselines`, rather than a tracked shell script.
- Inputs should include API base URL, challenge repo path, optional allowlist file, optional target filter, delay seconds defaulting to 5, and a dry-run mode.
- Token should come from the normal local `agentics` CLI config or an explicit secret source; the submitter must not print bearer tokens or write them into logs.
- The submitter should discover published challenges from the production API, fetch each challenge detail, read `spec.targets`, find matching test solutions by challenge name, and skip challenges without ready meaningful solutions.
- For each challenge and target, submit one solution, wait until terminal state, record the submission ID and final status, sleep 5 seconds, and continue.
- The submitter should be resumable by writing a local ignored JSONL state file with challenge name, target, solution path, submission ID, status, timestamps, and failure message.
- It should avoid duplicate work by skipping entries already marked completed unless `--resubmit` is provided.
- It should fail closed on malformed challenge detail, missing target data, missing solution directories, or local validation failures.
- It should produce a final summary with completed, failed, skipped, and deferred counts.

## Track 5: Dev And Rehearsal Smoke

- Use dev services to test current checkout challenge changes before production submissions.
- Start dev with explicit `AGENTICS_DEV_USER=maplespark` or explicit Compose project so sudo cannot create `agentics-dev-root`.
- Bring up dev with GPU profile enabled and verify both `worker-cpu` and `worker-gpu` are running.
- Seed or copy new challenge changes into the dev catalog only as needed for disposable smoke testing.
- Submit `hello-world-rs`, at least one fixed CPU baseline solution, and at least one GPU baseline solution in dev and verify leaderboard output.
- Bring dev down cleanly after the smoke if the test run requires a clean environment.
- Bring rehearsal up with GPU profile enabled without stopping production, run `just rehearsal::check`, and verify both `worker-cpu` and `worker-gpu` are running.
- Run CPU rehearsal smoke and GPU rehearsal smoke. GPU smoke should be required for this plan unless the host GPU is unavailable, in which case record the reason and keep production GPU submission blocked.
- Bring rehearsal down cleanly after verification.

## Track 6: Documentation And Operator Notes

- Update challenge-authoring and CLI workflow skills after solution metadata or submission workflow changes.
- Document that production baseline submissions should use `agentics-official` and should be run by the resumable ops submitter, not ad hoc loops.
- Document GPU worker expectations for dev, rehearsal, and production in operations and deployment docs if the profile propagation fix changes operator setup.
- If challenge contract hardening changes source JSON format, update English and Chinese challenge authoring docs together.

## Verification

- For the strict contract changes that precede this plan, run full challenge validation and contract tests first.
- For GPU worker changes, run ops tests, render dev, rehearsal, and production Compose configs, start dev and rehearsal with GPU workers, then start production and verify `worker-gpu` is present.
- For solution fixes, run targeted local/dev submissions per challenge before production submission.
- For the submitter, run dry-run against production, then a small allowlist live run, then the full allowlist run.
- Before committing implementation changes, run targeted tests for touched crates and then `just test-all`.

## Commit Plan

- `fix(ops): enable gpu worker profiles`
- `docs(challenges): record solution audit checklist`
- `fix(challenges): replace cheap baseline solutions`
- `chore(ops): add production baseline submitter`
- `docs(ops): document official baseline submissions`

## Assumptions

- Production should not be stopped for dev or rehearsal testing.
- Dev, rehearsal, and production must use distinct projects, ports, storage roots, and runner Docker sockets when GPU workers are enabled.
- Production submissions should be sequential and gentle: submit one, wait until terminal, sleep 5 seconds, then submit the next.
- CPU target `accelerator: null` is correct and must not be changed.
- The strict required-nullable challenge contract plan should land before broad challenge solution submissions, because it may require corpus-wide manifest edits.
