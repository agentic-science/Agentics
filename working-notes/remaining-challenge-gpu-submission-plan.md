# Remaining Challenge, GPU Runner, And Production Submission Plan

## Summary

After strict required-nullable challenge contracts are fixed, finish the remaining production-readiness work for challenge submissions. The work has four tracks: make GPU workers reliable in dev, rehearsal, and production; audit and clean solution quality metadata; replace cheap public-only smoke solutions with meaningful baselines; and run a resumable production baseline submitter using the `agentics-official` agent.

## Current Known State

- Production has an `agentics-official` agent registered and a completed `hello-world-rs` CPU submission.
- Strict required-nullable challenge contracts have landed, and the corpus validates with `agentics challenge-creator check`.
- Production previously showed only the CPU worker container during inspection; the GPU worker was not running even though GPU challenge targets exist.
- Dev and rehearsal must also run GPU workers by default on this NVIDIA host, otherwise production becomes the first environment that exercises scheduler and device-request behavior.
- `compose.prod.yml` defines `worker-gpu` behind the `gpu` Compose profile.
- The production bug is profile propagation, not worker config alone: `AGENTICS_WORKER_ACCELERATORS=gpu` does not create `worker-gpu`; Compose needs `COMPOSE_PROFILES=gpu` or an explicit `--profile gpu`.
- Dev now needs the same dedicated runner Docker boundary as rehearsal and production so GPU runner behavior is tested before production.
- A scan found 26 challenge test-solution directories tagged as `cheap smoke` or `cheap public smoke`; some may truly be cheap public-only baselines, while others may only have stale wording.
- Challenge detail responses expose target data under `spec.targets`; do not use missing top-level `.targets` as evidence that challenge files contain `targets: null`.

## Implementation Checklist

- [x] Spawn subagents to audit solution quality and GPU/profile plumbing.
- [x] Add dev `worker-cpu` and `worker-gpu` services, with `worker-gpu` behind the `gpu` Compose profile.
- [x] Add a dedicated dev runner Docker daemon lifecycle so dev workers do not use production Docker or the system socket by accident.
- [x] Pass Compose profiles explicitly through the production/rehearsal wrapper, derived from `COMPOSE_PROFILES` in the process or env file.
- [x] Fail production/rehearsal wrapper startup when legacy `AGENTICS_WORKER_ACCELERATORS=gpu` is set without activating the `gpu` Compose profile.
- [x] Make `just dev::check` and `just prod::check`/`just rehearsal::check` fail when the expected GPU worker service is missing.
- [x] Make the rehearsal heartbeat check scan all worker heartbeats instead of only the first worker.
- [x] Render dev, rehearsal, and production Compose service sets in CPU and GPU modes.
- [x] Start dev with GPU workers, run `just dev::check`, and bring dev down.
- [x] Start rehearsal with GPU workers, run `just rehearsal::check`, and bring rehearsal down.
- [x] Restart production with GPU profile active and verify `worker-gpu`.
- [x] Update migrated Frontier-CS separated evaluators to read challenge-specific run data from `metadata`, matching the strict required-nullable source manifests.
- [x] Update the Frontier-CS private asset refresh generator so regenerated private overlays emit strict run/session manifests.
- [x] Normalize stale persistent private-bundle backup objects and restore the repaired overlays into production object storage.
- [x] Verify production GPU worker scheduling by submitting GPU challenges with `agentics-official`.
- [x] Get a successful GPU challenge result after the host GPU is no longer saturated by non-Agentics processes.
- [x] Clean stale solution wording where the solution is already meaningful.
- [x] Record a production-submission solution audit checklist.
- [x] Replace cheap/public-only baseline solutions with meaningful baselines or explicitly defer them with challenge-specific reasons.
- [x] Add the resumable production baseline submitter.

## Solution Audit Snapshot

The 2026-06-13 solution audit initially reduced the broad not-submit-ready bucket to 68 explicitly deferred solutions. Follow-up baseline work upgraded 67 of those solutions; a later production baseline pass then found eight piped-stdio baselines that still need official-protocol-safe replay evidence, and `substring-ab-program-frontier-cs-algorithmic-23` was explicitly deferred for official-scale checker work. The default submitter now treats 238 checked-in solutions as submitter-ready and 10 as deferred.

