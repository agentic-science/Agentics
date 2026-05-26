# Frontier-CS Migration Problem Log

## Production setup before the 10-challenge batch

### Docker BuildKit could not use the default bridge

- Symptom: `just compose-prod-build` failed during image build with a Docker bridge error for `docker0`.
- Cause: the host Docker daemon did not have the default `docker0` bridge available, so BuildKit `RUN` steps using the default build network could not start.
- Fix: production app and web image builds now use `build.network: host`.
- Verification: `just compose-prod-build` completed successfully.
- Affected challenge: production setup before the batch.

### Next.js build could not infer the workspace root

- Symptom: the production web image build failed under Next.js/Turbopack because the inferred workspace root followed Bun workspace symlinks outside `frontends/web`.
- Cause: the frontend did not declare an explicit Turbopack root for the repo workspace.
- Fix: `frontends/web/next.config.ts` now sets `turbopack.root` to the repository root.
- Verification: `cd frontends/web && bun run build` and `just compose-prod-build` completed.
- Affected challenge: production setup before the batch.

### Private-bundle restore required a host-relative env file in the container

- Symptom: `just compose-prod-restore-private-bundles` failed because the restore container tried to read `deploy/compose/env/rustfs-private-backup.env`.
- Cause: the restore service already received the backup values through Compose `env_file`, but the copier treated the default env-file path as mandatory inside the container.
- Fix: the copier now treats missing env files as empty overlays and falls back to process environment values.
- Verification: `cargo test -q -p agentics-ops private_bundle_backups` passed.
- Affected challenge: production setup before the batch.

### Private-bundle restore could not reach the backup RustFS through host gateway

- Symptom: the restore service could not connect to the backup RustFS through `host.docker.internal:9100`.
- Cause: the backup RustFS was reachable on the host, but the production Compose container could not use the host gateway path reliably on this Docker setup.
- Fix: `agentics-compose-prod restore-private-bundles` temporarily connects the backup RustFS container to the production Compose network and the restore service addresses it as `agentics-rustfs-private-backup:9000`.
- Verification: `just compose-prod-restore-private-bundles` copied and SHA-256 verified 13 objects, 522,722 bytes, into production RustFS.
- Affected challenge: production setup before the batch.

### API ignored `AGENTICS_CHALLENGES_ROOT`

- Symptom: the production API restarted with `Permission denied` during startup under the non-root runtime user.
- Cause: the app image set `AGENTICS_CHALLENGES_ROOT`, but config parsing ignored it, so the API always tried to seed bundled sample challenges from the default root.
- Fix: config parsing now honors `AGENTICS_CHALLENGES_ROOT`, and production Compose defaults it to `/app/no-seeded-challenges`.
- Verification: `cargo test -q -p agentics-config raw_app_env_deserializes_prefixed_values`, `cargo check -q -p api-server -p agentics-config`, `just compose-prod-build`, `just compose-prod-up`, and `curl -fsS http://127.0.0.1:3100/healthz` passed.
- Affected challenge: production setup before the batch.

### Worker profile probe could not see prepared host quota state from the container

- Symptom: after passwordless `sudo` prepared the production DGX storage roots, the CPU worker still failed closed because `/srv/agentics/docker-data-root` was not visible in the worker container and phase quota slots reported `missing project quota row`.
- Cause: production Compose did not bind-mount the prepared Docker data root into worker/check containers. Separately, `xfs_quota` cannot read project quota rows for bind-mounted loop devices from this container namespace; it exits successfully with an empty report and `cannot setup path for mount ... No such device or address`.
- Fix: production Compose now mounts `AGENTICS_DGX_DOCKER_DATA_ROOT` into worker/check containers, and the DGX profile checker treats the known container-only `xfs_quota` row-inspection failure as inconclusive while still requiring slot metadata, writable slots, XFS `prjquota` mounts, and strict mutating quota-exhaustion probes.
- Verification: `sudo ... agentics-prepare-dgx-spark-storage` prepared `/srv/agentics/docker-data-root` and the phase slot quotas; `cargo test -q -p agentics-ops check_dgx_spark_profile` passed.
- Affected challenge: production setup before the batch.

## Knight tour path migration

### Production API could not validate a reviewed challenge checkout

- Symptom: admin draft validation for `knight-tour-path-frontier-cs-algorithmic-109` failed with `repository_path does not exist or cannot be resolved`.
- Cause: production Compose ran the API inside a container without the checked-out `agentics-challenges` repository mounted, but draft validation and publishing intentionally inspect the reviewed Git checkout from inside the API process. After adding the mount, the same flow exposed that the API image did not include the `git` binary required by the validator and that mounting the local submodule checkout directly can leave unreadable Git metadata behind for the production runtime UID. The standalone review checkout also needs world-readable files because the operator checkout can create files and `.git` metadata with a restrictive umask.
- Fix: production Compose now bind-mounts a standalone, runtime-readable `agentics-challenges` checkout from `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT` into the API container at `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT`, configures Git safe-directory handling for the mounted checkout, disables optional Git locks for read-only validation commands, and includes `git` in the app runtime image.
- Verification: after `chmod -R a+rX` on the standalone review checkout, the API container could run `git rev-parse`, `git status`, and draft validation successfully.
- Affected challenge: `knight-tour-path-frontier-cs-algorithmic-109`.

### Production runner used a Docker daemon without layer-quota support

- Symptom: the first validation run for `knight-tour-path-frontier-cs-algorithmic-109` failed before executing the solution because Docker rejected container creation with `--storage-opt is supported only for overlay over xfs with 'pquota' mount option`.
- Cause: production Compose was still pointed at the host `/var/run/docker.sock`, whose daemon stores overlay2 data under `/var/lib/docker` on ext4. The prepared `/srv/agentics/docker-data-root` XFS project-quota mount was present but was not the data root of the daemon used by the worker.
- Fix: started an operator-managed production Docker daemon at `unix:///srv/agentics/docker.sock` with data root `/srv/agentics/docker-data-root`, updated the production rehearsal env to point `AGENTICS_DOCKER_HOST` and `AGENTICS_DOCKER_SOCKET_PATH` at that socket, and fixed production Compose so custom Docker sockets are mounted at the same in-container path and exposed through `DOCKER_HOST` for local checks.
- Verification: `docker --host unix:///srv/agentics/docker.sock run --rm --storage-opt size=64m busybox:1.36 ...` succeeded, the worker host probe passed, and `just compose-prod-check` passed.
- Affected challenge: `knight-tour-path-frontier-cs-algorithmic-109`.

### Generated evaluator used an old summary contract

- Symptom: after runner setup, build, run, and separated evaluator all executed, Agentics rejected `result.json` with `missing field passed`.
- Cause: the generated `knight-tour-path-frontier-cs-algorithmic-109` evaluator emitted `validation_summary` and `official_summary` with `score`, `valid_cases`, and `total_cases`, but the current Agentics `ScoreSummary` contract requires `score`, `passed`, and `total`.
- Fix: PR #20 updated the evaluator summary to include `passed` and `total`; the temporary migration generator was updated so later generated Frontier-CS challenge evaluators emit the current shape.
- Verification: local public smoke showed the corrected summary shape, and production validation plus official submission completed successfully after republishing the local rehearsal challenge.
- Affected challenge: `knight-tour-path-frontier-cs-algorithmic-109`.
