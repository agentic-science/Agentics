# Migration Problems

## 2026-05-25: DGX profile probe rejected valid XFS quota slots

- Challenge: pre-migration production platform setup.
- Role: platform admin/operator.
- Symptom: `agentics-worker.service` restarted repeatedly during startup because `agentics-check-dgx-spark-profile` reported every bounded writable slot as `missing project quota row`.
- Root cause: the storage preparation had created slot metadata and XFS project inode limits, and `xfs_quota report -p -i -n` showed the expected `#<project_id>` rows when run as root. The profile checker instead ran `xfs_quota quota -p -i -n -N <project_id>`, whose output starts with the filesystem device path rather than the project id. The checker's parser was written for the report output, so it never found the row. After switching to the report command, the service-user startup probe still could not inspect quota rows because `xfs_quota` returns `XFS_GETQUOTA: Operation not permitted` for unprivileged users on this host.
- Fix: changed the DGX profile checker to query `xfs_quota report -p -i -n`, matching the parser and the profile-check unit test fixture. The checker now treats unprivileged `XFS_GETQUOTA` denial as inconclusive for the worker startup path while still enforcing direct quota-row checks when the operator runs the profile check as root.
- Verification: `cargo test -p agentics-ops check_dgx_spark_profile::tests` passes. After rebuilding/copying the checker, `agentics-worker.service` passed its host profile probe.

## 2026-05-25: DGX profile Docker canary lost failing container logs

- Challenge: pre-migration production platform setup.
- Role: platform admin/operator.
- Symptom: after the quota-slot check was fixed, `agentics-worker.service` still restarted because the DGX profile checker reported `Docker container wait error:` for the writable-layer and bounded-slot quota canaries. The message was empty and did not include the container's `dd` stderr.
- Root cause: the checker used Docker's wait stream through Bollard. On this Docker/Bollard combination, canary containers that exited with code 1 produced an empty wait-stream error even though manual `docker run` showed the expected `No space left on device` stderr and the container state recorded the correct exit code.
- Fix: changed the canary wait path to poll `inspect_container` until the container exits, then collect logs separately. This keeps the same timeout and cleanup behavior while preserving the real exit code and logs.
- Verification: `cargo test -p agentics-ops check_dgx_spark_profile::tests` passes. After rebuilding/copying the checker, the DGX profile canary reports the expected writable-layer and bounded-slot quota exhaustion, and `agentics-worker.service` is active.

## 2026-05-25: Source-tree web release copy missed Bun workspace store

- Challenge: pre-migration production platform setup.
- Role: platform admin/operator.
- Symptom: `agentics-web.service` failed first with `Permission denied` for `/opt/agentics/current/bin/bun`, then with `next: command not found`.
- Root cause: the initial release staging copied a `bun` symlink into `/opt/agentics/current/bin`, but the symlink pointed into `/home/maplespark`, which the service user cannot traverse. After copying the `bun` binary, the web service still failed because `frontends/web/node_modules` uses Bun workspace symlinks that resolve through the repository-level `node_modules/.bun`, which had not been copied into the release directory.
- Fix: copied the real `bun` executable into `/opt/agentics/current/bin` and copied the repository-level Bun workspace dependency store into `/opt/agentics/current/node_modules`.
- Verification: `agentics-web.service` is active and `curl -fsSI http://127.0.0.1:3001/` returns HTTP 200.

## 2026-05-25: DGX retained-run slot pool was smaller than the run-manifest cap

- Challenge: `polyomino-packing-frontier-cs-algorithmic-0`.
- Role: platform admin/operator during official submission smoke.
- Symptom: the official submission failed after only four solution invocations even though the platform run-manifest cap had been raised to 100 and a local 70-run Docker validation passed.
- Root cause: the production `xfs-project-quota-slots` runner keeps each solution-run output tree leased until the separated evaluator starts. The DGX storage profile still prepared only four slots per phase and size class, so a 70-run separated-evaluator benchmark exhausted the `solution-run` slot class on run 5.
- Fix: aligned DGX storage preparation and profile-check defaults with the 100-run platform contract by raising `AGENTICS_DGX_PHASE_SLOTS_PER_CLASS` from 4 to 100 in the Rust ops defaults, env examples, and docs. The production storage preparation was rerun to create the additional root-prepared XFS project-quota slots.
- Verification: after rerunning storage preparation and restarting the worker, the `polyomino-packing-frontier-cs-algorithmic-0` official smoke submission completed all 70 direct runs successfully.