Follow-up solution work upgraded `cube-sphere-packing-frontier-cs-algorithmic-48`, `functional-cycle-reach-frontier-cs-algorithmic-252`, `hamiltonian-path-frontier-cs-algorithmic-5`, and `signed-rooted-tree-frontier-cs-algorithmic-57` into meaningful baseline submissions.

The metadata review also cleared stale wording for official-capable baselines including `distinct-bakery-types-frontier-cs-algorithmic-141`, `line-recovery-frontier-cs-algorithmic-117`, `llm-sql-small-frontier-cs-llm-sql-small`, `llm-sql-large-frontier-cs-llm-sql-large`, `palindromic-grid-paths-frontier-cs-algorithmic-256`, `poker-action-seeds-frontier-cs-algorithmic-143`, `repaired-road-set-frontier-cs-algorithmic-253`, `snake-path-minima-frontier-cs-algorithmic-233`, `sorted-mode-array-frontier-cs-algorithmic-257`, `treasure-hunt-choices-frontier-cs-algorithmic-70`, and `world-map-frontier-cs-algorithmic-6`.

The remaining deferred set is documented in `working-notes/challenge-solution-baseline-audit.md` and mirrored in the `agentics-submit-baselines` default defer list. `colored-ball-pole-sorting-frontier-cs-algorithmic-142` still needs a stronger constructive algorithm under the 2,000,000-operation cap, `substring-ab-program-frontier-cs-algorithmic-23` needs official-scale checker or baseline work, and the eight piped-stdio entries need official-protocol-safe replay evidence before broad production submission resumes for them.

The GPU-dependent solution set for smoke and production scheduling checks is `cross-entropy-kernel`, `decoding-attn-kernel`, `flash-attn-kernel`, `fused-linear-ce-kernel`, `fused-linear-jsd-kernel`, `gdpa-attention-kernel`, `gemm-annoying`, `gemm-k-skewed`, `gemm-near-tile`, `gemm-rectangles`, `gemm-squares`, `gemm-transformer`, `group-gemm`, `mamba2-scan`, `mixed-gemm`, `qknorm`, `quant-dot-int4`, `ragged-attention`, `vector-add-2-24`, `vector-add-2-28`, and `vector-addition`.

## Live Verification Notes

