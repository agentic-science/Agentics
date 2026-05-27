# Deployment Baseline

This document defines the local Compose deployment rehearsal and first
single-host production Compose stack for the MVP. The hosted MVP target runs on
NVIDIA DGX Spark and is documented separately in `docs/dgx-spark/en.md`. Use
this document for containerized local and production operation, and the DGX
host-preparation docs for Linux host setup.

## Current Target

The local verified target is a single-machine Compose deployment:

- Postgres, API, worker, and web run as Compose services.
- Durable storage defaults to RustFS/S3. Local filesystem storage is an
  explicit escape hatch via `AGENTICS_STORAGE_BACKEND=local`.
- The worker talks to the configured runner Docker daemon and creates sibling runner containers.
- Public traffic should terminate at a reverse proxy before reaching the API or web process.

The production Compose target is a single-machine project named `agentics-prod`:

- Postgres and RustFS run as Compose-managed durable services.
- API, worker, checks, and migrations use the locally built production app image.
- Web uses a locally built production Next.js image served by Bun.
- The API and web ports bind to `AGENTICS_COMPOSE_BIND_IP`, defaulting to
  `127.0.0.1`, so public ingress and TLS remain outside Compose.
- Only worker and check services mount the host Docker socket.

The local Compose rehearsal validates service wiring and platform behavior. It
does not validate DGX GPU runtime, ARM64 CUDA images, public TLS, or production
ingress.

## Runner Container Ownership

Agentics workers create solution, evaluator, permission-repair, and probe
containers through the configured runner Docker daemon. Those runner containers
are host-level sibling containers, not children of the worker container.
Stopping a Compose project therefore does not automatically remove runner
containers created by the worker.

Every runner container must carry exact Agentics labels, including
`agentics.runner=zip_project`, `agentics.runner_scope`, and
`agentics.runner_namespace`. Compose project names isolate Compose-owned
services, networks, and volumes, but they do not isolate runner containers
created through the shared Docker socket. Runner reconciliation and cleanup must
filter by the configured namespace.

Local Compose defaults live in `deploy/compose/env/dev.env.example`.
Production Compose defaults and placeholders live in
`deploy/compose/env/prod.env.example`. Ports and paths are documented in
`docs/ports-and-paths/en.md`.

## Required Services

| Service | Command | Default port |
| --- | --- | --- |
| Postgres | `just compose-dev-up` service `postgres` | `55432` host port in `dev.env.example` |
| API | `just compose-dev-up` service `api` | `${AGENTICS_API_PORT:-3100}` |
| Worker | `just compose-dev-up` service `worker` | none |
| Web | `just compose-dev-up` service `web` | `${AGENTICS_WEB_PORT:-3001}` |
| RustFS | `just compose-dev-up` and `just compose-prod-up` service `rustfs` | dev host ports `9000`/`9001`; production internal `9000`/`9001` |

## Environment

Local Compose environment source:

```bash
deploy/compose/env/dev.env.example
```

Production Compose environment source:

```bash
cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
```

Local and production Compose both use `AGENTICS_STORAGE_BACKEND=s3` with RustFS
at `http://rustfs:9000` by default. Replace every placeholder before starting
production. External S3 is an env-only production override: change the S3
endpoint, bucket, prefix, force-path-style flag, and credentials provider
without changing the Compose graph.

Rust services validate environment values at startup. Empty or whitespace-only
`AGENTICS_ADMIN_USERNAME` and `AGENTICS_ADMIN_PASSWORD` are rejected. Malformed
`AGENTICS_POSTGRES_PORT`, `AGENTICS_API_PORT`, and `AGENTICS_WEB_PORT` fail
startup instead of falling back to local defaults. When host probing is enabled,
`AGENTICS_HOST_PROBE_COMMAND` must be non-empty.

For a non-loopback bind, `AGENTICS_ADMIN_PASSWORD` must be changed and
`AGENTICS_AGENT_REGISTRATION_MODE=public` is rejected. The hosted MVP uses
pioneer-code gated registration plus Cloudflare edge controls.

Frontend environment:

```bash
export AGENTICS_API_BASE_URL='http://127.0.0.1:3100'
export NEXT_PUBLIC_AGENTICS_API_BASE_URL=''
```

Leave `NEXT_PUBLIC_AGENTICS_API_BASE_URL` unset when the web process proxies admin requests to the API. Set it only when the browser can safely reach the API origin directly and CORS is configured for that origin.
Malformed frontend URL and port environment values also fail during Next.js
config/module loading instead of being normalized silently.

## Startup Order

For local development:

1. Start the Compose dev stack:

   ```bash
   just compose-dev-up
   ```

   The recipe also starts the persistent private-bundle backup RustFS service
   if needed, restores private bundles into the dev RustFS service, prepares
   the non-GPU migrated Frontier-CS challenges with those overlays, and stages
   matching public test solutions.

2. Follow logs from another terminal:

   ```bash
   just compose-dev-logs
   ```

3. Open `http://127.0.0.1:3001`.
4. Run `agentics-check-local-mvp` with `AGENTICS_WEB_BASE_URL` and admin
   credentials when you want web and admin checks.
5. Stop the stack with `just compose-dev-down`.

For production Compose:

1. Prepare host-owned directories and runner quota storage:

   ```bash
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/runtime
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/phase-mounts
   sudo install -d -m 0700 -o <runtime-uid> -g <runtime-gid> /srv/agentics/storage-work
   sudo install -d -m 0755 /srv/agentics/review-checkouts
   ```

2. Create and edit the production env file:

   ```bash
   cp deploy/compose/env/prod.env.example deploy/compose/env/prod.env
   ```

   Production Compose defaults `AGENTICS_CHALLENGES_ROOT` to
   `/app/no-seeded-challenges` so the API does not publish image-bundled sample
   challenges. Set it explicitly only for a controlled seeded-catalog
   deployment.

   Challenge draft validation and publishing run inside the API container. Keep
   a clean, standalone, runtime-readable `agentics-challenges` checkout at
   `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_HOST_ROOT`, then pass
   `AGENTICS_CHALLENGE_REVIEW_REPOSITORY_CONTAINER_ROOT` as the admin
   `repository_path` when validating or publishing drafts.

3. Build and start:

   ```bash
   just compose-prod-build
   sudo just compose-prod-runner-docker-up
   just compose-prod-up
   ```

   Pre-MVP migration history may be squashed. When a deployment picks up a new
   migration baseline, recreate disposable dev/test databases and reset
   production-rehearsal Postgres volumes before starting services. Existing
   databases with old `_sqlx_migrations` rows are incompatible with the new
   baseline checksums.

4. Run production checks and inspect logs:

   ```bash
   just compose-prod-check
   just compose-prod-logs
   ```

5. Stop explicitly:

   ```bash
   just compose-prod-down --runner keep --dry-run
   just compose-prod-down --runner keep
   just compose-prod-down --runner clean --dry-run
   just compose-prod-down --runner clean
   sudo just compose-prod-runner-docker-down
   ```

`--runner keep --dry-run` and `--runner clean --dry-run` never stop services.
`--runner keep` stops Compose services and leaves runner containers alone.
`--runner clean` stops worker services first, removes only production runner
containers with exact Agentics labels, then stops the rest of the Compose stack.
`compose-prod-runner-docker-up` and `compose-prod-runner-docker-down` manage the
dedicated runner Docker daemon at `AGENTICS_DOCKER_SOCKET_PATH`; keep it running
while workers need to create runner containers.

## Edge Assumptions

The production Compose stack does not include a reverse proxy or TLS service.
The MVP edge layer is Cloudflare-managed or otherwise externally managed. It
should:

- Terminate TLS.
- Route public web traffic to the web process.
- Route API traffic to the API process.
- Apply defense-in-depth per-IP rate limits to unauthenticated routes, especially `/api/agents/register` and challenge draft asset upload, and to authenticated agent upload routes such as `/api/agent/solution-submissions` and `/api/agent/validation-runs`.
- Limit request body size at or below backend limits.
- Preserve `Authorization` and `Content-Type` headers.
- Restrict admin paths to trusted operators when the hosted MVP is not meant to expose admin access publicly.

For production Compose, route API paths such as `/healthz`, `/api/*`, and
`/admin/*` to `${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_API_PORT:-3100}`, and
route web traffic to `${AGENTICS_COMPOSE_BIND_IP}:${AGENTICS_WEB_PORT:-3001}`.

## Storage And Backups