- Dev GPU stack rendered correctly in CPU/GPU modes, then started with `worker-cpu` and `worker-gpu` through the dedicated dev runner Docker daemon. `just dev::check` passed, then dev and the dev runner daemon were brought down.
- Rehearsal GPU stack rendered correctly in CPU/GPU modes, then started with `worker-cpu` and `worker-gpu`. `just rehearsal::check` passed after API warmup, then rehearsal and the rehearsal runner daemon were brought down.
- Production was restarted with `COMPOSE_PROFILES=gpu` in the ignored production env, and `just prod::check` passed with both `worker-cpu` and `worker-gpu` running.
- The first production GPU submission attempt for `vector-addition-frontier-cs-vector-addition-2-20` did not reach runner scheduling because the production API still returned an older published challenge spec that the current CLI rejects as missing the required `solution.run` profile.
- Production API startup then exposed a second strict-contract gap: stale private-bundle backup objects still contained pre-refactor top-level run metadata such as `answer_text`, and later some older run manifests missed required nullable keys such as `stdin_json`.
- The persistent private-bundle backup store was mirrored into `target/private-bundle-backup-scan`, normalized locally, synced back to the backup RustFS store, restored into production with `just prod::restore-private-bundles --overwrite`, and verified with `just prod::check`.
- The production repair normalized 24 stale private overlay ZIPs in the object store. No private ZIP files were committed.
- After the object-store repair, production API health, public challenge catalog, web frontend, `worker-cpu`, `worker-gpu`, and GitHub egress checks all passed.
- Migrated Frontier-CS separated evaluators now read challenge-specific case data only from `run.metadata`, and `agentics challenge-creator check` passes for the published and dev challenge corpora.
- Production GPU jobs now reach the GPU worker and runner. Submissions for `vector-addition-frontier-cs-vector-addition-2-20`, `cross-entropy-kernel-frontier-cs-cross-entropy`, and validations for `vector-add-2-24-frontier-cs-vector-add-2-24` showed that scheduling and device requests are functioning, but results failed or scored zero because the host GPU was saturated.
- The first GPU validation failures exposed two separate setup/runtime issues. The dedicated runner Docker bridge lacked `DOCKER-USER` egress rules, so dependency installation timed out, and an interim system-Python workaround broke Triton compilation because `Python.h` was missing.
- The runner Docker egress fix now verifies scoped forwarding for the dedicated runner bridge during `prod::up`, `prod::check`, `rehearsal::up`, and `rehearsal::check`. `prod::check` also runs a real TLS probe from a runner container to `pypi.org:443`, which catches network-enabled evaluator setup failures before challenge jobs are claimed.
- The Triton/Python fix restored uv-managed evaluator Python for coexecuted GPU setup scripts after runner egress was repaired. `agentics challenge-creator check` passes for both the main and dev challenge corpora after the restore.
- The previous GPU blocker was host-level contention, not Agentics scheduling. `nvidia-smi` showed six long-running `python3` processes owned by user `tengteng`, each using roughly 15 to 17 GiB of GPU memory, while the dedicated runner Docker daemon had no live containers. Agentics should not kill those external jobs. A future scheduler improvement could add a minimum-free-GPU-memory admission probe before claiming GPU jobs.
- A later production status query confirmed completed official CUDA submissions for `vector-addition-frontier-cs-vector-addition-2-20` and `cross-entropy-kernel-frontier-cs-cross-entropy`, so the representative GPU result requirement is now fixed.
- GPU baseline solution README and manifest notes were cleaned where the implementation is an honest PyTorch, FlashInfer, or Triton baseline. Public-only and token-flood solutions intentionally keep honest smoke/public-only wording.
- `working-notes/challenge-solution-baseline-audit.md` now tracks ready baselines and solutions that should stay out of broad production submission until they are replaced.
- `agentics-submit-baselines` now provides the resumable production baseline submitter. It reads the normal local agent token source, skips known deferred solutions by default, defaults broad runs to `linux-arm64-cpu`, submits one challenge-target pair at a time, waits to terminal state, records JSONL progress, and sleeps five seconds before continuing.
- Submitter dry-run against production succeeded for the first three discovered challenge-target pairs.
- A live submitter smoke for `hello-world-rs/linux-arm64-cpu` succeeded with submission `b12fae78-18d5-45d6-92b9-53f04231e887`.
- A bounded CPU live pass submitted five challenges: `advertisement-rectangle-placement-frontier-cs-algorithmic-147` and `almost-monochromatic-cycle-frontier-cs-algorithmic-24` completed, while `adaptive-impostor-search-frontier-cs-algorithmic-245`, `adventure-rank-segmentation-frontier-cs-algorithmic-61`, and `average-permutation-frontier-cs-algorithmic-124` failed. This exposed that many still-labeled smoke solutions were not covered by the hardcoded defer list.
- `agentics-submit-baselines` now also defers local solutions whose README or manifest still says smoke/public-only unless `--include-deferred` is passed. `adaptive-impostor-search-frontier-cs-algorithmic-245` and `average-permutation-frontier-cs-algorithmic-124` were added to the default defer list because their live official runs failed despite not being marked smoke.
- A focused metadata review cleaned stale wording for official-capable baselines and expanded the default defer list to the 68 initially deferred solutions. A follow-up subagent pass upgraded 67 of those solutions and initially narrowed the default defer list to `colored-ball-pole-sorting-frontier-cs-algorithmic-142`. A later official baseline pass re-expanded the default defer list to 10 solutions: the older `colored-ball-pole-sorting-frontier-cs-algorithmic-142`, the explicitly deferred `substring-ab-program-frontier-cs-algorithmic-23`, and eight piped-stdio baselines awaiting official-protocol-safe replay.
- Broad CPU baseline submission can now run through `just prod::submit-baselines`. Broad GPU baseline submission can use the same submitter with an explicit CUDA target or `--all-targets` once the operator wants to spend the GPU time.

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
- If GPU submissions fail with CUDA out-of-memory while `worker-gpu` is healthy, inspect `nvidia-smi` and distinguish Agentics-owned runner containers from unrelated host processes before treating the failure as a platform bug.

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
- Inputs should include API base URL, challenge repo path, optional allowlist file, optional target filter, an explicit all-targets mode, delay seconds defaulting to 5, and a dry-run mode. Broad runs without target flags should submit CPU targets only.
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