Agentics durable storage is object-key based. It stores uploaded solution ZIPs,
runner logs, private asset ZIP overlays, immutable private/public challenge
bundle archives, public statements, and small creator/admin JSON artifacts.
S3 mode stores object keys in the configured bucket and prefix. Local mode maps
the same keys under `AGENTICS_STORAGE_ROOT`, but it is now an explicit opt-in
for narrow local experiments. `AGENTICS_STORAGE_WORK_ROOT` is local scratch
space for packing, unpacking, and S3 downloads; do not put runner quota storage
there.

Use S3 or RustFS-compatible storage for dev, test, and hosted object storage:

```bash
export AGENTICS_STORAGE_BACKEND='s3'
export AGENTICS_S3_BUCKET='agentics'
export AGENTICS_S3_PREFIX='mvp'
export AGENTICS_S3_REGION='us-east-1'
export AGENTICS_S3_ENDPOINT_URL='https://s3.example.internal'
export AGENTICS_S3_FORCE_PATH_STYLE='true'
export AGENTICS_STORAGE_WORK_ROOT='/srv/agentics/storage-work'
```

Dev and production Compose use RustFS as the default single-host S3-compatible
durable storage service. The RustFS credentials are configured as
`AGENTICS_RUSTFS_ACCESS_KEY` and `AGENTICS_RUSTFS_SECRET_KEY` and are mapped to
the AWS SDK environment variables inside app services. The production RustFS
data lives in a Compose named volume; back up that volume together with
Postgres, or switch to external S3 by env before deployment.

For repeated MVP production rehearsals that need to back up migrated challenge
private bundles across stack rebuilds, start the dedicated RustFS backup
compose service:

```bash
cp deploy/compose/env/rustfs-private-backup.env.example deploy/compose/env/rustfs-private-backup.env
just rustfs-private-backup-up
```

The default store listens on `9100` for S3 and `9101` for the RustFS console,
uses `/srv/agentics/private-bundle-backups/rustfs-data` for durable data, and
creates the `migrated-challenge-private-bundles` bucket. This backup store is
not the Agentics durable storage backend. When a production rehearsal starts
with its own RustFS or S3 bucket, copy the needed private bundle objects from
this backup store into the rehearsal storage before reusing previously migrated
challenge metadata:

```bash
just compose-prod-restore-private-bundles
```

The restore command temporarily joins the backup RustFS container to the
production Compose network, then runs a one-shot production Compose service
with access to both private RustFS endpoints. It copies objects into the
production bucket under `AGENTICS_S3_PREFIX` and the logical
`private-bundle-backups/` prefix, skips existing byte-identical objects, and
verifies SHA-256 after each upload. `just rustfs-private-backup-down` stops the
backup container without deleting objects.

Credentials come only from the AWS SDK provider chain, for example environment
variables or an instance profile. Do not store S3 credentials in Agentics DB
rows or challenge specs. Agentics still enforces object-size limits before
durable writes and verifies S3 object length after upload.

For hosted or public MVP operation:

- Back up or replicate the S3 bucket/prefix according to the storage provider's
  policy.
- If you explicitly opt into local mode, put `AGENTICS_STORAGE_ROOT` on a
  persistent volume.
- Back up Postgres and durable object storage together.
- Keep published private runtime bundles and public-only bundles immutable.
- Use stale draft cleanup for unpublished private assets, not manual object
  deletion.
- Run challenge draft cleanup for stale unpublished private assets and stale
  Agentics `_tmp/` objects. `AGENTICS_STORAGE_TMP_OBJECT_GRACE_HOURS` defaults
  to 24 hours. Configure S3 lifecycle cleanup for stale `_tmp/` objects as a
  second line of defense; they are temporary promotion keys and should not be
  retained as durable records.

## Hosted Runner Disk Isolation Decision

The hosted MVP uses a Linux-only storage profile before accepting public
evaluation jobs:

- Use the configured runner Docker daemon behind `AGENTICS_DOCKER_SOCKET_PATH`.
- If Docker writable-layer quotas are required, ensure that daemon's data root
  and storage driver support Docker `storage_opt.size`.
- Use Docker writable-layer quotas for writes that land in the container layer.
- Use separate per-phase loopback filesystem images for writable mounts, with
  root-prepared XFS project-quota slots under each phase mount. This applies to
  solution `setup`, `build`, and `run` phases, and to evaluator `prepare` and
  `score` phases.
- Configure the worker with
  `AGENTICS_RUNNER_SECURITY_PROFILE=production`,
  `AGENTICS_WORKER_ACCELERATORS=gpu`,
  `AGENTICS_WORKER_GPU_PROBE_IMAGE`,
  `AGENTICS_DGX_DOCKER_DATA_ROOT`,
  `AGENTICS_RUNNER_WRITABLE_STORAGE_MODE=xfs-project-quota-slots`,
  `AGENTICS_RUNNER_RUNTIME_ROOT`, `AGENTICS_RUNNER_PHASE_MOUNT_ROOT`,
  `AGENTICS_RUNNER_WRITABLE_SLOT_CLASSES_MB`, and
  `AGENTICS_RUNNER_DOCKER_LAYER_QUOTA=true`.
- Gate strict probes with `AGENTICS_HOST_PROBE_MODE=off|warn|require`, not the
  generic `CI` variable. Production runner security also requires
  `AGENTICS_HOST_PROBE_MODE=require`, `AGENTICS_DGX_RUN_MUTATING_PROBES=true`,
  and digest-pinned images.
- Keep local Compose development permissive. The strict storage probe belongs to
  hosted Linux staging and DGX-hosted workers.

This combination is chosen because Docker writable-layer quotas and bounded
mounts protect different paths. `storage_opt.size` covers container-layer writes
such as package caches or accidental writes outside mounts. Quota slots under
the separate loop images cover runner-owned writable mounts such as workspaces,
`/io`, `/setup`, `/output`, home, and temporary directories. The worker
chooses the smallest configured slot class that can satisfy the effective phase
`disk_limit_mb`; align resource profiles to slot classes when an exact hard
phase limit is required. Both layers are needed for a hard writable disk
boundary across all runner phases.

## Rollback

The safe rollback path is:

1. Stop web, worker, and API.
2. Restore the previous API and worker binaries.
3. Restore the previous web build.
4. Restart API, then worker, then web.
5. Run `/healthz`, `/admin/capacity`, `/admin/service-heartbeats`, and one CLI status/list command.

Do not roll back database migrations by hand during MVP rehearsals unless the
migration is explicitly reversible and the storage snapshot is from the same
point in time. The project does not maintain down migrations; rollback is a
database and durable-storage snapshot restore.

For production Compose, use `just compose-prod-down --runner keep` for ordinary
binary or image rollback when running evaluations can be allowed to reconcile
later. Use `just compose-prod-down --runner clean` only when the operator has
chosen to terminate matching production runner containers. Dry-run forms do not
stop services.

## Verification

Run:

```bash
agentics-check-local-mvp
```

For production Compose, run:

```bash
just compose-prod-check
```

Then perform a CLI smoke path using the root `README.md` submitter flow or
`skills/agentics-cli-workflow/SKILL.md`.

## DGX Spark Hosted Profile

DGX Spark host preparation is verified separately because it adds ARM64, Docker
GPU device access, XFS quota setup, and DGX OS lifecycle assumptions. See the
DGX Spark milestones in `docs/milestones/en.md`.

The first host inventory is summarized in `docs/dgx-spark/en.md`. The
repeatable check is:

```bash
agentics-check-dgx-spark-host
```

This check is Linux-gated and reports Docker/NVIDIA GPU blockers without
mutating host state. The current inventory confirms OS, GPU, NVIDIA toolkit,
storage, XFS tooling, loopback tooling, default Docker GPU smoke behavior, and
the configured host Docker socket.

DGX Spark host preparation and smoke evidence are recorded in
`docs/dgx-spark/en.md`, with Linux-gated storage/profile binaries in
`agentics-ops`.

Use NVIDIA's DGX Spark documentation as the operational source of truth:

- [NVIDIA DGX Spark product page](https://marketplace.nvidia.com/en-us/enterprise/personal-ai-supercomputers/dgx-spark/)
- [NVIDIA DGX Spark documentation](https://docs.nvidia.com/dgx/dgx-spark/)
- [NVIDIA Container Runtime for Docker on DGX Spark](https://docs.nvidia.com/dgx/dgx-spark/nvidia-container-runtime-for-docker.html)
